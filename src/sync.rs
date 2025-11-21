// src/sync.rs

use tokio::sync::{mpsc, RwLock};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;
use crate::web3::{RoninClient, RoninTransaction, TransactionStatus};
use crate::transaction_queue::{OfflineTransactionQueue, TransactionType};
use crate::config::RoninConfig;

/// Web3 synchronization manager for replaying offline transactions
pub struct Web3SyncManager {
    ronin_client: RoninClient,
    transaction_queue: Arc<OfflineTransactionQueue>,
    config: RoninConfig,
    is_syncing: Arc<RwLock<bool>>,
    sync_stats: Arc<RwLock<SyncStats>>,
}

#[derive(Debug, Default, Clone)]
pub struct SyncStats {
    pub total_synced: u64,
    pub successful_syncs: u64,
    pub failed_syncs: u64,
    pub last_sync_time: Option<std::time::SystemTime>,
    pub sync_duration_ms: u64,
    pub pending_transactions: usize,
}

impl Web3SyncManager {
    /// Create a new Web3 sync manager
    pub fn new(
        config: RoninConfig,
        transaction_queue: Arc<OfflineTransactionQueue>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let ronin_client = RoninClient::new(config.clone())?;

        Ok(Self {
            ronin_client,
            transaction_queue,
            config,
            is_syncing: Arc::new(RwLock::new(false)),
            sync_stats: Arc::new(RwLock::new(SyncStats::default())),
        })
    }

    /// Start the synchronization service
    pub async fn start_sync_service(&self) -> mpsc::Receiver<SyncEvent> {
        let (tx, rx) = mpsc::channel(100);
        
        let sync_manager = self.clone();
        tokio::spawn(async move {
            sync_manager.sync_loop(tx).await;
        });
        
        rx
    }

    /// Main synchronization loop
    async fn sync_loop(&self, event_tx: mpsc::Sender<SyncEvent>) {
        let mut interval = tokio::time::interval(
            Duration::from_secs(self.config.sync_retry_interval_secs)
        );
        
        loop {
            interval.tick().await;
            
            // Check if we're already syncing
            {
                let is_syncing = self.is_syncing.read().await;
                if *is_syncing {
                    continue;
                }
            }
            
            // Check connectivity to Ronin network
            if !self.ronin_client.check_connectivity().await {
                tracing::debug!("No connectivity to Ronin network, skipping sync");
                continue;
            }
            
            // Start synchronization
            let _ = event_tx.send(SyncEvent::SyncStarted).await;
            
            let sync_result = self.perform_sync(&event_tx).await;
            match sync_result {
                Err(e) => {
                    let error_msg = e.to_string();
                    tracing::error!("Sync failed: {}", error_msg);
                    let _ = event_tx.send(SyncEvent::SyncFailed(error_msg)).await;
                }
                Ok(_) => {
                    let _ = event_tx.send(SyncEvent::SyncCompleted).await;
                }
            }

        }
    }

    /// Perform the actual synchronization
    async fn perform_sync(&self, event_tx: &mpsc::Sender<SyncEvent>) -> Result<(), String> {
        let start_time = std::time::Instant::now();
        
        // Set syncing flag
        {
            let mut is_syncing = self.is_syncing.write().await;
            *is_syncing = true;
        }
        
        let mut synced_count = 0;
        let mut failed_count = 0;
        
        // Process transactions from the queue
        while let Some(queued_tx) = self.transaction_queue.get_next_transaction().await {
            let tx_id = queued_tx.id;
            
            // Extract result and error message in a block scope to ensure Send trait
            let (is_success, error_msg_opt) = {
                let sync_result = self.sync_transaction(queued_tx, event_tx).await;
                match sync_result {
                    Ok(_) => (true, None),
                    Err(e) => {
                        // Convert error to String immediately to ensure Send trait
                        (false, Some(e.to_string()))
                    }
                }
            };
            
            if is_success {
                synced_count += 1;
                tracing::debug!("Successfully synced transaction {}", tx_id);
                // Emit per-transaction success event
                let _ = event_tx.send(SyncEvent::TransactionSynced(tx_id)).await;
            } else {
                failed_count += 1;
                let error_msg = error_msg_opt.unwrap_or_else(|| "Unknown error".to_string());
                tracing::warn!("Failed to sync transaction {}: {}", tx_id, error_msg);
                // Emit per-transaction failure event
                let _ = event_tx.send(SyncEvent::TransactionFailed(tx_id, error_msg)).await;
            }
            
            // Add small delay between transactions to avoid overwhelming the network
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        // Update sync statistics
        {
            let mut stats = self.sync_stats.write().await;
            stats.total_synced += synced_count;
            stats.successful_syncs += synced_count;
            stats.failed_syncs += failed_count;
            stats.last_sync_time = Some(std::time::SystemTime::now());
            stats.sync_duration_ms = start_time.elapsed().as_millis() as u64;
            stats.pending_transactions = self.transaction_queue.get_stats().await.queued;
        }
        
        // Clear syncing flag
        {
            let mut is_syncing = self.is_syncing.write().await;
            *is_syncing = false;
        }
        
        tracing::info!(
            "Sync completed: {} successful, {} failed in {}ms",
            synced_count,
            failed_count,
            start_time.elapsed().as_millis()
        );
        
        Ok(())
    }

    /// Synchronize a single transaction
    async fn sync_transaction(
        &self,
        queued_tx: crate::transaction_queue::QueuedTransaction,
        _event_tx: &mpsc::Sender<SyncEvent>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Note: Events are emitted in perform_sync after sync_transaction completes
        // This allows us to capture both success and failure cases with proper error messages
        match queued_tx.transaction {
            TransactionType::Ronin(ronin_tx) => {
                self.sync_ronin_transaction(queued_tx.id, ronin_tx).await
            }
            TransactionType::Utility(utility_tx) => {
                self.sync_utility_transaction(queued_tx.id, utility_tx).await
            }
            TransactionType::Nft(nft_op) => {
                self.sync_nft_operation(queued_tx.id, nft_op).await
            }
        }
    }

    /// Sync a Ronin blockchain transaction
    async fn sync_ronin_transaction(
        &self,
        queue_id: Uuid,
        mut tx: RoninTransaction,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Update nonce if needed
        if tx.nonce == 0 {
            tx.nonce = self.ronin_client.get_nonce(&tx.from).await?;
        }
        
        // Update gas price if needed
        let current_gas_price = self.ronin_client.get_gas_price().await?;
        if tx.gas_price < current_gas_price {
            tx.gas_price = current_gas_price;
        }
        
        // Submit transaction
        match self.ronin_client.submit_transaction(&tx).await {
            Ok(tx_hash) => {
                tracing::info!("Transaction submitted with hash: {}", tx_hash);
                
                // Wait for confirmation
                let mut attempts = 0;
                let max_attempts = 30; // 5 minutes with 10-second intervals
                
                loop {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    
                    match self.ronin_client.get_transaction_status(&tx_hash).await {
                        Ok(TransactionStatus::Confirmed) => {
                            self.transaction_queue.mark_completed(queue_id).await?;
                            return Ok(());
                        }
                        Ok(TransactionStatus::Failed(reason)) => {
                            self.transaction_queue.mark_failed(queue_id, reason).await?;
                            return Err("Transaction failed on chain".into());
                        }
                        Ok(_) => {
                            // Still pending, continue waiting
                        }
                        Err(e) => {
                            tracing::warn!("Error checking transaction status: {}", e);
                        }
                    }
                    
                    attempts += 1;
                    if attempts >= max_attempts {
                        return Err("Transaction confirmation timeout".into());
                    }
                }
            }
            Err(e) => {
                self.transaction_queue.mark_failed(queue_id, e.clone()).await?;
                Err(e.into())
            }
        }
    }

    /// Sync a utility transaction (convert to appropriate Ronin transactions)
    async fn sync_utility_transaction(
        &self,
        queue_id: Uuid,
        utility_tx: crate::web3::UtilityTransaction,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Convert utility transactions to appropriate Ronin transactions
        // This handles mesh settlement and other utility operations

        tracing::info!("Syncing utility transaction: {:?}", utility_tx);
        
        // For now, mark as completed (placeholder)
        self.transaction_queue.mark_completed(queue_id).await?;
        Ok(())
    }

    /// Sync an NFT operation
    async fn sync_nft_operation(
        &self,
        queue_id: Uuid,
        nft_op: crate::web3::NftOperation,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Convert NFT operations to appropriate Ronin transactions
        // This creates ERC-721 or ERC-1155 transaction calls
        
        tracing::info!("Converting NFT operation {:?} to Ronin transaction", nft_op.operation_type);
        
        // Get current nonce and gas price for the transaction
        let nonce = self.ronin_client.get_nonce(&nft_op.from).await?;
        let gas_price = self.ronin_client.get_gas_price().await?;
        
        // Capture operation type for logging before moving nft_op
        let operation_type = nft_op.operation_type.clone();
        
        // Convert NFT operation to appropriate Ronin transaction based on operation type
        let ronin_tx = match nft_op.operation_type {
            crate::web3::NftOperationType::Transfer => {
                // Use existing utility function for NFT transfers
                crate::web3::utils::create_nft_transfer(
                    nft_op.contract_address,
                    nft_op.from,
                    nft_op.to,
                    nft_op.token_id,
                    nonce,
                    gas_price,
                    self.ronin_client.get_chain_id(),
                )
            }
            crate::web3::NftOperationType::Mint => {
                // Create mint transaction (contract-specific implementation)
                self.create_nft_mint_transaction(nft_op, nonce, gas_price)?
            }
            crate::web3::NftOperationType::Burn => {
                // Create burn transaction
                self.create_nft_burn_transaction(nft_op, nonce, gas_price)?
            }
            crate::web3::NftOperationType::Approve => {
                // Create approve transaction
                self.create_nft_approve_transaction(nft_op, nonce, gas_price)?
            }
            crate::web3::NftOperationType::SetApprovalForAll => {
                // Create setApprovalForAll transaction
                self.create_nft_approval_for_all_transaction(nft_op, nonce, gas_price)?
            }
        };
        
        tracing::info!("Created Ronin transaction {} for NFT operation {:?}", 
            ronin_tx.id, operation_type);
        
        // Sync the converted Ronin transaction
        self.sync_ronin_transaction(queue_id, ronin_tx).await
    }

    /// Create NFT mint transaction
    fn create_nft_mint_transaction(
        &self,
        nft_op: crate::web3::NftOperation,
        nonce: u64,
        gas_price: u64,
    ) -> Result<RoninTransaction, Box<dyn std::error::Error>> {
        // Create mint transaction call data using proper ABI encoding
        let call_data = crate::web3::utils::encode_mint(&nft_op.to, nft_op.token_id);
        
        Ok(RoninTransaction {
            id: Uuid::new_v4(),
            from: nft_op.from,
            to: nft_op.contract_address,
            value: 0,
            gas_price,
            gas_limit: 150000, // Higher gas for minting
            nonce,
            data: call_data,
            chain_id: self.ronin_client.get_chain_id(),
            created_at: std::time::SystemTime::now(),
            status: TransactionStatus::Pending,
        })
    }

    /// Create NFT burn transaction
    fn create_nft_burn_transaction(
        &self,
        nft_op: crate::web3::NftOperation,
        nonce: u64,
        gas_price: u64,
    ) -> Result<RoninTransaction, Box<dyn std::error::Error>> {
        // Create burn transaction call data using proper ABI encoding
        let call_data = crate::web3::utils::encode_burn(nft_op.token_id);
        
        Ok(RoninTransaction {
            id: Uuid::new_v4(),
            from: nft_op.from,
            to: nft_op.contract_address,
            value: 0,
            gas_price,
            gas_limit: 120000, // Gas for burning
            nonce,
            data: call_data,
            chain_id: self.ronin_client.get_chain_id(),
            created_at: std::time::SystemTime::now(),
            status: TransactionStatus::Pending,
        })
    }

    /// Create NFT approve transaction
    fn create_nft_approve_transaction(
        &self,
        nft_op: crate::web3::NftOperation,
        nonce: u64,
        gas_price: u64,
    ) -> Result<RoninTransaction, Box<dyn std::error::Error>> {
        // Create approve transaction call data using proper ABI encoding
        let call_data = crate::web3::utils::encode_approve(&nft_op.to, nft_op.token_id);
        
        Ok(RoninTransaction {
            id: Uuid::new_v4(),
            from: nft_op.from,
            to: nft_op.contract_address,
            value: 0,
            gas_price,
            gas_limit: 80000, // Gas for approval
            nonce,
            data: call_data,
            chain_id: self.ronin_client.get_chain_id(),
            created_at: std::time::SystemTime::now(),
            status: TransactionStatus::Pending,
        })
    }

    /// Create NFT setApprovalForAll transaction
    fn create_nft_approval_for_all_transaction(
        &self,
        nft_op: crate::web3::NftOperation,
        nonce: u64,
        gas_price: u64,
    ) -> Result<RoninTransaction, Box<dyn std::error::Error>> {
        // Create setApprovalForAll transaction call data using proper ABI encoding
        let call_data = crate::web3::utils::encode_set_approval_for_all(&nft_op.to, true);
        
        Ok(RoninTransaction {
            id: Uuid::new_v4(),
            from: nft_op.from,
            to: nft_op.contract_address,
            value: 0,
            gas_price,
            gas_limit: 90000, // Gas for approval for all
            nonce,
            data: call_data,
            chain_id: self.ronin_client.get_chain_id(),
            created_at: std::time::SystemTime::now(),
            status: TransactionStatus::Pending,
        })
    }

    /// Get current sync statistics
    pub async fn get_sync_stats(&self) -> SyncStats {
        self.sync_stats.read().await.clone()
    }

    /// Check if currently syncing
    pub async fn is_syncing(&self) -> bool {
        *self.is_syncing.read().await
    }

    /// Force a sync attempt (useful for testing or manual triggers)
    pub async fn force_sync(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_syncing().await {
            return Err("Sync already in progress".into());
        }

        // Create a temporary event channel for force_sync
        // Note: Events from force_sync won't be consumed by the main event loop
        // If you need events from force_sync, consider adding an event_tx parameter
        let (_tx, _rx) = mpsc::channel(100);
        self.perform_sync(&_tx).await.map_err(|e| e.into())
    }
}

impl Clone for Web3SyncManager {
    fn clone(&self) -> Self {
        // Note: This will panic if RoninClient creation fails
        // In a production environment, you might want to handle this differently
        let ronin_client = RoninClient::new(self.config.clone())
            .expect("Failed to create RoninClient in clone");

        Self {
            ronin_client,
            transaction_queue: Arc::clone(&self.transaction_queue),
            config: self.config.clone(),
            is_syncing: Arc::clone(&self.is_syncing),
            sync_stats: Arc::clone(&self.sync_stats),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SyncEvent {
    SyncStarted,
    SyncCompleted,
    SyncFailed(String),
    TransactionSynced(Uuid),
    TransactionFailed(Uuid, String),
}
