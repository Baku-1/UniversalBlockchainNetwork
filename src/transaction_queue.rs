// src/transaction_queue.rs

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use uuid::Uuid;
use tokio::sync::{mpsc, RwLock};
use std::sync::Arc;
use anyhow::Result;
use crate::web3::{RoninTransaction, TransactionStatus, UtilityTransaction, NftOperation};
use crate::config::RoninConfig;

/// Offline transaction queue for storing transactions when network is unavailable
#[derive(Debug)]
pub struct OfflineTransactionQueue {
    /// Persistent storage for transactions
    db: sled::Db,
    /// In-memory queue for fast access
    pending_queue: Arc<RwLock<VecDeque<QueuedTransaction>>>,
    /// Transaction dependencies (tx_id -> depends_on_tx_ids)
    dependencies: Arc<RwLock<HashMap<Uuid, Vec<Uuid>>>>,
    /// Configuration
    config: RoninConfig,
    /// Channel for notifying about queue changes
    queue_notify: mpsc::Sender<QueueEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedTransaction {
    pub id: Uuid,
    pub transaction: TransactionType,
    pub priority: TransactionPriority,
    pub dependencies: Vec<Uuid>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub created_at: std::time::SystemTime,
    pub last_attempt: Option<std::time::SystemTime>,
    pub status: TransactionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionType {
    Ronin(RoninTransaction),
    Utility(UtilityTransaction),
    Nft(NftOperation),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum TransactionPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone)]
pub enum QueueEvent {
    TransactionAdded(Uuid),
    TransactionCompleted(Uuid),
    TransactionFailed(Uuid, String),
    QueueSizeChanged(usize),
}

impl OfflineTransactionQueue {
    /// Create a new offline transaction queue
    pub async fn new(
        db_path: &Path,
        config: RoninConfig,
        queue_notify: mpsc::Sender<QueueEvent>,
    ) -> Result<Self> {
        let db = sled::open(db_path)?;
        let pending_queue = Arc::new(RwLock::new(VecDeque::new()));
        let dependencies = Arc::new(RwLock::new(HashMap::new()));

        let mut queue = Self {
            db,
            pending_queue,
            dependencies,
            config,
            queue_notify,
        };

        // Load existing transactions from persistent storage
        queue.load_from_storage().await?;

        Ok(queue)
    }

    /// Add a transaction to the offline queue
    pub async fn add_transaction(
        &self,
        transaction: TransactionType,
        priority: TransactionPriority,
        dependencies: Vec<Uuid>,
    ) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        let queued_tx = QueuedTransaction {
            id,
            transaction,
            priority,
            dependencies: dependencies.clone(),
            retry_count: 0,
            max_retries: 3,
            created_at: std::time::SystemTime::now(),
            last_attempt: None,
            status: TransactionStatus::Queued,
        };

        // Store in persistent storage
        let serialized = bincode::serialize(&queued_tx)?;
        self.db.insert(id.as_bytes(), serialized)?;

        // Add to in-memory queue
        {
            let mut queue = self.pending_queue.write().await;
            queue.push_back(queued_tx);
            queue.make_contiguous().sort_by(|a, b| b.priority.cmp(&a.priority));
        }

        // Update dependencies
        if !dependencies.is_empty() {
            let mut deps = self.dependencies.write().await;
            deps.insert(id, dependencies);
        }

        // Notify about queue change
        let queue_size = self.pending_queue.read().await.len();
        let _ = self.queue_notify.send(QueueEvent::TransactionAdded(id)).await;
        let _ = self.queue_notify.send(QueueEvent::QueueSizeChanged(queue_size)).await;

        tracing::info!("Added transaction {} to offline queue", id);
        Ok(id)
    }

    /// Get the next transaction ready for processing
    pub async fn get_next_transaction(&self) -> Option<QueuedTransaction> {
        let mut queue = self.pending_queue.write().await;
        let deps = self.dependencies.read().await;

        // Find the first transaction with no unresolved dependencies
        let mut index = None;
        for (i, tx) in queue.iter().enumerate() {
            if tx.status != TransactionStatus::Queued {
                continue;
            }

            let has_unresolved_deps = if let Some(tx_deps) = deps.get(&tx.id) {
                tx_deps.iter().any(|dep_id| {
                    queue.iter().any(|other_tx| {
                        other_tx.id == *dep_id && 
                        other_tx.status != TransactionStatus::Confirmed
                    })
                })
            } else {
                false
            };

            if !has_unresolved_deps {
                index = Some(i);
                break;
            }
        }

        if let Some(i) = index {
            Some(queue.remove(i).unwrap())
        } else {
            None
        }
    }

    /// Mark a transaction as completed
    pub async fn mark_completed(&self, tx_id: Uuid) -> Result<()> {
        // Remove from persistent storage
        self.db.remove(tx_id.as_bytes())?;

        // Remove from dependencies
        {
            let mut deps = self.dependencies.write().await;
            deps.remove(&tx_id);
            
            // Remove this transaction from other transactions' dependencies
            for (_, dep_list) in deps.iter_mut() {
                dep_list.retain(|&id| id != tx_id);
            }
        }

        // Notify about completion
        let _ = self.queue_notify.send(QueueEvent::TransactionCompleted(tx_id)).await;
        let queue_size = self.pending_queue.read().await.len();
        let _ = self.queue_notify.send(QueueEvent::QueueSizeChanged(queue_size)).await;

        tracing::info!("Transaction {} marked as completed", tx_id);
        Ok(())
    }

    /// Mark a transaction as failed
    pub async fn mark_failed(
        &self,
        tx_id: Uuid,
        error: String,
    ) -> Result<()> {
        let mut queue = self.pending_queue.write().await;
        
        if let Some(tx) = queue.iter_mut().find(|tx| tx.id == tx_id) {
            tx.retry_count += 1;
            tx.last_attempt = Some(std::time::SystemTime::now());
            
            if tx.retry_count >= tx.max_retries {
                tx.status = TransactionStatus::Failed(error.clone());
                
                // Remove from persistent storage if permanently failed
                self.db.remove(tx_id.as_bytes())?;
                
                // Notify about failure
                let _ = self.queue_notify.send(QueueEvent::TransactionFailed(tx_id, error)).await;
            } else {
                tx.status = TransactionStatus::Queued; // Retry later
                
                // Update in persistent storage
                let serialized = bincode::serialize(&*tx)?;
                self.db.insert(tx_id.as_bytes(), serialized)?;
            }
        }

        Ok(())
    }

    /// Get queue statistics
    pub async fn get_stats(&self) -> QueueStats {
        let queue = self.pending_queue.read().await;
        let mut stats = QueueStats::default();
        
        for tx in queue.iter() {
            match tx.status {
                TransactionStatus::Queued => stats.queued += 1,
                TransactionStatus::Pending => stats.pending += 1,
                TransactionStatus::Failed(_) => stats.failed += 1,
                _ => {}
            }
            
            match tx.priority {
                TransactionPriority::Critical => stats.critical += 1,
                TransactionPriority::High => stats.high += 1,
                TransactionPriority::Normal => stats.normal += 1,
                TransactionPriority::Low => stats.low += 1,
            }
        }
        
        stats.total = queue.len();
        stats
    }

    /// Load transactions from persistent storage
    async fn load_from_storage(&mut self) -> Result<()> {
        let mut queue = self.pending_queue.write().await;
        let mut deps = self.dependencies.write().await;
        
        for result in self.db.iter() {
            let (_key, value) = result?;
            let tx: QueuedTransaction = bincode::deserialize(&value)?;
            
            // Only load non-completed transactions
            if !matches!(tx.status, TransactionStatus::Confirmed) {
                if !tx.dependencies.is_empty() {
                    deps.insert(tx.id, tx.dependencies.clone());
                }
                queue.push_back(tx);
            }
        }
        
        // Sort by priority
        queue.make_contiguous().sort_by(|a, b| b.priority.cmp(&a.priority));
        
        tracing::info!("Loaded {} transactions from storage", queue.len());
        Ok(())
    }

    /// Clear all completed and failed transactions from storage
    pub async fn cleanup(&self) -> Result<usize> {
        let mut removed = 0;
        let queue = self.pending_queue.read().await;
        
        for tx in queue.iter() {
            if matches!(tx.status, TransactionStatus::Confirmed | TransactionStatus::Failed(_)) {
                self.db.remove(tx.id.as_bytes())?;
                removed += 1;
            }
        }
        
        tracing::info!("Cleaned up {} completed/failed transactions", removed);
        Ok(removed)
    }
}

#[derive(Debug, Default)]
pub struct QueueStats {
    pub total: usize,
    pub queued: usize,
    pub pending: usize,
    pub failed: usize,
    pub critical: usize,
    pub high: usize,
    pub normal: usize,
    pub low: usize,
}
