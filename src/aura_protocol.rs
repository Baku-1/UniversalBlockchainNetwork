// src/aura_protocol.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;
use anyhow::{Result, Context};
use ethers::{
    contract::abigen,
    providers::{Http, Provider, Middleware},
    types::{Address, U256, Bytes, Filter, Log},
    core::types::BlockNumber,
};
use crate::crypto::NodeKeypair;
use crate::config::RoninConfig;
use crate::web3::RoninClient;

// Generate contract bindings for AuraProtocol
abigen!(
    AuraProtocol,
    r#"[
        function tasks(uint256) external view returns (uint256 id, address requester, bytes taskData, uint256 bounty, uint256 createdAt, uint256 submissionDeadline, uint8 status, address[] workerCohort, bytes32 resultHash)
        function nextTaskId() external view returns (uint256)
        function verificationQuorum() external view returns (uint256)
        function createTask(bytes calldata taskData, uint256 bounty, address[] calldata workerCohort, uint32 deadlineDuration) external
        function submitResult(uint256 taskId, bytes calldata result, bytes[] calldata signatures) external
        function cancelStuckTask(uint256 taskId) external
        function requesterCancelTask(uint256 taskId) external
        event TaskCreated(uint256 indexed taskId, address indexed requester, uint256 bounty, bytes taskData, uint256 deadline)
        event TaskResultSubmitted(uint256 indexed taskId, bytes result, bytes32 resultHash, address submitter)
        event TaskCompleted(uint256 indexed taskId)
        event TaskFailed(uint256 indexed taskId)
        event TaskCancelled(uint256 indexed taskId, string reason)
    ]"#
);

/// Task status enum matching the contract
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Open = 0,
    Processing = 1,
    Verifying = 2,
    Completed = 3,
    Failed = 4,
    Cancelled = 5,
}

impl From<u8> for TaskStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => TaskStatus::Open,
            1 => TaskStatus::Processing,
            2 => TaskStatus::Verifying,
            3 => TaskStatus::Completed,
            4 => TaskStatus::Failed,
            5 => TaskStatus::Cancelled,
            _ => TaskStatus::Failed, // Default to failed for unknown values
        }
    }
}

/// Validation task from the AuraProtocol contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationTask {
    pub id: u64,
    pub requester: String,
    pub task_data: Vec<u8>,
    pub bounty: u64,
    pub created_at: u64,
    pub submission_deadline: u64,
    pub status: TaskStatus,
    pub worker_cohort: Vec<String>,
    pub result_hash: Option<String>,
    pub mesh_task_id: Option<Uuid>, // Link to mesh validation task
}

/// Task result with signatures from mesh nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: u64,
    pub result_data: Vec<u8>,
    pub signatures: Vec<Vec<u8>>,
    pub signers: Vec<String>,
    pub mesh_validation_id: Uuid,
}

/// Events from the AuraProtocol contract
#[derive(Debug, Clone)]
pub enum ContractEvent {
    TaskCreated {
        task_id: u64,
        requester: String,
        bounty: u64,
        task_data: Vec<u8>,
        deadline: u64,
    },
    TaskCompleted {
        task_id: u64,
    },
    TaskFailed {
        task_id: u64,
    },
    TaskCancelled {
        task_id: u64,
        reason: String,
    },
}

/// AuraProtocol contract client for mesh validation network
pub struct AuraProtocolClient {
    contract: AuraProtocol<Provider<Http>>,
    provider: Arc<Provider<Http>>,
    contract_address: Address,
    node_keys: NodeKeypair,
    config: RoninConfig,
    active_tasks: Arc<RwLock<HashMap<u64, ValidationTask>>>,
    pending_results: Arc<RwLock<HashMap<u64, TaskResult>>>,
    event_sender: mpsc::Sender<ContractEvent>,
    last_processed_block: Arc<RwLock<u64>>,
}

impl AuraProtocolClient {
    /// Create a new AuraProtocol client
    pub fn new(
        contract_address: Address,
        ronin_client: Arc<RoninClient>,
        node_keys: NodeKeypair,
        config: RoninConfig,
    ) -> Result<Self> {
        let provider = ronin_client.provider.clone();
        let contract = AuraProtocol::new(contract_address, provider.clone());
        
        let (event_sender, _) = mpsc::channel(100);
        
        Ok(Self {
            contract,
            provider,
            contract_address,
            node_keys,
            config,
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            pending_results: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            last_processed_block: Arc::new(RwLock::new(0)),
        })
    }

    /// Start the contract event monitoring service
    pub async fn start_event_monitor(&self) -> mpsc::Receiver<ContractEvent> {
        let (tx, rx) = mpsc::channel(100);
        
        let client = self.clone();
        tokio::spawn(async move {
            client.event_monitor_loop(tx).await;
        });
        
        rx
    }

    /// Get all open validation tasks from the contract
    pub async fn get_open_tasks(&self) -> Result<Vec<ValidationTask>> {
        let next_task_id = self.contract.next_task_id().call().await
            .context("Failed to get next task ID")?;
        
        let mut open_tasks = Vec::new();
        
        // Check recent tasks (last 100 tasks or so)
        let start_id = if next_task_id.as_u64() > 100 { 
            next_task_id.as_u64() - 100 
        } else { 
            0 
        };
        
        for task_id in start_id..next_task_id.as_u64() {
            if let Ok(task) = self.get_task(task_id).await {
                if task.status == TaskStatus::Open {
                    open_tasks.push(task);
                }
            }
        }
        
        tracing::info!("Found {} open validation tasks", open_tasks.len());
        Ok(open_tasks)
    }

    /// Get a specific task from the contract
    pub async fn get_task(&self, task_id: u64) -> Result<ValidationTask> {
        let task_result = self.contract.tasks(U256::from(task_id)).call().await
            .context("Failed to get task from contract")?;

        // The result is a tuple: (id, requester, taskData, bounty, createdAt, submissionDeadline, status, workerCohort, resultHash)
        let worker_cohort: Vec<String> = task_result.7
            .iter()
            .map(|addr| format!("{:?}", addr))
            .collect();

        let result_hash = if task_result.8 != [0u8; 32] {
            Some(format!("0x{}", hex::encode(task_result.8)))
        } else {
            None
        };

        Ok(ValidationTask {
            id: task_result.0.as_u64(),
            requester: format!("{:?}", task_result.1),
            task_data: task_result.2.to_vec(),
            bounty: task_result.3.as_u64(),
            created_at: task_result.4.as_u64(),
            submission_deadline: task_result.5.as_u64(),
            status: TaskStatus::from(task_result.6),
            worker_cohort,
            result_hash,
            mesh_task_id: None,
        })
    }

    /// Submit validation result to the contract
    pub async fn submit_result(&self, result: TaskResult) -> Result<String> {
        tracing::info!("Submitting result for task {}", result.task_id);

        // Convert signatures to the format expected by the contract
        let signatures: Vec<Bytes> = result.signatures
            .iter()
            .map(|sig| Bytes::from(sig.clone()))
            .collect();
        
        tracing::debug!("Converted {} signatures for contract submission", signatures.len());

        // For now, we'll return a simulated transaction hash
        // In a real implementation, this would require a wallet/signer
        let tx_hash = format!("0x{:064x}", result.mesh_validation_id.as_u128());

        tracing::info!("Result submission would create transaction: {}", tx_hash);

        // Store the pending result for tracking
        {
            let mut pending = self.pending_results.write().await;
            pending.insert(result.task_id, result);
        }

        Ok(tx_hash)
    }

    /// Check if this node is part of a task's worker cohort
    pub fn is_node_in_cohort(&self, task: &ValidationTask) -> bool {
        let node_address = self.node_keys.node_id();
        task.worker_cohort.contains(&node_address)
    }

    /// Get the verification quorum percentage from the contract
    pub async fn get_verification_quorum(&self) -> Result<u64> {
        let quorum = self.contract.verification_quorum().call().await
            .context("Failed to get verification quorum")?;
        Ok(quorum.as_u64())
    }
}

impl AuraProtocolClient {
    /// Event monitoring loop for contract events
    async fn event_monitor_loop(&self, event_tx: mpsc::Sender<ContractEvent>) {
        let mut interval = tokio::time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;

            if let Err(e) = self.process_contract_events(&event_tx).await {
                tracing::error!("Error processing contract events: {}", e);
            }
        }
    }

    /// Process new contract events since last check
    async fn process_contract_events(&self, event_tx: &mpsc::Sender<ContractEvent>) -> Result<()> {
        let current_block = self.provider.get_block_number().await
            .context("Failed to get current block number")?;

        let last_block = {
            let last = self.last_processed_block.read().await;
            *last
        };

        if current_block.as_u64() <= last_block {
            return Ok(());
        }

        // Create filter for AuraProtocol events
        let filter = Filter::new()
            .address(self.contract_address)
            .from_block(BlockNumber::Number((last_block + 1).into()))
            .to_block(BlockNumber::Number(current_block));

        let logs = self.provider.get_logs(&filter).await
            .context("Failed to get contract logs")?;

        for log in logs {
            if let Some(event) = self.parse_contract_event(log).await {
                if let Err(e) = event_tx.send(event).await {
                    tracing::error!("Failed to send contract event: {}", e);
                }
            }
        }

        // Update last processed block
        {
            let mut last = self.last_processed_block.write().await;
            *last = current_block.as_u64();
        }

        Ok(())
    }

    /// Parse contract log into ContractEvent
    async fn parse_contract_event(&self, log: Log) -> Option<ContractEvent> {
        // For now, we'll use a simplified event parsing approach
        // In a production implementation, you would use the proper event filters

        if let Some(topic) = log.topics.first() {
            // Convert topic to string for comparison
            let topic_str = format!("{:?}", topic);

            // TaskCreated event signature: TaskCreated(uint256,address,uint256,bytes,uint256)
            if topic_str.contains("TaskCreated") || log.topics.len() >= 3 {
                // Parse basic task created event
                if log.topics.len() >= 3 {
                    let task_id = log.topics[1].as_bytes()[24..32].iter()
                        .fold(0u64, |acc, &b| (acc << 8) | b as u64);

                    return Some(ContractEvent::TaskCreated {
                        task_id,
                        requester: format!("{:?}", log.topics[2]),
                        bounty: 0, // Would need to parse from data
                        task_data: log.data.to_vec(),
                        deadline: 0, // Would need to parse from data
                    });
                }
            }
            // TaskCompleted event
            else if topic_str.contains("TaskCompleted") {
                if log.topics.len() >= 2 {
                    let task_id = log.topics[1].as_bytes()[24..32].iter()
                        .fold(0u64, |acc, &b| (acc << 8) | b as u64);

                    return Some(ContractEvent::TaskCompleted { task_id });
                }
            }
            // TaskFailed event
            else if topic_str.contains("TaskFailed") {
                if log.topics.len() >= 2 {
                    let task_id = log.topics[1].as_bytes()[24..32].iter()
                        .fold(0u64, |acc, &b| (acc << 8) | b as u64);

                    return Some(ContractEvent::TaskFailed { task_id });
                }
            }
            // TaskCancelled event
            else if topic_str.contains("TaskCancelled") {
                if log.topics.len() >= 2 {
                    let task_id = log.topics[1].as_bytes()[24..32].iter()
                        .fold(0u64, |acc, &b| (acc << 8) | b as u64);

                    return Some(ContractEvent::TaskCancelled {
                        task_id,
                        reason: "Contract event".to_string(),
                    });
                }
            }
        }

        None
    }

    /// Add a task to active tracking
    pub async fn add_active_task(&self, task: ValidationTask) {
        let mut active = self.active_tasks.write().await;
        active.insert(task.id, task);
    }

    /// Remove a task from active tracking
    pub async fn remove_active_task(&self, task_id: u64) -> Option<ValidationTask> {
        let mut active = self.active_tasks.write().await;
        active.remove(&task_id)
    }

    /// Get all active tasks being processed by this node
    pub async fn get_active_tasks(&self) -> Vec<ValidationTask> {
        let active = self.active_tasks.read().await;
        active.values().cloned().collect()
    }

    /// Check if a task deadline has passed
    pub fn is_task_expired(&self, task: &ValidationTask) -> bool {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        now > task.submission_deadline
    }

    /// Calculate required signatures for quorum
    pub async fn calculate_required_signatures(&self, cohort_size: usize) -> Result<usize> {
        let quorum = self.get_verification_quorum().await?;
        let required = (cohort_size * quorum as usize + 99) / 100; // Ceiling division
        Ok(required.max(1)) // At least 1 signature required
    }
}

// Clone implementation for async tasks
impl Clone for AuraProtocolClient {
    fn clone(&self) -> Self {
        Self {
            contract: self.contract.clone(),
            provider: self.provider.clone(),
            contract_address: self.contract_address,
            node_keys: self.node_keys.clone(),
            config: self.config.clone(),
            active_tasks: self.active_tasks.clone(),
            pending_results: self.pending_results.clone(),
            event_sender: self.event_sender.clone(),
            last_processed_block: self.last_processed_block.clone(),
        }
    }
}
