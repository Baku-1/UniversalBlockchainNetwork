// src/mesh_validation.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;
use tokio::sync::{broadcast, RwLock};
use std::sync::Arc;
use uuid::Uuid;
use sha3::{Keccak256, Digest};
use crate::crypto::NodeKeypair;
use crate::config::RoninConfig;

use crate::aura_protocol::{ValidationTask, TaskResult as ContractTaskResult, TaskStatus};
use crate::contract_integration::{ContractIntegration, TaskResult as ContractTaskResultForSubmission};
use crate::economic_engine::{EconomicEngine, NetworkStats, CollateralRequirements, LendingEligibility};
use crate::contract_integration::ContractTask;

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
    validation_events: broadcast::Sender<ValidationEvent>,
    // Contract task tracking
    active_contract_tasks: Arc<RwLock<HashMap<u64, ValidationTask>>>,
    contract_task_signatures: Arc<RwLock<HashMap<u64, HashMap<String, Vec<u8>>>>>, // task_id -> (node_id -> signature)
    contract_task_results: Arc<RwLock<HashMap<u64, Vec<u8>>>>, // task_id -> result_data
    
    // Economic engine integration
    economic_engine: Arc<EconomicEngine>,
    // Collateral management system
    collateral_requirements: CollateralRequirements,
    // Signature threshold for contract tasks
    minimum_signature_threshold: u32,
    

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
    /// Validate collateral requirements for a transaction
    pub fn validate_collateral_requirements(
        &self,
        transaction: &MeshTransaction,
        user_balance: u64,
        collateral_ratio: f64,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        // Check minimum collateral ratio
        if collateral_ratio < self.collateral_requirements.minimum_ratio {
            return Ok(false);
        }
        
        // Check if user has sufficient balance
        let required_collateral = (transaction.amount as f64 * collateral_ratio) as u64;
        if user_balance < required_collateral {
            return Ok(false);
        }
        
        // Check liquidation threshold
        if collateral_ratio < self.collateral_requirements.liquidation_threshold {
            return Ok(false);
        }
        
        // Check maintenance margin
        if collateral_ratio < self.collateral_requirements.maintenance_margin {
            return Ok(false);
        }
        
        Ok(true)
    }
    /// Create a new mesh validator
    pub fn new(node_keys: NodeKeypair, config: RoninConfig) -> (Self, broadcast::Receiver<ValidationEvent>) {
        let (validation_events, validation_events_rx) = broadcast::channel(100);

        let validator = Self {
            node_keys,
            config,
            pending_transactions: Arc::new(RwLock::new(HashMap::new())),
            validation_results: Arc::new(RwLock::new(HashMap::new())),
            user_balances: Arc::new(RwLock::new(HashMap::new())),
            validation_events,
            active_contract_tasks: Arc::new(RwLock::new(HashMap::new())),
            contract_task_signatures: Arc::new(RwLock::new(HashMap::new())),
            contract_task_results: Arc::new(RwLock::new(HashMap::new())),
            economic_engine: Arc::new(EconomicEngine::new()),
            collateral_requirements: CollateralRequirements {
                minimum_ratio: 1.2,
                liquidation_threshold: 1.1,
                maintenance_margin: 1.15,
                risk_adjustment: 0.1,
            },
            minimum_signature_threshold: 3, // Require at least 3 valid signatures
        };
        
        (validator, validation_events_rx)
    }

    /// Process a mesh transaction
    pub async fn process_transaction(&mut self, transaction: MeshTransaction) -> Result<ValidationResult, Box<dyn std::error::Error>> {
        let transaction_id = transaction.id;
        
        // Add to pending transactions
        {
            let mut pending = self.pending_transactions.write().await;
            pending.insert(transaction_id, transaction.clone());
        }
        
        // Send transaction received event
        if let Err(e) = self.validation_events.send(ValidationEvent::TransactionReceived(transaction_id)) {
            tracing::warn!("Failed to send transaction received event: {}", e);
        }
        
        // Validate transaction
        let validation_result = self.validate_transaction(&transaction).await?;
        
        if validation_result.is_valid {
            // Execute transaction if valid
            self.execute_transaction(transaction.clone()).await?;
            
            // Update economic stats
            self.update_economic_stats(&transaction).await?;
            
            // Send validation completed event
            if let Err(e) = self.validation_events.send(ValidationEvent::ValidationCompleted(transaction_id, true)) {
                tracing::warn!("Failed to send validation completed event: {}", e);
            }
            
            // Send transaction executed event
            if let Err(e) = self.validation_events.send(ValidationEvent::TransactionExecuted(transaction_id)) {
                tracing::warn!("Failed to send transaction executed event: {}", e);
            }
            
            tracing::info!("Transaction {} processed successfully", transaction_id);
        } else {
            // Reject invalid transaction
            self.reject_transaction(transaction_id, validation_result.reason.as_ref().unwrap_or(&"Validation failed".to_string()).clone()).await?;
            
            // Send validation completed event
            if let Err(e) = self.validation_events.send(ValidationEvent::ValidationCompleted(transaction_id, false)) {
                tracing::warn!("Failed to send validation completed event: {}", e);
            }
            
            tracing::warn!("Transaction {} rejected: {}", transaction_id, validation_result.reason.as_deref().unwrap_or("Unknown reason"));
        }
        
        Ok(validation_result)
    }

    /// Validate a mesh transaction
    pub async fn validate_transaction(
        &self,
        transaction: &MeshTransaction,
    ) -> Result<ValidationResult, Box<dyn std::error::Error>> {
        // First validate transaction format
        let format_valid = self.validate_transaction_format(transaction).await?;
        if !format_valid {
            let validation_data = format!("INVALID_FORMAT_{}", transaction.id);
            let signature = self.node_keys.sign(validation_data.as_bytes());
            
            return Ok(ValidationResult {
                transaction_id: transaction.id,
                validator_id: self.node_keys.node_id(),
                is_valid: false,
                reason: Some("Invalid transaction format".to_string()),
                signature,
                timestamp: SystemTime::now(),
            });
        }
        
        // Check if sender has sufficient balance
        let has_balance = self.check_user_balance(&transaction.from_address, &transaction).await?;
        
        // Check collateral requirements for high-value transactions
        let collateral_valid = if transaction.amount > self.config.max_offline_transactions as u64 * 10 { // Use config-based threshold
            let user_balance = self.get_user_balance_amount(&transaction.from_address).await?;
            let collateral_ratio = user_balance as f64 / transaction.amount as f64;
            self.validate_collateral_requirements(&transaction, user_balance, collateral_ratio)?
        } else {
            true // No collateral required for small transactions
        };
        
        // Create validation result
        let validation_result = ValidationResult {
            transaction_id: transaction.id,
            validator_id: self.node_keys.node_id(),
            is_valid: has_balance && collateral_valid,
            reason: if !has_balance { 
                Some("Insufficient balance".to_string()) 
            } else if !collateral_valid {
                Some("Insufficient collateral".to_string())
            } else {
                None 
            },
            signature: self.sign_validation(&transaction).await?,
            timestamp: SystemTime::now(),
        };

        // Store validation result
        {
            let mut results = self.validation_results.write().await;
            results.entry(transaction.id).or_insert_with(Vec::new).push(validation_result.clone());
        }

        // Check if we have enough validations to proceed
        self.check_validation_consensus(transaction.id).await?;

        Ok(validation_result)
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
                    let _ = self.validation_events.send(ValidationEvent::ValidationCompleted(tx_id, true));
                } else {
                    self.reject_transaction(tx_id, "Validation consensus not reached".to_string()).await?;
                    let _ = self.validation_events.send(ValidationEvent::ValidationCompleted(tx_id, false));
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

        // Update economic engine with transaction settlement
        let _ = self.economic_engine.record_transaction_settled_with_details(
            transaction.amount,
            format!("{:?}", transaction.token_type),
            transaction.from_address.clone(),
            transaction.to_address.clone(),
        ).await;

        // Integrate with lending pools for high-value transactions
        if transaction.amount > self.config.max_offline_transactions as u64 * 10 { // Use config-based threshold for lending pool integration
            self.process_lending_pool_integration(&transaction).await?;
        }

        // Mark transaction as validated
        {
            let mut pending = self.pending_transactions.write().await;
            if let Some(tx) = pending.get_mut(&transaction.id) {
                tx.status = MeshTransactionStatus::Validated;
            }
        }

        let _ = self.validation_events.send(ValidationEvent::TransactionExecuted(transaction.id));
        Ok(())
    }

    /// Process lending pool integration for high-value transactions
    async fn process_lending_pool_integration(&self, transaction: &MeshTransaction) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Processing lending pool integration for transaction: {}", transaction.id);

        // Check if this transaction qualifies for lending pool operations
        if let TokenType::RON = transaction.token_type {
            // Get current user balance to assess lending eligibility
            let user_balance = self.get_user_balance_amount(&transaction.from_address).await?;
            
            // Calculate lending pool eligibility based on transaction amount and user balance
            let lending_eligibility = self.calculate_lending_eligibility(transaction.amount, user_balance).await?;
            
            if lending_eligibility.eligible {
                // Record lending pool activity in economic engine
                let _ = self.economic_engine.record_lending_pool_activity(
                    transaction.id.to_string(),
                    transaction.amount,
                    lending_eligibility.loan_amount,
                    lending_eligibility.interest_rate,
                ).await;
                
                tracing::info!("Transaction {} qualifies for lending pool integration: {} RON loan at {:.2}% interest", 
                    transaction.id, lending_eligibility.loan_amount, lending_eligibility.interest_rate * 100.0);
            }
        }

        Ok(())
    }

    /// Calculate lending eligibility for a transaction
    async fn calculate_lending_eligibility(&self, transaction_amount: u64, user_balance: u64) -> Result<LendingEligibility, Box<dyn std::error::Error>> {
        // Simple lending eligibility calculation
        let collateral_ratio = user_balance as f64 / transaction_amount as f64;
        let eligible = collateral_ratio >= 1.5; // 150% collateral requirement
        
        let loan_amount = if eligible {
            (transaction_amount as f64 * 0.8) as u64 // 80% of transaction amount
        } else {
            0
        };
        
        let interest_rate = if eligible {
            // Dynamic interest rate based on collateral ratio
            let base_rate = 0.05; // 5% base rate
            let collateral_bonus = if collateral_ratio >= 2.0 { 0.02 } else { 0.01 }; // 2% or 1% bonus
            base_rate - collateral_bonus
        } else {
            0.0
        };
        
        Ok(LendingEligibility {
            eligible,
            loan_amount,
            interest_rate,
            collateral_ratio,
        })
    }

    /// Reject a transaction
    async fn reject_transaction(&self, tx_id: Uuid, reason: String) -> Result<(), Box<dyn std::error::Error>> {
        {
            let mut pending = self.pending_transactions.write().await;
            if let Some(tx) = pending.get_mut(&tx_id) {
                tx.status = MeshTransactionStatus::Rejected;
            }
        }

        let _ = self.validation_events.send(ValidationEvent::TransactionRejected(tx_id, reason));
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

        let _ = self.validation_events.send(ValidationEvent::BalanceUpdated(transaction.from_address.clone()));
        let _ = self.validation_events.send(ValidationEvent::BalanceUpdated(transaction.to_address.clone()));

        // Update economic engine with network statistics
        self.update_economic_stats(transaction).await?;

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

        let _ = self.validation_events.send(ValidationEvent::ContractTaskReceived(task.id));
        let _ = self.validation_events.send(ValidationEvent::ContractTaskSignatureCollected(task.id, node_id));

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

        let _ = self.validation_events.send(ValidationEvent::ContractTaskSignatureCollected(task_id, node_id));

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

                let _ = self.validation_events.send(ValidationEvent::ContractTaskCompleted(task_id, true));
            }
        }

        Ok(())
    }

    /// Get contract task result ready for submission
    pub async fn get_contract_task_result(&self, task_id: u64) -> Option<ContractTaskResult> {
        let (task, task_signatures, result) = {
            let active = self.active_contract_tasks.read().await;
            let sigs = self.contract_task_signatures.read().await;
            let results = self.contract_task_results.read().await;

            let task = active.get(&task_id).cloned()?;
            let task_signatures = sigs.get(&task_id).cloned().unwrap_or_default();
            let result = results.get(&task_id).cloned()?;

            (task, task_signatures, result)
        };

        // Check if we have enough signatures
        let required_signatures = (task.worker_cohort.len() / 2) + 1;
        if task_signatures.len() >= required_signatures {
            let signature_vec: Vec<Vec<u8>> = task_signatures.values().cloned().collect();
            let signers: Vec<String> = task_signatures.keys().cloned().collect();

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

    /// Get user balance amount for collateral validation
    pub async fn get_user_balance_amount(&self, address: &str) -> Result<u64, Box<dyn std::error::Error>> {
        let balance = self.get_user_balance(address).await
            .ok_or_else(|| format!("User balance not found for address: {}", address))?;
        Ok(balance.ron_balance)
    }

    /// Initialize user balance from blockchain
    pub async fn initialize_user_balance(&self, address: String, balance: UserBalance) -> Result<(), Box<dyn std::error::Error>> {
        let mut balances = self.user_balances.write().await;
        balances.insert(address, balance);
        Ok(())
    }



    /// Submit contract task result through mesh validation
    pub async fn submit_contract_task_result(&self, contract_integration: &ContractIntegration, task_id: u64, result_data: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Submitting contract task result for task {}", task_id);

        // Get the task
        let task = contract_integration.get_task(task_id).await
            .ok_or_else(|| format!("Contract task {} not found", task_id))?;

        // Create signature for the result
        let signature = self.sign_contract_result(task_id, &result_data)?;

        // Collect signatures from other mesh validators
        let signatures = self.collect_mesh_signatures(task_id, &result_data).await?;

        // Add our own signature to the collection
        let mut all_signatures = signatures.clone();
        all_signatures.push(signature);

        // Validate the task result against the original task
        if !self.validate_task_result(&task, &result_data).await? {
            return Err("Task result validation failed".into());
        }

        // Verify the collected signatures (including our own)
        let valid_signatures = self.verify_collected_signatures(task_id, &result_data, &all_signatures).await?;
        if valid_signatures.len() < self.minimum_signature_threshold as usize {
            return Err(format!("Insufficient valid signatures: {} < {}", 
                valid_signatures.len(), self.minimum_signature_threshold).into());
        }

        // Create contract task result
        // Create the contract result for submission with actual signatures
        let contract_result = ContractTaskResultForSubmission {
            task_id,
            result_data: result_data.clone(),
            signatures: valid_signatures, // Use the verified signatures
            submitter: self.node_keys.node_id(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        // Submit to contract
        let tx_hash = contract_integration.submit_task_result(contract_result).await?;
        tracing::info!("Contract task result submitted with tx hash: {}", tx_hash);

        // Send validation event
        let _ = self.validation_events.send(ValidationEvent::ContractTaskCompleted(task_id, true));

        Ok(())
    }

    /// Sign contract task result
    fn sign_contract_result(&self, task_id: u64, result_data: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
        // Create message to sign: task_id + result_hash
        let result_hash = Keccak256::digest(result_data);
        let mut message = Vec::new();
        message.extend_from_slice(&task_id.to_be_bytes());
        message.extend_from_slice(&result_hash);

        // Sign the message
        let signature = self.node_keys.sign(&message);
        Ok(hex::encode(signature))
    }

    /// Collect signatures from other mesh validators
    async fn collect_mesh_signatures(&self, task_id: u64, result_data: &[u8]) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        // Implement mesh signature collection
        // This involves:
        // 1. Broadcasting the result to other mesh validators
        // 2. Collecting their signatures
        // 3. Verifying the signatures
        // 4. Returning the collected signatures

        let mut collected_signatures = Vec::new();
        
        // Start with our own signature
        let our_signature = self.sign_contract_result(task_id, result_data)?;
        collected_signatures.push(our_signature);
        
        // In production, this would:
        // 1. Broadcast the result to mesh peers via mesh.rs
        // 2. Wait for responses with signatures
        // 3. Verify each signature using crypto.rs
        // 4. Collect valid signatures
        
        // For now, simulate collecting signatures from mesh peers
        // This would integrate with mesh.rs to get peer list and send validation requests
        tracing::debug!("Mesh signature collection for task {}: collected {} signatures (including our own)", 
            task_id, collected_signatures.len());
        
        Ok(collected_signatures)
    }

    /// Verify contract task result from another validator
    pub async fn verify_contract_task_result(&self, task_id: u64, result_data: &[u8], signature: &str, validator_id: &str) -> Result<bool, Box<dyn std::error::Error>> {
        // Verify the signature against the validator's public key
        tracing::debug!("Verifying contract task result from validator {}", validator_id);

        // Decode the hex signature
        let signature_bytes = hex::decode(signature)
            .map_err(|_| "Invalid signature format")?;

        // Create the message that was signed: task_id + result_hash
        let result_hash = Keccak256::digest(result_data);
        let mut message = Vec::new();
        message.extend_from_slice(&task_id.to_be_bytes());
        message.extend_from_slice(&result_hash);

        // Get validator's public key and verify signature using signature_bytes
        // In production, this would verify the actual cryptographic signature
        match crate::crypto::public_key_from_node_id(validator_id) {
            Ok(public_key) => {
                // Verify the signature using crypto.rs
                match crate::crypto::verify_signature(&public_key, &message, &signature_bytes) {
                    Ok(_) => {
                        tracing::debug!("Signature verification for task {} from validator {}: SUCCESS", 
                            task_id, validator_id);
                        Ok(true)
                    }
                    Err(e) => {
                        tracing::warn!("Signature verification for task {} from validator {}: FAILED - {}", 
                            task_id, validator_id, e);
                        Ok(false)
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to get public key for validator {}: {}", validator_id, e);
                Ok(false)
            }
        }
    }

    /// Validate task result against original task
    async fn validate_task_result(&self, task: &ContractTask, result_data: &[u8]) -> Result<bool, Box<dyn std::error::Error>> {
        // Check if result data matches expected format
        if result_data.len() < task.minimum_result_size {
            return Ok(false);
        }

        // Validate result hash if provided
        if let Some(expected_hash) = &task.expected_result_hash {
            let actual_hash = Keccak256::digest(result_data);
            if actual_hash.as_slice() != expected_hash.as_slice() {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Verify collected signatures from mesh validators
    async fn verify_collected_signatures(&self, task_id: u64, result_data: &[u8], signatures: &[String]) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut valid_signatures = Vec::new();
        
        for signature in signatures {
            // Verify each signature
            if self.verify_signature(task_id, result_data, signature).await? {
                valid_signatures.push(signature.clone());
            }
        }
        
        Ok(valid_signatures)
    }

    /// Verify individual signature
    async fn verify_signature(&self, task_id: u64, result_data: &[u8], signature: &str) -> Result<bool, Box<dyn std::error::Error>> {
        // Verify the signature format and content
        let signature_bytes = hex::decode(signature)
            .map_err(|_| "Invalid signature format")?;

        // Create the message that was signed: task_id + result_hash
        let result_hash = Keccak256::digest(result_data);
        let mut message = Vec::new();
        message.extend_from_slice(&task_id.to_be_bytes());
        message.extend_from_slice(&result_hash);

        // Implement actual signature verification using the node's public key and signature_bytes
        // For now, accept all signatures as valid (placeholder implementation)
        // In production, this would:
        // 1. Get the validator's public key from the signature context
        // 2. Use crypto.rs verify_signature function
        // 3. Return true only if signature is cryptographically valid
        
        tracing::debug!("Signature verification for task {}: accepted (signature length: {})", task_id, signature_bytes.len());
        
        // Implement proper signature verification using crypto.rs
        // Extract validator ID from signature context (first 32 bytes)
        if signature_bytes.len() < 32 {
            tracing::warn!("Signature too short for task {}: expected at least 32 bytes, got {}", 
                task_id, signature_bytes.len());
            return Ok(false);
        }
        
        // Use the first 32 bytes as validator ID (in production, this would be more sophisticated)
        let validator_id_bytes = &signature_bytes[..32];
        let validator_id = hex::encode(validator_id_bytes);
        
        // Get validator's public key and verify signature
        match crate::crypto::public_key_from_node_id(&validator_id) {
            Ok(public_key) => {
                // Extract actual signature (remaining bytes after validator ID)
                let actual_signature = &signature_bytes[32..];
                
                // Verify the signature using crypto.rs
                match crate::crypto::verify_signature(&public_key, &message, actual_signature) {
                    Ok(_) => {
                        tracing::debug!("Signature verification for task {} from validator {}: SUCCESS", 
                            task_id, validator_id);
                        Ok(true)
                    }
                    Err(e) => {
                        tracing::warn!("Signature verification for task {} from validator {}: FAILED - {}", 
                            task_id, validator_id, e);
                        Ok(false)
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to get public key for validator {}: {}", validator_id, e);
                Ok(false)
            }
        }
    }

    /// Get contract task statistics
    pub async fn get_contract_task_stats(&self) -> HashMap<String, u64> {
        let mut stats = HashMap::new();

        let active_tasks = self.active_contract_tasks.read().await;
        stats.insert("active_contract_tasks".to_string(), active_tasks.len() as u64);

        let signatures = self.contract_task_signatures.read().await;
        stats.insert("pending_signatures".to_string(), signatures.len() as u64);

        let results = self.contract_task_results.read().await;
        stats.insert("pending_results".to_string(), results.len() as u64);

        stats
    }

    /// Update economic engine with network statistics
    async fn update_economic_stats(&self, transaction: &MeshTransaction) -> Result<(), Box<dyn std::error::Error>> {
        // Get current network statistics
        let balances = self.user_balances.read().await;
        let pending = self.pending_transactions.read().await;
        
        let total_transactions = pending.len() as u64;
        let active_users = balances.len() as u64;
        
        // Calculate network utilization based on active transactions
        let network_utilization = (total_transactions as f64 / self.config.max_offline_transactions as f64).min(1.0); // Use config-based network capacity
        
        // Calculate average transaction value
        let total_value: u64 = pending.values().map(|tx| tx.amount).sum();
        let average_transaction_value = if total_transactions > 0 {
            total_value / total_transactions
        } else {
            0
        };
        
        // Calculate mesh congestion level based on pending transactions
        let mesh_congestion_level = (total_transactions as f64 / 500.0).min(1.0); // Cap at 100%
        
        // Create network stats for economic engine
        let network_stats = NetworkStats {
            total_transactions,
            active_users,
            network_utilization,
            average_transaction_value,
            mesh_congestion_level,
            total_lending_volume: 0, // Will be updated by economic engine
            total_borrowing_volume: 0, // Will be updated by economic engine
            average_collateral_ratio: 1.5, // Default collateral ratio
        };
        
        // Update economic engine with new stats
        let _ = self.economic_engine.update_network_stats(network_stats).await;
        
        // Record transaction for economic analysis
        let _ = self.economic_engine.record_transaction_settled_with_details(
            transaction.amount,
            format!("{:?}", transaction.token_type),
            transaction.from_address.clone(),
            transaction.to_address.clone(),
        ).await;
        
        tracing::info!("Updated economic stats: {} transactions, {} users, utilization: {:.2}%", 
            total_transactions, active_users, network_utilization * 100.0);
        
        Ok(())
    }
}
