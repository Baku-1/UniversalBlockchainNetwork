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
            
            let sync_result = self.perform_sync().await;
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
    async fn perform_sync(&self) -> Result<(), String> {
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
            match self.sync_transaction(queued_tx).await {
                Ok(_) => {
                    synced_count += 1;
                    tracing::debug!("Successfully synced transaction");
                }
                Err(e) => {
                    failed_count += 1;
                    tracing::warn!("Failed to sync transaction: {}", e);
                }
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
    ) -> Result<(), Box<dyn std::error::Error>> {
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
        // TODO: Convert NFT operations to appropriate Ronin transactions
        // This would create ERC-721 or ERC-1155 transaction calls
        
        tracing::info!("Syncing NFT operation: {:?}", nft_op.operation_type);
        
        // For now, mark as completed (placeholder)
        self.transaction_queue.mark_completed(queue_id).await?;
        Ok(())
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

        self.perform_sync().await.map_err(|e| e.into())
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
