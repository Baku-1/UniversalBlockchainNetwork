// src/mesh_validation.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;
use tokio::sync::{mpsc, RwLock};
use std::sync::Arc;
use uuid::Uuid;
use crate::crypto::NodeKeypair;
use crate::config::RoninConfig;
use crate::aura_protocol::{ValidationTask, TaskResult as ContractTaskResult, TaskStatus};

/// Mesh transaction that can be processed between mesh participants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshTransaction {
    pub id: Uuid,
    pub from_address: String,
    pub to_address: String,
    pub amount: u64, // Amount in wei
    pub token_type: TokenType,
    pub nonce: u64,
    pub mesh_participants: Vec<String>, // Node IDs that need to validate
    pub signatures: HashMap<String, Vec<u8>>, // Node ID -> signature
    pub created_at: SystemTime,
    pub expires_at: SystemTime,
    pub status: MeshTransactionStatus,
    pub validation_threshold: u32, // Minimum validators needed
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TokenType {
    RON, // Ronin native token
    SLP, // Smooth Love Potion
    AXS, // Axie Infinity Shards
    NFT { contract_address: String, token_id: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MeshTransactionStatus {
    Pending,
    Validating,
    Validated,
    Rejected,
    Expired,
}

/// Validation result from a mesh participant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub transaction_id: Uuid,
    pub validator_id: String,
    pub is_valid: bool,
    pub reason: Option<String>,
    pub signature: Vec<u8>,
    pub timestamp: SystemTime,
}

/// Mesh validator for processing peer-to-peer transactions and contract tasks
pub struct MeshValidator {
    node_keys: NodeKeypair,
    config: RoninConfig,
    pending_transactions: Arc<RwLock<HashMap<Uuid, MeshTransaction>>>,
    validation_results: Arc<RwLock<HashMap<Uuid, Vec<ValidationResult>>>>,
    user_balances: Arc<RwLock<HashMap<String, UserBalance>>>, // Track mesh balances
    validation_events: mpsc::Sender<ValidationEvent>,
    // Contract task validation
    active_contract_tasks: Arc<RwLock<HashMap<u64, ValidationTask>>>,
    contract_task_signatures: Arc<RwLock<HashMap<u64, HashMap<String, Vec<u8>>>>>, // task_id -> (node_id -> signature)
    contract_task_results: Arc<RwLock<HashMap<u64, Vec<u8>>>>, // task_id -> result_data
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserBalance {
    pub address: String,
    pub ron_balance: u64,
    pub slp_balance: u64,
    pub axs_balance: u64,
    pub nfts: Vec<NftBalance>,
    pub last_updated: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftBalance {
    pub contract_address: String,
    pub token_id: u64,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ValidationEvent {
    TransactionReceived(Uuid),
    ValidationCompleted(Uuid, bool),
    TransactionExecuted(Uuid),
    TransactionRejected(Uuid, String),
    BalanceUpdated(String),
    ContractTaskReceived(u64),
    ContractTaskCompleted(u64, bool),
    ContractTaskSignatureCollected(u64, String),
}

impl MeshValidator {
    /// Create a new mesh validator
    pub fn new(node_keys: NodeKeypair, config: RoninConfig) -> Self {
        let (validation_events, _) = mpsc::channel(100);

        Self {
            node_keys,
            config,
            pending_transactions: Arc::new(RwLock::new(HashMap::new())),
            validation_results: Arc::new(RwLock::new(HashMap::new())),
            user_balances: Arc::new(RwLock::new(HashMap::new())),
            validation_events,
            active_contract_tasks: Arc::new(RwLock::new(HashMap::new())),
            contract_task_signatures: Arc::new(RwLock::new(HashMap::new())),
            contract_task_results: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Process a new mesh transaction
    pub async fn process_mesh_transaction(
        &self,
        mut transaction: MeshTransaction,
    ) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Processing mesh transaction: {}", transaction.id);

        // Validate transaction format
        if !self.validate_transaction_format(&transaction).await? {
            return Err("Invalid transaction format".into());
        }

        // Check if we have sufficient participants
        if transaction.mesh_participants.len() < transaction.validation_threshold as usize {
            return Err("Insufficient mesh participants for validation".into());
        }

        // Add to pending transactions
        transaction.status = MeshTransactionStatus::Validating;
        {
            let mut pending = self.pending_transactions.write().await;
            pending.insert(transaction.id, transaction.clone());
        }

        // Start validation process
        let tx_id = transaction.id;
        self.validate_transaction(transaction).await?;

        let _ = self.validation_events.send(ValidationEvent::TransactionReceived(tx_id)).await;
        Ok(())
    }

    /// Validate a mesh transaction
    async fn validate_transaction(
        &self,
        transaction: MeshTransaction,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if sender has sufficient balance
        let has_balance = self.check_user_balance(&transaction.from_address, &transaction).await?;
        
        // Create validation result
        let validation_result = ValidationResult {
            transaction_id: transaction.id,
            validator_id: self.node_keys.node_id(),
            is_valid: has_balance,
            reason: if has_balance { None } else { Some("Insufficient balance".to_string()) },
            signature: self.sign_validation(&transaction).await?,
            timestamp: SystemTime::now(),
        };

        // Store validation result
        {
            let mut results = self.validation_results.write().await;
            results.entry(transaction.id).or_insert_with(Vec::new).push(validation_result);
        }

        // Check if we have enough validations to proceed
        self.check_validation_consensus(transaction.id).await?;

        Ok(())
    }

    /// Check if transaction has reached validation consensus
    async fn check_validation_consensus(&self, tx_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
        let (transaction, validations) = {
            let pending = self.pending_transactions.read().await;
            let results = self.validation_results.read().await;
            
            let tx = pending.get(&tx_id).cloned();
            let vals = results.get(&tx_id).cloned().unwrap_or_default();
            (tx, vals)
        };

        if let Some(transaction) = transaction {
            let valid_count = validations.iter().filter(|v| v.is_valid).count();
            let total_count = validations.len();

            // Check if we have enough validations
            if total_count >= transaction.validation_threshold as usize {
                let consensus_reached = valid_count as f32 / total_count as f32 >= 0.67; // 67% consensus

                if consensus_reached {
                    self.execute_transaction(transaction).await?;
                    let _ = self.validation_events.send(ValidationEvent::ValidationCompleted(tx_id, true)).await;
                } else {
                    self.reject_transaction(tx_id, "Validation consensus not reached".to_string()).await?;
                    let _ = self.validation_events.send(ValidationEvent::ValidationCompleted(tx_id, false)).await;
                }
            }
        }

        Ok(())
    }

    /// Execute a validated mesh transaction
    async fn execute_transaction(&self, transaction: MeshTransaction) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Executing mesh transaction: {}", transaction.id);

        // Update balances
        self.update_user_balances(&transaction).await?;

        // Mark transaction as validated
        {
            let mut pending = self.pending_transactions.write().await;
            if let Some(tx) = pending.get_mut(&transaction.id) {
                tx.status = MeshTransactionStatus::Validated;
            }
        }

        let _ = self.validation_events.send(ValidationEvent::TransactionExecuted(transaction.id)).await;
        Ok(())
    }

    /// Reject a transaction
    async fn reject_transaction(&self, tx_id: Uuid, reason: String) -> Result<(), Box<dyn std::error::Error>> {
        {
            let mut pending = self.pending_transactions.write().await;
            if let Some(tx) = pending.get_mut(&tx_id) {
                tx.status = MeshTransactionStatus::Rejected;
            }
        }

        let _ = self.validation_events.send(ValidationEvent::TransactionRejected(tx_id, reason)).await;
        Ok(())
    }

    /// Check if user has sufficient balance for transaction
    async fn check_user_balance(
        &self,
        address: &str,
        transaction: &MeshTransaction,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let balances = self.user_balances.read().await;
        
        if let Some(balance) = balances.get(address) {
            match &transaction.token_type {
                TokenType::RON => Ok(balance.ron_balance >= transaction.amount),
                TokenType::SLP => Ok(balance.slp_balance >= transaction.amount),
                TokenType::AXS => Ok(balance.axs_balance >= transaction.amount),
                TokenType::NFT { contract_address, token_id } => {
                    Ok(balance.nfts.iter().any(|nft| 
                        nft.contract_address == *contract_address && nft.token_id == *token_id
                    ))
                }
            }
        } else {
            // If no balance record, assume insufficient funds
            Ok(false)
        }
    }

    /// Update user balances after transaction execution
    async fn update_user_balances(&self, transaction: &MeshTransaction) -> Result<(), Box<dyn std::error::Error>> {
        let mut balances = self.user_balances.write().await;

        // Update sender balance
        if let Some(sender_balance) = balances.get_mut(&transaction.from_address) {
            match &transaction.token_type {
                TokenType::RON => sender_balance.ron_balance -= transaction.amount,
                TokenType::SLP => sender_balance.slp_balance -= transaction.amount,
                TokenType::AXS => sender_balance.axs_balance -= transaction.amount,
                TokenType::NFT { contract_address, token_id } => {
                    sender_balance.nfts.retain(|nft| 
                        !(nft.contract_address == *contract_address && nft.token_id == *token_id)
                    );
                }
            }
            sender_balance.last_updated = SystemTime::now();
        }

        // Update receiver balance
        let receiver_balance = balances.entry(transaction.to_address.clone()).or_insert_with(|| UserBalance {
            address: transaction.to_address.clone(),
            ron_balance: 0,
            slp_balance: 0,
            axs_balance: 0,
            nfts: Vec::new(),
            last_updated: SystemTime::now(),
        });

        match &transaction.token_type {
            TokenType::RON => receiver_balance.ron_balance += transaction.amount,
            TokenType::SLP => receiver_balance.slp_balance += transaction.amount,
            TokenType::AXS => receiver_balance.axs_balance += transaction.amount,
            TokenType::NFT { contract_address, token_id } => {
                receiver_balance.nfts.push(NftBalance {
                    contract_address: contract_address.clone(),
                    token_id: *token_id,
                    metadata: None,
                });
            }
        }
        receiver_balance.last_updated = SystemTime::now();

        let _ = self.validation_events.send(ValidationEvent::BalanceUpdated(transaction.from_address.clone())).await;
        let _ = self.validation_events.send(ValidationEvent::BalanceUpdated(transaction.to_address.clone())).await;

        Ok(())
    }

    /// Validate transaction format
    async fn validate_transaction_format(&self, transaction: &MeshTransaction) -> Result<bool, Box<dyn std::error::Error>> {
        // Check basic format requirements
        if transaction.from_address.is_empty() || transaction.to_address.is_empty() {
            return Ok(false);
        }

        if transaction.amount == 0 && !matches!(transaction.token_type, TokenType::NFT { .. }) {
            return Ok(false);
        }

        if transaction.expires_at <= SystemTime::now() {
            return Ok(false);
        }

        Ok(true)
    }

    /// Sign a validation result
    async fn sign_validation(&self, transaction: &MeshTransaction) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Create validation message to sign
        let validation_data = format!("{}:{}:{}:{}", 
            transaction.id, 
            self.node_keys.node_id(),
            transaction.from_address,
            transaction.amount
        );

        // Sign with node's private key
        let signature = self.node_keys.sign(validation_data.as_bytes());
        Ok(signature)
    }

    /// Get validated transactions ready for blockchain settlement
    pub async fn get_validated_transactions(&self) -> Vec<MeshTransaction> {
        let pending = self.pending_transactions.read().await;
        pending.values()
            .filter(|tx| tx.status == MeshTransactionStatus::Validated)
            .cloned()
            .collect()
    }

    // === Contract Task Validation Methods ===

    /// Process a new contract validation task
    pub async fn process_contract_task(
        &self,
        task: ValidationTask,
    ) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Processing contract validation task: {}", task.id);

        // Check if this node is part of the worker cohort
        let node_id = self.node_keys.node_id();
        if !task.worker_cohort.contains(&node_id) {
            return Err("Node not in worker cohort for this task".into());
        }

        // Check if task is still open
        if task.status != TaskStatus::Open {
            return Err("Task is not open for validation".into());
        }

        // Check if task has expired
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now > task.submission_deadline {
            return Err("Task has expired".into());
        }

        // Add to active tasks
        {
            let mut active = self.active_contract_tasks.write().await;
            active.insert(task.id, task.clone());
        }

        // Start validation process
        let validation_result = self.validate_contract_task(&task).await?;

        // Store the result
        {
            let mut results = self.contract_task_results.write().await;
            results.insert(task.id, validation_result.clone());
        }

        // Create and store signature
        let signature = self.sign_contract_task_result(task.id, &validation_result).await?;
        {
            let mut signatures = self.contract_task_signatures.write().await;
            signatures.entry(task.id).or_insert_with(HashMap::new)
                .insert(node_id.clone(), signature);
        }

        let _ = self.validation_events.send(ValidationEvent::ContractTaskReceived(task.id)).await;
        let _ = self.validation_events.send(ValidationEvent::ContractTaskSignatureCollected(task.id, node_id)).await;

        Ok(())
    }

    /// Validate a contract task and produce result data
    async fn validate_contract_task(
        &self,
        task: &ValidationTask,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        tracing::debug!("Validating contract task {} with data length: {}", task.id, task.task_data.len());

        // For now, we'll implement a simple validation that processes the task data
        // In a real implementation, this would depend on the specific task type

        // Example validation: hash the task data and return it as the result
        let result_data = crate::crypto::hash_data(&task.task_data);

        // Simulate some processing time
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        tracing::debug!("Contract task {} validation completed", task.id);
        Ok(result_data)
    }

    /// Sign a contract task result
    async fn sign_contract_task_result(
        &self,
        task_id: u64,
        result_data: &[u8],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Create the data to sign (task_id + result_hash)
        let mut sign_data = Vec::new();
        sign_data.extend_from_slice(&task_id.to_be_bytes());
        sign_data.extend_from_slice(result_data);

        // Sign with node's private key
        let signature = self.node_keys.sign(&sign_data);
        Ok(signature)
    }

    /// Collect signatures from other mesh nodes for a contract task
    pub async fn add_contract_task_signature(
        &self,
        task_id: u64,
        node_id: String,
        signature: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        {
            let mut signatures = self.contract_task_signatures.write().await;
            signatures.entry(task_id).or_insert_with(HashMap::new)
                .insert(node_id.clone(), signature);
        }

        let _ = self.validation_events.send(ValidationEvent::ContractTaskSignatureCollected(task_id, node_id)).await;

        // Check if we have enough signatures to submit
        self.check_contract_task_completion(task_id).await?;

        Ok(())
    }

    /// Check if a contract task has enough signatures to be submitted
    async fn check_contract_task_completion(&self, task_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let (task, signatures, result) = {
            let active = self.active_contract_tasks.read().await;
            let sigs = self.contract_task_signatures.read().await;
            let results = self.contract_task_results.read().await;

            let task = active.get(&task_id).cloned();
            let signatures = sigs.get(&task_id).cloned().unwrap_or_default();
            let result = results.get(&task_id).cloned();

            (task, signatures, result)
        };

        if let (Some(task), Some(_result_data)) = (task, result) {
            let cohort_size = task.worker_cohort.len();
            let signature_count = signatures.len();

            // Calculate required signatures (simple majority for now)
            let required_signatures = (cohort_size / 2) + 1;

            if signature_count >= required_signatures {
                tracing::info!("Contract task {} has sufficient signatures ({}/{})",
                    task_id, signature_count, cohort_size);

                let _ = self.validation_events.send(ValidationEvent::ContractTaskCompleted(task_id, true)).await;
            }
        }

        Ok(())
    }

    /// Get contract task result ready for submission
    pub async fn get_contract_task_result(&self, task_id: u64) -> Option<ContractTaskResult> {
        let (task, signatures, result) = {
            let active = self.active_contract_tasks.read().await;
            let sigs = self.contract_task_signatures.read().await;
            let results = self.contract_task_results.read().await;

            let task = active.get(&task_id).cloned()?;
            let signatures = sigs.get(&task_id).cloned().unwrap_or_default();
            let result = results.get(&task_id).cloned()?;

            (task, signatures, result)
        };

        // Check if we have enough signatures
        let required_signatures = (task.worker_cohort.len() / 2) + 1;
        if signatures.len() >= required_signatures {
            let signature_vec: Vec<Vec<u8>> = signatures.values().cloned().collect();
            let signers: Vec<String> = signatures.keys().cloned().collect();

            Some(ContractTaskResult {
                task_id,
                result_data: result,
                signatures: signature_vec,
                signers,
                mesh_validation_id: task.mesh_task_id.unwrap_or_else(|| Uuid::new_v4()),
            })
        } else {
            None
        }
    }

    /// Get all active contract tasks
    pub async fn get_active_contract_tasks(&self) -> Vec<ValidationTask> {
        let active = self.active_contract_tasks.read().await;
        active.values().cloned().collect()
    }

    /// Remove a completed contract task
    pub async fn remove_contract_task(&self, task_id: u64) {
        {
            let mut active = self.active_contract_tasks.write().await;
            active.remove(&task_id);
        }
        {
            let mut signatures = self.contract_task_signatures.write().await;
            signatures.remove(&task_id);
        }
        {
            let mut results = self.contract_task_results.write().await;
            results.remove(&task_id);
        }
    }

    /// Get user balance
    pub async fn get_user_balance(&self, address: &str) -> Option<UserBalance> {
        let balances = self.user_balances.read().await;
        balances.get(address).cloned()
    }

    /// Initialize user balance from blockchain
    pub async fn initialize_user_balance(&self, address: String, balance: UserBalance) -> Result<(), Box<dyn std::error::Error>> {
        let mut balances = self.user_balances.write().await;
        balances.insert(address, balance);
        Ok(())
    }
}
