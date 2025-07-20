// src/bridge_node.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock};
use std::sync::Arc;
use uuid::Uuid;
use anyhow::Result;
use crate::web3::{RoninClient, RoninTransaction, TransactionStatus};
use crate::transaction_queue::OfflineTransactionQueue;
use crate::mesh_validation::{MeshTransaction, TokenType, MeshValidator};
use crate::aura_protocol::{AuraProtocolClient, ValidationTask, ContractEvent};
use crate::contract_integration::ContractIntegration;

/// Bridge node for settling mesh transactions and handling contract interactions
pub struct BridgeNode {
    ronin_client: Arc<RoninClient>,
    transaction_queue: Arc<OfflineTransactionQueue>,
    settlement_batch: Arc<RwLock<Vec<MeshTransaction>>>,
    settlement_stats: Arc<RwLock<SettlementStats>>,
    bridge_events: mpsc::Sender<BridgeEvent>,
    // Contract integration
    aura_protocol_client: Option<Arc<AuraProtocolClient>>,
    contract_integration: Option<Arc<ContractIntegration>>,
    mesh_validator: Option<Arc<MeshValidator>>,
    contract_task_queue: Arc<RwLock<Vec<ValidationTask>>>,
}

#[derive(Debug, Clone)]
pub enum BridgeEvent {
    ConnectivityRestored,
    MeshTransactionSettled(Uuid),
    SettlementFailed(Uuid, String),
    BatchSettlementCompleted(usize),
    ContractTaskReceived(u64),
    ContractTaskProcessed(u64),
    ContractResultSubmitted(u64, String),
    ContractEventProcessed(String),
}

#[derive(Debug, Default, Clone)]
pub struct SettlementStats {
    pub total_settled: u64,
    pub successful_settlements: u64,
    pub failed_settlements: u64,
    pub last_settlement_time: Option<SystemTime>,
    pub pending_settlements: usize,
}

#[derive(Debug, Default, Clone)]
pub struct ContractTaskStats {
    pub active_tasks: usize,
    pub total_processed: u64,
    pub successful_submissions: u64,
    pub failed_submissions: u64,
}

/// Settlement transaction that aggregates mesh transactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementTransaction {
    pub id: Uuid,
    pub mesh_transactions: Vec<Uuid>,
    pub net_transfers: Vec<NetTransfer>,
    pub created_at: SystemTime,
    pub ronin_tx_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetTransfer {
    pub from_address: String,
    pub to_address: String,
    pub token_type: TokenType,
    pub net_amount: i64, // Can be negative for outgoing transfers
}

impl BridgeNode {
    /// Create a new bridge node
    pub fn new(
        ronin_client: Arc<RoninClient>,
        transaction_queue: Arc<OfflineTransactionQueue>,
    ) -> Self {
        let (bridge_events, _) = mpsc::channel(100);

        Self {
            ronin_client,
            transaction_queue,
            settlement_batch: Arc::new(RwLock::new(Vec::new())),
            settlement_stats: Arc::new(RwLock::new(SettlementStats::default())),
            bridge_events,
            aura_protocol_client: None,
            contract_integration: None,
            mesh_validator: None,
            contract_task_queue: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Set the AuraProtocol client for contract interactions
    pub fn set_aura_protocol_client(&mut self, client: Arc<AuraProtocolClient>) {
        self.aura_protocol_client = Some(client);
    }

    /// Set the contract integration for AuraProtocol interactions
    pub fn set_contract_integration(&mut self, integration: Arc<ContractIntegration>) {
        self.contract_integration = Some(integration);
    }

    /// Set the mesh validator for processing contract tasks
    pub fn set_mesh_validator(&mut self, validator: Arc<MeshValidator>) {
        self.mesh_validator = Some(validator);
    }

    /// Start the bridge service
    pub async fn start_bridge_service(&self) -> mpsc::Receiver<BridgeEvent> {
        let (tx, rx) = mpsc::channel(100);
        
        let bridge_node = self.clone();
        tokio::spawn(async move {
            bridge_node.bridge_loop(tx).await;
        });
        
        rx
    }

    /// Main bridge loop
    async fn bridge_loop(&self, event_tx: mpsc::Sender<BridgeEvent>) {
        let mut connectivity_check_interval = tokio::time::interval(Duration::from_secs(30));
        let mut settlement_interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
        let mut contract_task_interval = tokio::time::interval(Duration::from_secs(60)); // 1 minute

        // Start contract event monitoring if available
        let mut contract_events_rx = if let Some(client) = &self.aura_protocol_client {
            Some(client.start_event_monitor().await)
        } else {
            None
        };

        loop {
            tokio::select! {
                _ = connectivity_check_interval.tick() => {
                    if self.ronin_client.check_connectivity().await {
                        let _ = event_tx.send(BridgeEvent::ConnectivityRestored).await;
                    }
                }

                _ = settlement_interval.tick() => {
                    if self.ronin_client.check_connectivity().await {
                        if let Err(e) = self.process_settlement_batch().await {
                            let error_msg = e.to_string();
                            tracing::error!("Settlement batch processing failed: {}", error_msg);
                        }
                    }
                }

                _ = contract_task_interval.tick() => {
                    if self.ronin_client.check_connectivity().await {
                        if let Err(e) = self.process_contract_tasks().await {
                            tracing::error!("Contract task processing failed: {}", e);
                        }
                    }
                }

                contract_event = Self::recv_contract_event(&mut contract_events_rx) => {
                    if let Some(event) = contract_event {
                        if let Err(e) = self.handle_contract_event(event, &event_tx).await {
                            tracing::error!("Failed to handle contract event: {}", e);
                        }
                    }
                }
            }
        }
    }

    /// Helper function to receive contract events
    async fn recv_contract_event(
        contract_events_rx: &mut Option<mpsc::Receiver<ContractEvent>>
    ) -> Option<ContractEvent> {
        match contract_events_rx {
            Some(rx) => rx.recv().await,
            None => {
                // Return None immediately if no receiver
                None
            }
        }
    }

    /// Add mesh transaction to settlement batch
    pub async fn add_to_settlement_batch(&self, mesh_tx: MeshTransaction) -> Result<()> {
        let mut batch = self.settlement_batch.write().await;
        batch.push(mesh_tx);
        
        // Process batch if it reaches threshold
        if batch.len() >= 10 {
            drop(batch); // Release lock before processing
            self.process_settlement_batch().await?;
        }
        
        Ok(())
    }

    /// Process settlement batch
    async fn process_settlement_batch(&self) -> Result<()> {
        let batch = {
            let mut settlement_batch = self.settlement_batch.write().await;
            if settlement_batch.is_empty() {
                return Ok(());
            }
            std::mem::take(&mut *settlement_batch)
        };

        tracing::info!("Processing settlement batch with {} transactions", batch.len());

        // Calculate net transfers
        let net_transfers = self.calculate_net_transfers(&batch).await?;
        
        // Create settlement transaction
        let settlement_tx = SettlementTransaction {
            id: Uuid::new_v4(),
            mesh_transactions: batch.iter().map(|tx| tx.id).collect(),
            net_transfers,
            created_at: SystemTime::now(),
            ronin_tx_hash: None,
        };

        // Submit to Ronin blockchain
        match self.submit_settlement_to_ronin(settlement_tx).await {
            Ok(tx_count) => {
                let _ = self.bridge_events.send(BridgeEvent::BatchSettlementCompleted(tx_count)).await;
                
                // Update stats
                let mut stats = self.settlement_stats.write().await;
                stats.total_settled += tx_count as u64;
                stats.successful_settlements += 1;
                stats.last_settlement_time = Some(SystemTime::now());
                stats.pending_settlements = 0;
            }
            Err(e) => {
                let error_msg = e.to_string();
                drop(e); // Drop the error before any await
                tracing::error!("Settlement submission failed: {}", error_msg);

                // Return transactions to batch for retry
                let mut settlement_batch = self.settlement_batch.write().await;
                settlement_batch.extend(batch);

                let mut stats = self.settlement_stats.write().await;
                stats.failed_settlements += 1;
            }
        }

        Ok(())
    }

    /// Calculate net transfers from mesh transactions
    async fn calculate_net_transfers(&self, transactions: &[MeshTransaction]) -> Result<Vec<NetTransfer>> {
        let mut net_balances: HashMap<(String, TokenType), i64> = HashMap::new();

        // Calculate net balance changes for each address and token type
        for tx in transactions {
            let key_from = (tx.from_address.clone(), tx.token_type.clone());
            let key_to = (tx.to_address.clone(), tx.token_type.clone());

            // Subtract from sender
            *net_balances.entry(key_from).or_insert(0) -= tx.amount as i64;
            
            // Add to receiver
            *net_balances.entry(key_to).or_insert(0) += tx.amount as i64;
        }

        // Convert to net transfers (only include non-zero balances)
        let mut net_transfers = Vec::new();
        for ((address, token_type), net_amount) in net_balances {
            if net_amount != 0 {
                // Determine if this is a net sender or receiver
                if net_amount > 0 {
                    // Net receiver - create incoming transfer
                    net_transfers.push(NetTransfer {
                        from_address: "mesh_settlement".to_string(), // Special address for mesh settlements
                        to_address: address,
                        token_type,
                        net_amount,
                    });
                } else {
                    // Net sender - create outgoing transfer
                    net_transfers.push(NetTransfer {
                        from_address: address,
                        to_address: "mesh_settlement".to_string(),
                        token_type,
                        net_amount: -net_amount, // Make positive for transfer amount
                    });
                }
            }
        }

        Ok(net_transfers)
    }

    /// Submit settlement to Ronin blockchain
    async fn submit_settlement_to_ronin(&self, settlement: SettlementTransaction) -> Result<usize> {
        let mut submitted_count = 0;

        for net_transfer in &settlement.net_transfers {
            // Skip zero-amount transfers
            if net_transfer.net_amount <= 0 {
                continue;
            }

            // Create Ronin transaction for this net transfer
            let ronin_tx = self.create_ronin_transaction_from_transfer(net_transfer).await?;
            
            // Submit to transaction queue for processing
            match self.transaction_queue.add_transaction(
                crate::transaction_queue::TransactionType::Ronin(ronin_tx.clone()),
                crate::transaction_queue::TransactionPriority::High,
                vec![], // No dependencies for settlement transactions
            ).await {
                Ok(tx_id) => {
                    tracing::info!("Settlement transaction {} queued for Ronin submission", tx_id);
                    submitted_count += 1;
                    
                    let _ = self.bridge_events.send(BridgeEvent::MeshTransactionSettled(tx_id)).await;
                }
                Err(e) => {
                    tracing::error!("Failed to queue settlement transaction: {}", e);
                    let _ = self.bridge_events.send(BridgeEvent::SettlementFailed(settlement.id, e.to_string())).await;
                }
            }
        }

        Ok(submitted_count)
    }

    /// Create Ronin transaction from net transfer
    async fn create_ronin_transaction_from_transfer(&self, transfer: &NetTransfer) -> Result<RoninTransaction> {
        let tx = RoninTransaction {
            id: Uuid::new_v4(),
            from: transfer.from_address.clone(),
            to: transfer.to_address.clone(),
            value: transfer.net_amount as u64,
            gas_price: 20_000_000_000, // 20 gwei
            gas_limit: match &transfer.token_type {
                TokenType::RON => 21_000,
                TokenType::SLP | TokenType::AXS => 65_000, // ERC-20 transfer
                TokenType::NFT { .. } => 100_000, // ERC-721 transfer
            },
            nonce: 0, // Will be set by sync manager
            data: self.encode_transfer_data(transfer).await?,
            chain_id: 2020, // Ronin mainnet
            created_at: SystemTime::now(),
            status: TransactionStatus::Pending,
        };

        Ok(tx)
    }

    /// Encode transfer data for different token types
    async fn encode_transfer_data(&self, transfer: &NetTransfer) -> Result<Vec<u8>> {
        match &transfer.token_type {
            TokenType::RON => {
                // Simple RON transfer - no data needed
                Ok(vec![])
            }
            TokenType::SLP => {
                // ERC-20 transfer function call
                // transfer(address to, uint256 amount)
                Ok(format!("transfer({},{})", transfer.to_address, transfer.net_amount).into_bytes())
            }
            TokenType::AXS => {
                // ERC-20 transfer function call
                Ok(format!("transfer({},{})", transfer.to_address, transfer.net_amount).into_bytes())
            }
            TokenType::NFT { contract_address: _, token_id } => {
                // ERC-721 transferFrom function call
                // transferFrom(address from, address to, uint256 tokenId)
                Ok(format!("transferFrom({},{},{})", transfer.from_address, transfer.to_address, token_id).into_bytes())
            }
        }
    }

    /// Get settlement statistics
    pub async fn get_settlement_stats(&self) -> SettlementStats {
        self.settlement_stats.read().await.clone()
    }

    /// Force settlement of current batch
    pub async fn force_settlement(&self) -> Result<()> {
        if self.ronin_client.check_connectivity().await {
            self.process_settlement_batch().await
        } else {
            Err(anyhow::anyhow!("No connectivity to Ronin network"))
        }
    }

    // === Contract Task Processing Methods ===

    /// Process contract validation tasks
    async fn process_contract_tasks(&self) -> Result<()> {
        let client = match &self.aura_protocol_client {
            Some(client) => client,
            None => return Ok(()), // No contract client configured
        };

        let validator = match &self.mesh_validator {
            Some(validator) => validator,
            None => return Ok(()), // No mesh validator configured
        };

        // Fetch open tasks from the contract
        let open_tasks = client.get_open_tasks().await?;

        for task in open_tasks {
            // Check if this node is part of the worker cohort
            if client.is_node_in_cohort(&task) {
                // Check if we're already processing this task
                let active_tasks = validator.get_active_contract_tasks().await;
                if !active_tasks.iter().any(|t| t.id == task.id) {
                    tracing::info!("Processing new contract task: {}", task.id);

                    // Process the task through mesh validation
                    let task_success = match validator.process_contract_task(task.clone()).await {
                        Ok(()) => true,
                        Err(e) => {
                            tracing::error!("Failed to process contract task {}: {}", task.id, e);
                            false
                        }
                    };

                    if task_success {
                        let _ = self.bridge_events.send(BridgeEvent::ContractTaskReceived(task.id)).await;
                    }
                }
            }
        }

        // Check for completed tasks ready for submission
        self.submit_completed_contract_tasks().await?;

        Ok(())
    }

    /// Submit completed contract task results
    async fn submit_completed_contract_tasks(&self) -> Result<()> {
        let client = match &self.aura_protocol_client {
            Some(client) => client,
            None => return Ok(()),
        };

        let validator = match &self.mesh_validator {
            Some(validator) => validator,
            None => return Ok(()),
        };

        let active_tasks = validator.get_active_contract_tasks().await;

        for task in active_tasks {
            // Check if we have a completed result ready for submission
            if let Some(result) = validator.get_contract_task_result(task.id).await {
                tracing::info!("Submitting result for contract task: {}", task.id);

                match client.submit_result(result).await {
                    Ok(tx_hash) => {
                        tracing::info!("Contract task {} result submitted: {}", task.id, tx_hash);

                        // Remove the completed task
                        validator.remove_contract_task(task.id).await;

                        let _ = self.bridge_events.send(BridgeEvent::ContractResultSubmitted(task.id, tx_hash)).await;
                        let _ = self.bridge_events.send(BridgeEvent::ContractTaskProcessed(task.id)).await;
                    }
                    Err(e) => {
                        tracing::error!("Failed to submit result for task {}: {}", task.id, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle contract events
    async fn handle_contract_event(
        &self,
        event: ContractEvent,
        event_tx: &mpsc::Sender<BridgeEvent>,
    ) -> Result<()> {
        match event {
            ContractEvent::TaskCreated { task_id, requester, bounty, task_data: _, deadline: _ } => {
                tracing::info!("New contract task created: {} by {} with bounty {}",
                    task_id, requester, bounty);

                let _ = event_tx.send(BridgeEvent::ContractEventProcessed(
                    format!("TaskCreated:{}", task_id)
                )).await;
            }

            ContractEvent::TaskCompleted { task_id } => {
                tracing::info!("Contract task completed: {}", task_id);

                // Clean up any local state for this task
                if let Some(validator) = &self.mesh_validator {
                    validator.remove_contract_task(task_id).await;
                }

                let _ = event_tx.send(BridgeEvent::ContractEventProcessed(
                    format!("TaskCompleted:{}", task_id)
                )).await;
            }

            ContractEvent::TaskFailed { task_id } => {
                tracing::warn!("Contract task failed: {}", task_id);

                // Clean up any local state for this task
                if let Some(validator) = &self.mesh_validator {
                    validator.remove_contract_task(task_id).await;
                }

                let _ = event_tx.send(BridgeEvent::ContractEventProcessed(
                    format!("TaskFailed:{}", task_id)
                )).await;
            }

            ContractEvent::TaskCancelled { task_id, reason } => {
                tracing::info!("Contract task cancelled: {} - {}", task_id, reason);

                // Clean up any local state for this task
                if let Some(validator) = &self.mesh_validator {
                    validator.remove_contract_task(task_id).await;
                }

                let _ = event_tx.send(BridgeEvent::ContractEventProcessed(
                    format!("TaskCancelled:{}", task_id)
                )).await;
            }
        }

        Ok(())
    }

    /// Get contract task processing statistics
    pub async fn get_contract_task_stats(&self) -> Option<ContractTaskStats> {
        if let Some(validator) = &self.mesh_validator {
            let active_tasks = validator.get_active_contract_tasks().await;

            Some(ContractTaskStats {
                active_tasks: active_tasks.len(),
                total_processed: 0, // Would need to track this
                successful_submissions: 0, // Would need to track this
                failed_submissions: 0, // Would need to track this
            })
        } else {
            None
        }
    }



    /// Submit contract task results
    pub async fn submit_contract_results(&self) -> Result<()> {
        if let Some(contract_integration) = &self.contract_integration {
            let pending_results = contract_integration.get_pending_results().await;

            for result in pending_results {
                tracing::info!("Submitting contract result for task {} from bridge", result.task_id);

                match contract_integration.submit_task_result(result.clone()).await {
                    Ok(tx_hash) => {
                        // Remove from pending
                        contract_integration.remove_pending_result(result.task_id).await?;

                        // Send bridge event
                        let _ = self.bridge_events.send(
                            BridgeEvent::ContractResultSubmitted(result.task_id, tx_hash)
                        ).await;
                    }
                    Err(e) => {
                        tracing::error!("Failed to submit contract result for task {}: {}", result.task_id, e);
                    }
                }
            }
        }

        Ok(())
    }



    /// Get contract integration statistics
    pub async fn get_contract_stats(&self) -> Option<HashMap<String, u64>> {
        if let Some(contract_integration) = &self.contract_integration {
            let stats = contract_integration.get_stats().await;
            let mut result = HashMap::new();
            result.insert("active_tasks".to_string(), stats.active_tasks as u64);
            result.insert("pending_results".to_string(), stats.pending_results as u64);
            result.insert("total_processed".to_string(), stats.total_tasks_processed);
            result.insert("successful_submissions".to_string(), stats.successful_submissions);
            result.insert("failed_submissions".to_string(), stats.failed_submissions);
            Some(result)
        } else {
            None
        }
    }
}

impl Clone for BridgeNode {
    fn clone(&self) -> Self {
        Self {
            ronin_client: Arc::clone(&self.ronin_client),
            transaction_queue: Arc::clone(&self.transaction_queue),
            settlement_batch: Arc::clone(&self.settlement_batch),
            settlement_stats: Arc::clone(&self.settlement_stats),
            bridge_events: self.bridge_events.clone(),
            aura_protocol_client: self.aura_protocol_client.clone(),
            contract_integration: self.contract_integration.clone(),
            mesh_validator: self.mesh_validator.clone(),
            contract_task_queue: Arc::clone(&self.contract_task_queue),
        }
    }
}
