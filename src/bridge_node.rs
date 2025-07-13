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
use crate::mesh_validation::{MeshTransaction, TokenType};

/// Bridge node for settling mesh transactions on Ronin blockchain
pub struct BridgeNode {
    ronin_client: Arc<RoninClient>,
    transaction_queue: Arc<OfflineTransactionQueue>,
    settlement_batch: Arc<RwLock<Vec<MeshTransaction>>>,
    settlement_stats: Arc<RwLock<SettlementStats>>,
    bridge_events: mpsc::Sender<BridgeEvent>,
}

#[derive(Debug, Clone)]
pub enum BridgeEvent {
    ConnectivityRestored,
    MeshTransactionSettled(Uuid),
    SettlementFailed(Uuid, String),
    BatchSettlementCompleted(usize),
}

#[derive(Debug, Default, Clone)]
pub struct SettlementStats {
    pub total_settled: u64,
    pub successful_settlements: u64,
    pub failed_settlements: u64,
    pub last_settlement_time: Option<SystemTime>,
    pub pending_settlements: usize,
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
        }
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
}

impl Clone for BridgeNode {
    fn clone(&self) -> Self {
        Self {
            ronin_client: Arc::clone(&self.ronin_client),
            transaction_queue: Arc::clone(&self.transaction_queue),
            settlement_batch: Arc::clone(&self.settlement_batch),
            settlement_stats: Arc::clone(&self.settlement_stats),
            bridge_events: self.bridge_events.clone(),
        }
    }
}
