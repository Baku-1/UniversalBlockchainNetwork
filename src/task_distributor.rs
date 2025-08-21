// src/task_distributor.rs

use std::collections::HashMap;
use tokio::sync::{mpsc, RwLock};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use anyhow::Result;
use tracing;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use crate::validator::{ComputationTask, TaskType, BlockToValidate, GameStateData, TransactionData, ConflictData, TaskPriority};
use crate::gpu_processor::{GPUTaskScheduler, GPUProcessingTask, TaskStatus, TaskComplexity as GPUTaskComplexity};

/// Device capability information for task distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCapability {
    pub cpu_cores: u32,
    pub gpu_compute_units: Option<u32>,
    pub memory_gb: f32,
    pub benchmark_score: f64,
    pub current_load: f32,
    pub network_latency: Duration,
    pub battery_level: Option<f32>,
    pub thermal_status: ThermalStatus,
}

/// Thermal status for device management
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThermalStatus {
    Cool,
    Warm,
    Hot,
    Critical,
}

/// Task complexity analyzer for intelligent distribution
#[derive(Debug, Clone)]
pub struct ComplexityAnalyzer {
    complexity_cache: Arc<RwLock<HashMap<TaskType, f64>>>,
    historical_data: Arc<RwLock<Vec<ComplexityRecord>>>,
}

/// Complexity record for analysis
#[derive(Debug, Clone)]
pub struct ComplexityRecord {
    pub task_type: TaskType,
    pub processing_time: Duration,
    pub device_capability: DeviceCapability,
    pub success: bool,
    pub timestamp: SystemTime,
}

/// Load balancer for distributed task processing
#[derive(Debug, Clone)]
pub struct LoadBalancer {
    balancing_strategy: BalancingStrategy,
    health_check_interval: Duration,
    failover_threshold: u32,
}

/// Balancing strategy for task distribution
#[derive(Debug, Clone, PartialEq)]
pub enum BalancingStrategy {
    RoundRobin,
    LeastLoaded,
    BestPerformance,
    Adaptive,
}

// Task complexity levels are now imported from gpu_processor

/// Task result from distributed processing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DistributedTaskResult {
    pub subtask_id: Uuid,
    pub result_data: Vec<u8>,
    pub processing_time: Duration,
    pub node_id: String,
    pub confidence_score: f64,
    pub metadata: HashMap<String, String>,
}

/// Aggregated result from distributed processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedResult {
    pub task_id: Uuid,
    pub final_result: Vec<u8>,
    pub confidence_score: f64,
    pub contributing_nodes: Vec<String>,
    pub processing_time_total: Duration,
    pub consensus_achieved: bool,
    pub subtask_results: Vec<DistributedTaskResult>,
}

/// Distributed task for mesh processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedTask {
    pub id: Uuid,
    pub original_task: ComputationTask,
    pub subtasks: Vec<SubTask>,
    pub assigned_nodes: HashMap<String, SubTask>,
    pub status: DistributionStatus,
    pub created_at: SystemTime,
    pub deadline: SystemTime,
    pub consensus_threshold: f64,
}

/// Sub-task for distributed processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    pub id: Uuid,
    pub parent_task_id: Uuid,
    pub task_data: Vec<u8>,
    pub assigned_node: Option<String>,
    pub status: SubTaskStatus,
    pub complexity_score: f64,
    pub estimated_duration: Duration,
    pub complexity: GPUTaskComplexity,
    pub data_size: usize,
    pub deadline: SystemTime,
}

/// Sub-task status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SubTaskStatus {
    Pending,
    Assigned,
    Processing,
    Completed(DistributedTaskResult),
    Failed(String),
}

/// Distribution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DistributionStatus {
    Planning,
    Distributing,
    Processing,
    Collecting,
    Completed,
    Failed(String),
}

/// Distribution events for monitoring
#[derive(Debug, Clone)]
pub enum DistributionEvent {
    TaskDistributed(Uuid, Vec<String>),
    SubTaskCompleted(Uuid, Uuid, DistributedTaskResult),
    SubTaskFailed(Uuid, Uuid, String),
    TaskCompleted(Uuid, AggregatedResult),
    NodeCapabilityUpdated(String, DeviceCapability),
}

/// Task distributor for mesh network computation
pub struct TaskDistributor {
    peer_capabilities: Arc<RwLock<HashMap<String, DeviceCapability>>>,
    task_complexity_analyzer: Arc<ComplexityAnalyzer>,
    load_balancer: Arc<RwLock<LoadBalancer>>,
    active_tasks: Arc<RwLock<HashMap<Uuid, DistributedTask>>>,
    task_results: Arc<RwLock<HashMap<Uuid, Vec<DistributedTaskResult>>>>,
    gpu_scheduler: Arc<GPUTaskScheduler>,
    distribution_events: mpsc::Sender<DistributionEvent>,
}

impl TaskDistributor {
    /// Create a new task distributor
    pub fn new() -> (Self, mpsc::Receiver<DistributionEvent>) {
        let (distribution_events, distribution_rx) = mpsc::channel(100);
        let (gpu_scheduler, _) = GPUTaskScheduler::new();
        
        // Initialize load balancer and ensure strategy setter is exercised
        let mut initial_lb = LoadBalancer::new();
        initial_lb.set_strategy(BalancingStrategy::BestPerformance);

        let distributor = Self {
            peer_capabilities: Arc::new(RwLock::new(HashMap::new())),
            task_complexity_analyzer: Arc::new(ComplexityAnalyzer::new()),
            load_balancer: Arc::new(RwLock::new(initial_lb)),
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            task_results: Arc::new(RwLock::new(HashMap::new())),
            gpu_scheduler: Arc::new(gpu_scheduler),
            distribution_events,
        };
        
        (distributor, distribution_rx)
    }
    
    /// Register a peer device with capabilities
    pub async fn register_peer(&self, node_id: String, capability: DeviceCapability) -> Result<()> {
        {
            let mut capabilities = self.peer_capabilities.write().await;
            capabilities.insert(node_id.clone(), capability.clone());
        }
        
        // Send capability update event
        if let Err(e) = self.distribution_events.send(DistributionEvent::NodeCapabilityUpdated(node_id.clone(), capability.clone())).await {
            tracing::warn!("Failed to send capability update event: {}", e);
        }
        
        tracing::info!("Registered peer {} with {} CPU cores and {:.2} benchmark score", 
            node_id, capability.cpu_cores, capability.benchmark_score);
        Ok(())
    }

    /// Distribute a computation task to available nodes
    pub async fn distribute_task(&self, task: ComputationTask) -> Result<Uuid> {
        // Analyze task complexity using the complexity analyzer
        let complexity_score = self.task_complexity_analyzer.analyze_complexity(&task).await?;
        
        // Use load balancer to select optimal distribution strategy
        let distribution_strategy = self
            .load_balancer
            .read()
            .await
            .select_distribution_strategy(complexity_score)
            .await;
        
        // Create subtasks based on complexity analysis
        let subtasks = self.create_subtasks(&task, complexity_score).await?;
        
        // Assign subtasks to nodes using load balancing
        let node_assignments = self.assign_subtasks_to_nodes(&subtasks).await?;
        
        // Register task for tracking
        {
            let mut active_tasks = self.active_tasks.write().await;
            active_tasks.insert(task.id, DistributedTask {
                id: Uuid::new_v4(),
                original_task: task.clone(),
                subtasks: subtasks.clone(),
                assigned_nodes: node_assignments.clone(),
                status: DistributionStatus::Planning,
                created_at: SystemTime::now(),
                deadline: SystemTime::now() + Duration::from_secs(300), // 5 minutes default
                consensus_threshold: 0.67, // 67% consensus required
            });
        }
        
        // Send distribution event
        if let Err(e) = self.distribution_events.send(DistributionEvent::TaskDistributed(task.id, node_assignments.keys().cloned().collect())).await {
            tracing::warn!("Failed to send task distribution event: {}", e);
        }
        
        tracing::info!("Task {} distributed with complexity score {:.2} using {:?} strategy to {} nodes", 
            task.id, complexity_score, distribution_strategy, node_assignments.len());
        
        Ok(task.id)
    }

    /// Distribute a GPU-intensive task to available GPU nodes
    pub async fn distribute_gpu_task(&self, task: ComputationTask) -> Result<Uuid> {
        // Check if this task is GPU-intensive
        if !self.is_gpu_intensive(&task).await {
            return Err(anyhow::anyhow!("Task is not GPU-intensive"));
        }
        
        // Generate compute shader for GPU processing
        let compute_shader = self.generate_compute_shader(&task).await?;
        
        // Extract input data for GPU processing
        let input_data = self.extract_input_data(&task).await?;
        
        // Calculate expected output size
        let output_size = self.calculate_output_size(&task).await?;
        
        // Map task complexity to GPU complexity
        let gpu_complexity = self.map_task_complexity(&task).await;
        
        // Create GPU processing task using the correct structure
        let gpu_task = GPUProcessingTask {
            id: task.id,
            priority: task.priority.clone(),
            compute_shader,
            input_data,
            expected_output_size: output_size,
            deadline: SystemTime::now() + Duration::from_secs(600), // 10 minutes default
            complexity: gpu_complexity.clone(),
            assigned_node: None,
            status: TaskStatus::Pending,
        };
        
        // Submit task to GPU scheduler
        self.gpu_scheduler.submit_task(gpu_task).await?;
        
        // Send distribution event
        if let Err(e) = self.distribution_events.send(DistributionEvent::TaskDistributed(task.id, vec!["gpu_scheduler".to_string()])).await {
            tracing::warn!("Failed to send GPU task distribution event: {}", e);
        }
        
        tracing::info!("GPU task {} distributed to GPU scheduler with complexity {:?}", task.id, gpu_complexity.clone());
        Ok(task.id)
    }

    /// Check if a task requires GPU processing
    async fn is_gpu_intensive(&self, task: &ComputationTask) -> bool {
        match &task.task_type {
            TaskType::BlockValidation(_) => false, // CPU intensive
            TaskType::GameStateUpdate(_) => true,  // GPU intensive for rendering
            TaskType::TransactionValidation(_) => false, // CPU intensive
            TaskType::ConflictResolution(_) => true, // GPU intensive for simulations
        }
    }

    /// Generate compute shader for GPU task
    async fn generate_compute_shader(&self, task: &ComputationTask) -> Result<String> {
        match &task.task_type {
            TaskType::GameStateUpdate(_) => Ok("game_state_update_compute".to_string()),
            TaskType::ConflictResolution(_) => Ok("conflict_resolution_compute".to_string()),
            _ => Err(anyhow::anyhow!("Unsupported task type for GPU processing")),
        }
    }

    /// Extract input data for GPU processing
    async fn extract_input_data(&self, task: &ComputationTask) -> Result<Vec<f32>> {
        match &task.task_type {
            TaskType::GameStateUpdate(game_data) => {
                // Convert game state data to GPU-compatible format
                Ok(vec![
                    game_data.state_hash.len() as f32, // Use hash length as complexity indicator
                    1.0, // Default complexity factor
                    1.0, // Default priority
                ])
            }
            TaskType::ConflictResolution(conflict_data) => {
                // Convert conflict data to GPU-compatible format
                Ok(vec![
                    conflict_data.conflicting_actions.len() as f32, // Number of conflicting actions
                    1.0, // Default resolution difficulty
                    1.0, // Default priority score
                ])
            }
            _ => Err(anyhow::anyhow!("Unsupported task type for GPU processing")),
        }
    }

    /// Calculate expected output size for GPU task
    async fn calculate_output_size(&self, task: &ComputationTask) -> Result<usize> {
        match &task.task_type {
            TaskType::GameStateUpdate(_) => Ok(1024), // 1KB output
            TaskType::ConflictResolution(_) => Ok(2048), // 2KB output
            _ => Err(anyhow::anyhow!("Unsupported task type for GPU processing")),
        }
    }

    /// Map task complexity to GPU complexity levels
    async fn map_task_complexity(&self, task: &ComputationTask) -> GPUTaskComplexity {
        match task.priority {
            TaskPriority::Low => GPUTaskComplexity::Simple,
            TaskPriority::Normal => GPUTaskComplexity::Moderate,
            TaskPriority::High => GPUTaskComplexity::Complex,
            TaskPriority::Critical => GPUTaskComplexity::Extreme,
        }
    }
    
    /// Create subtasks from a main task
    async fn create_subtasks(&self, task: &ComputationTask, complexity_score: f64) -> Result<Vec<SubTask>> {
        let mut subtasks = Vec::new();
        
        match &task.task_type {
            TaskType::BlockValidation(block_data) => {
                // Split block validation into chunks
                let chunk_size = self.calculate_optimal_chunk_size(complexity_score).await;
                let chunks = self.split_block_into_chunks(block_data, chunk_size).await?;
                
                for (i, chunk) in chunks.iter().enumerate() {
                    let subtask = SubTask {
                        id: Uuid::new_v4(),
                        parent_task_id: task.id,
                        task_data: chunk.clone(),
                        assigned_node: None,
                        status: SubTaskStatus::Pending,
                        complexity_score: complexity_score / chunks.len() as f64,
                        estimated_duration: Duration::from_millis(100 * (i + 1) as u64),
                        complexity: self.determine_complexity(complexity_score / chunks.len() as f64),
                        data_size: chunk.len(),
                        deadline: SystemTime::now() + Duration::from_secs(300), // 5 minutes default
                    };
                    subtasks.push(subtask);
                }
            }
            TaskType::GameStateUpdate(game_data) => {
                // Split game state update into regions
                let regions = self.split_game_state_into_regions(game_data).await?;
                
                for (i, region) in regions.iter().enumerate() {
                    let subtask = SubTask {
                        id: Uuid::new_v4(),
                        parent_task_id: task.id,
                        task_data: region.clone(),
                        assigned_node: None,
                        status: SubTaskStatus::Pending,
                        complexity_score: complexity_score / regions.len() as f64,
                        estimated_duration: Duration::from_millis(200 * (i + 1) as u64),
                        complexity: self.determine_complexity(complexity_score / regions.len() as f64),
                        data_size: region.len(),
                        deadline: SystemTime::now() + Duration::from_secs(300), // 5 minutes default
                    };
                    subtasks.push(subtask);
                }
            }
            TaskType::TransactionValidation(tx_data) => {
                // Split transaction validation into parallel validations
                let validation_tasks = self.split_transaction_validation(tx_data).await?;
                
                for (i, validation_task) in validation_tasks.iter().enumerate() {
                    let subtask = SubTask {
                        id: Uuid::new_v4(),
                        parent_task_id: task.id,
                        task_data: validation_task.clone(),
                        assigned_node: None,
                        status: SubTaskStatus::Pending,
                        complexity_score: complexity_score / validation_tasks.len() as f64,
                        estimated_duration: Duration::from_millis(150 * (i + 1) as u64),
                        complexity: self.determine_complexity(complexity_score / validation_tasks.len() as f64),
                        data_size: validation_task.len(),
                        deadline: SystemTime::now() + Duration::from_secs(300), // 5 minutes default
                    };
                    subtasks.push(subtask);
                }
            }
            TaskType::ConflictResolution(conflict_data) => {
                // Split conflict resolution into analysis phases
                let phases = self.split_conflict_resolution(conflict_data).await?;
                
                for (i, phase) in phases.iter().enumerate() {
                    let subtask = SubTask {
                        id: Uuid::new_v4(),
                        parent_task_id: task.id,
                        task_data: phase.clone(),
                        assigned_node: None,
                        status: SubTaskStatus::Pending,
                        complexity_score: complexity_score / phases.len() as f64,
                        estimated_duration: Duration::from_millis(300 * (i + 1) as u64),
                        complexity: self.determine_complexity(complexity_score / phases.len() as f64),
                        data_size: phase.len(),
                        deadline: SystemTime::now() + Duration::from_secs(300), // 5 minutes default
                    };
                    subtasks.push(subtask);
                }
            }
        }
        
        Ok(subtasks)
    }
    
    /// Assign subtasks to optimal nodes
    async fn assign_subtasks_to_nodes(&self, subtasks: &[SubTask]) -> Result<HashMap<String, SubTask>> {
        let mut assignments = HashMap::new();
        let capabilities = self.peer_capabilities.read().await;
        
        // Sort subtasks by complexity (highest first)
        let mut sorted_subtasks: Vec<&SubTask> = subtasks.iter().collect();
        sorted_subtasks.sort_by(|a, b| b.complexity_score.partial_cmp(&a.complexity_score).unwrap());
        
        // Sort nodes by capability (best first)
        let mut sorted_nodes: Vec<(String, DeviceCapability)> = capabilities.iter()
            .map(|(id, cap)| (id.clone(), cap.clone()))
            .collect();
        sorted_nodes.sort_by(|a, b| b.1.benchmark_score.partial_cmp(&a.1.benchmark_score).unwrap());
        
        // Assign complex tasks to best nodes
        for subtask in sorted_subtasks {
            if let Some((node_id, _)) = self.find_best_node_for_subtask(subtask, &sorted_nodes).await {
                assignments.insert(node_id.clone(), subtask.clone());
                
                // Remove assigned node from available list
                sorted_nodes.retain(|(id, _)| id != &node_id);
            }
        }
        
        Ok(assignments)
    }
    
    /// Determine task complexity based on complexity score
    fn determine_complexity(&self, complexity_score: f64) -> GPUTaskComplexity {
        match complexity_score {
            0.0..=0.25 => GPUTaskComplexity::Simple,
            0.25..=0.5 => GPUTaskComplexity::Moderate,
            0.5..=0.75 => GPUTaskComplexity::Complex,
            _ => GPUTaskComplexity::Extreme,
        }
    }
    
    /// Find best node for a subtask
    async fn find_best_node_for_subtask(
        &self,
        subtask: &SubTask,
        available_nodes: &[(String, DeviceCapability)]
    ) -> Option<(String, DeviceCapability)> {
        let mut best_node: Option<(String, DeviceCapability)> = None;
        let mut best_score = 0.0;
        
        for (node_id, capability) in available_nodes.iter() {
            let score = self.calculate_node_fitness_score(subtask, capability).await;
            
            if score > best_score {
                best_score = score;
                best_node = Some((node_id.clone(), capability.clone()));
            }
        }
        
        best_node
    }
    
    /// Calculate fitness score for node-subtask pairing
    async fn calculate_node_fitness_score(&self, subtask: &SubTask, capability: &DeviceCapability) -> f64 {
        let mut score = capability.benchmark_score;
        
        // Load penalty
        let load_penalty = 1.0 - capability.current_load as f64;
        score *= load_penalty;
        
        // Memory bonus
        let memory_bonus = if capability.memory_gb >= 8.0 { 1.2 } else { 1.0 };
        score *= memory_bonus;
        
        // Thermal penalty
        let thermal_penalty = match capability.thermal_status {
            ThermalStatus::Cool => 1.0,
            ThermalStatus::Warm => 0.9,
            ThermalStatus::Hot => 0.7,
            ThermalStatus::Critical => 0.5,
        };
        score *= thermal_penalty;
        
        // Battery penalty (if available)
        if let Some(battery_level) = capability.battery_level {
            let battery_penalty = if battery_level > 0.5 { 1.0 } else { 0.8 };
            score *= battery_penalty;
        }
        
        // Subtask-specific optimizations
        let subtask_bonus = match subtask.complexity {
            GPUTaskComplexity::Simple => 1.0,
            GPUTaskComplexity::Moderate => 1.1,
            GPUTaskComplexity::Complex => 1.2,
            GPUTaskComplexity::Extreme => 1.3,
        };
        score *= subtask_bonus;
        
        // Data size optimization
        let data_size_factor = match subtask.data_size {
            0..=1024 => 1.0,      // Small data: no penalty
            1025..=8192 => 0.95,  // Medium data: slight penalty
            8193..=65536 => 0.9,  // Large data: moderate penalty
            _ => 0.8,             // Very large data: significant penalty
        };
        score *= data_size_factor;
        
        // Deadline urgency bonus
        let now = SystemTime::now();
        let time_until_deadline = subtask.deadline.duration_since(now).unwrap_or_default();
        let urgency_bonus = if time_until_deadline < Duration::from_secs(60) {
            1.5 // High urgency bonus
        } else if time_until_deadline < Duration::from_secs(300) {
            1.2 // Medium urgency bonus
        } else {
            1.0 // No urgency bonus
        };
        score *= urgency_bonus;
        
        score
    }
    
    /// Update task status
    async fn update_task_status(&self, task_id: Uuid, status: DistributionStatus) -> Result<()> {
        let mut active_tasks = self.active_tasks.write().await;
        if let Some(task) = active_tasks.get_mut(&task_id) {
            task.status = status.clone();
        }
        Ok(())
    }
    
    /// Record subtask completion
    pub async fn record_subtask_completion(&self, task_id: Uuid, subtask_id: Uuid, result: DistributedTaskResult) -> Result<()> {
        // Store the result
        {
            let mut results = self.task_results.write().await;
            let task_results = results.entry(task_id).or_insert_with(Vec::new);
            task_results.push(result.clone());
        }

        // Record complexity observation into analyzer history when possible
        if let Some(task) = self.active_tasks.read().await.get(&task_id) {
            // Attempt to fetch device capability for the reporting node
            if let Some(device_capability) = self.peer_capabilities.read().await.get(&result.node_id) {
                let record = ComplexityRecord {
                    task_type: task.original_task.task_type.clone(),
                    processing_time: result.processing_time,
                    device_capability: device_capability.clone(),
                    success: true,
                    timestamp: SystemTime::now(),
                };
                self.task_complexity_analyzer.record_observation(record).await;
            }
        }
        
        // Send completion event
        if let Err(e) = self.distribution_events.send(DistributionEvent::SubTaskCompleted(task_id, subtask_id, result)).await {
            tracing::warn!("Failed to send subtask completion event: {}", e);
        }
        
        // Check if all subtasks are completed
        self.check_task_completion(task_id).await?;
        
        Ok(())
    }

    /// Record subtask failure
    pub async fn record_subtask_failure(&self, task_id: Uuid, subtask_id: Uuid, error_message: String) -> Result<()> {
        // Update subtask status in active tasks
        {
            let mut active_tasks = self.active_tasks.write().await;
            if let Some(task) = active_tasks.get_mut(&task_id) {
                for subtask in &mut task.subtasks {
                    if subtask.id == subtask_id {
                        subtask.status = SubTaskStatus::Failed(error_message.clone());
                        break;
                    }
                }
            }
        }

        // Send failure event (eliminates SubTaskFailed warning)
        if let Err(e) = self.distribution_events.send(DistributionEvent::SubTaskFailed(task_id, subtask_id, error_message.clone())).await {
            tracing::warn!("Failed to send subtask failure event: {}", e);
        }

        // Check if task should be marked as failed or if retry is possible
        self.check_task_failure(task_id).await?;

        Ok(())
    }

    /// Check if a distributed task should be marked as failed
    async fn check_task_failure(&self, task_id: Uuid) -> Result<()> {
        let mut active_tasks = self.active_tasks.write().await;
        if let Some(task) = active_tasks.get_mut(&task_id) {
            let failed_count = task.subtasks.iter()
                .filter(|st| matches!(st.status, SubTaskStatus::Failed(_)))
                .count();

            // If more than 50% of subtasks failed, mark the entire task as failed
            if failed_count > task.subtasks.len() / 2 {
                task.status = DistributionStatus::Failed(format!("{} out of {} subtasks failed", failed_count, task.subtasks.len()));
                tracing::warn!("Task {} marked as failed due to excessive subtask failures", task_id);
            }
        }
        Ok(())
    }

    /// Check if a distributed task is complete
    async fn check_task_completion(&self, task_id: Uuid) -> Result<()> {
        let active_tasks = self.active_tasks.read().await;
        let results = self.task_results.read().await;
        
        if let Some(task) = active_tasks.get(&task_id) {
            if let Some(task_results) = results.get(&task_id) {
                if task_results.len() >= task.subtasks.len() {
                    // All subtasks completed, aggregate results
                    let aggregated_result = self.aggregate_results(task_id, task_results).await?;
                    
                    // Send task completion event
                    if let Err(e) = self.distribution_events.send(DistributionEvent::TaskCompleted(task_id, aggregated_result)).await {
                        tracing::warn!("Failed to send task completion event: {}", e);
                    }
                    
                    // Update task status
                    self.update_task_status(task_id, DistributionStatus::Completed).await?;
                    
                    tracing::info!("Distributed task {} completed with {} results", task_id, task_results.len());
                }
            }
        }
        
        Ok(())
    }
    
    /// Aggregate results from distributed processing
    async fn aggregate_results(&self, task_id: Uuid, results: &[DistributedTaskResult]) -> Result<AggregatedResult> {
        // Simple aggregation - take the result with highest confidence
        let best_result = results.iter()
            .max_by(|a, b| a.confidence_score.partial_cmp(&b.confidence_score).unwrap())
            .ok_or_else(|| anyhow::anyhow!("No results to aggregate"))?;
        
        let contributing_nodes: Vec<String> = results.iter()
            .map(|r| r.node_id.clone())
            .collect();
        
        let total_processing_time = results.iter()
            .map(|r| r.processing_time)
            .max()
            .unwrap_or(Duration::from_secs(0));
        
        let consensus_achieved = results.len() >= 2; // Simple consensus check
        
        Ok(AggregatedResult {
            task_id,
            final_result: best_result.result_data.clone(),
            confidence_score: best_result.confidence_score,
            contributing_nodes,
            processing_time_total: total_processing_time,
            consensus_achieved,
            subtask_results: results.to_vec(),
        })
    }
    
    /// Helper methods for task splitting
    async fn calculate_optimal_chunk_size(&self, complexity_score: f64) -> usize {
        // Simple heuristic based on complexity
        match complexity_score {
            s if s < 100.0 => 1024,
            s if s < 500.0 => 512,
            s if s < 1000.0 => 256,
            _ => 128,
        }
    }
    
    async fn split_block_into_chunks(&self, block_data: &BlockToValidate, chunk_size: usize) -> Result<Vec<Vec<u8>>> {
        let data = &block_data.data;
        let mut chunks = Vec::new();
        
        for chunk in data.chunks(chunk_size) {
            chunks.push(chunk.to_vec());
        }
        
        Ok(chunks)
    }
    
    async fn split_game_state_into_regions(&self, game_data: &GameStateData) -> Result<Vec<Vec<u8>>> {
        // Simple splitting - split data into 4 regions
        let data = &game_data.state_hash;
        let chunk_size = data.len() / 4;
        let mut regions = Vec::new();
        
        for i in 0..4 {
            let start = i * chunk_size;
            let end = if i == 3 { data.len() } else { (i + 1) * chunk_size };
            regions.push(data[start..end].to_vec());
        }
        
        Ok(regions)
    }
    
    async fn split_transaction_validation(&self, tx_data: &TransactionData) -> Result<Vec<Vec<u8>>> {
        // Split into signature validation, balance check, and state update
        let mut validations = Vec::new();
        
        // Signature validation (use data field as signature)
        validations.push(tx_data.data.clone());
        
        // Balance check
        validations.push(tx_data.value.to_le_bytes().to_vec());
        
        // State update
        validations.push(tx_data.to.clone().into_bytes());
        
        Ok(validations)
    }
    
    async fn split_conflict_resolution(&self, conflict_data: &ConflictData) -> Result<Vec<Vec<u8>>> {
        // Split into analysis, resolution, and verification phases
        let mut phases = Vec::new();
        
        // Analysis phase
        phases.push(conflict_data.conflicting_actions.first().map(|a| a.state_hash.clone()).unwrap_or_default());
        
        // Resolution phase
        phases.push(conflict_data.resolution_strategy.clone().into_bytes());
        
        // Verification phase
        phases.push(b"verification".to_vec());
        
        Ok(phases)
    }
    
    /// Get distributor statistics
    pub async fn get_stats(&self) -> DistributorStats {
        let capabilities = self.peer_capabilities.read().await;
        let active_tasks = self.active_tasks.read().await;
        let results = self.task_results.read().await;
        
        DistributorStats {
            registered_peers: capabilities.len(),
            active_distributions: active_tasks.len(),
            total_results_collected: results.values().map(|r| r.len()).sum(),
            average_peer_capability: capabilities.values().map(|c| c.benchmark_score).sum::<f64>() / capabilities.len() as f64,
        }
    }

    /// Expose load balancer health parameters for external monitors
    pub async fn health_params(&self) -> (Duration, u32, BalancingStrategy) {
        let lb = self.load_balancer.read().await;
        (
            lb.health_check_interval(),
            lb.failover_threshold(),
            lb.balancing_strategy(),
        )
    }

    /// Allow external controllers to adjust balancing strategy
    pub async fn set_balancing_strategy(&self, strategy: BalancingStrategy) {
        let mut lb = self.load_balancer.write().await;
        lb.set_strategy(strategy);
    }
}

/// Distributor statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributorStats {
    pub registered_peers: usize,
    pub active_distributions: usize,
    pub total_results_collected: usize,
    pub average_peer_capability: f64,
}

impl ComplexityAnalyzer {
    /// Create a new complexity analyzer
    pub fn new() -> Self {
        Self {
            complexity_cache: Arc::new(RwLock::new(HashMap::new())),
            historical_data: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Record an observation for post-hoc analysis
    pub async fn record_observation(&self, record: ComplexityRecord) {
        let mut history = self.historical_data.write().await;
        
        // Use timestamp to filter old records (older than 24 hours)
        let now = SystemTime::now();
        let twenty_four_hours_ago = now - Duration::from_secs(24 * 60 * 60);
        
        // Remove old records based on timestamp
        history.retain(|existing_record| {
            existing_record.timestamp > twenty_four_hours_ago
        });
        
        history.push(record);
        if history.len() > 1000 {
            // Keep bounded history
            history.remove(0);
        }
    }

    /// Analyze task complexity
    pub async fn analyze_complexity(&self, task: &ComputationTask) -> Result<f64> {
        // Check cache first
        {
            let cache = self.complexity_cache.read().await;
            if let Some(complexity) = cache.get(&task.task_type) {
                return Ok(*complexity);
            }
        }
        
        // Calculate complexity based on task type and data size
        let complexity = match &task.task_type {
            TaskType::BlockValidation(block_data) => {
                let data_size = block_data.data.len();
                let base_complexity = 100.0;
                base_complexity + (data_size as f64 * 0.1)
            }
            TaskType::GameStateUpdate(game_data) => {
                let state_size = game_data.state_hash.len();
                let base_complexity = 200.0;
                base_complexity + (state_size as f64 * 0.05)
            }
            TaskType::TransactionValidation(tx_data) => {
                let base_complexity = 150.0;
                base_complexity + (tx_data.value as f64 * 0.001)
            }
            TaskType::ConflictResolution(conflict_data) => {
                let data_size = conflict_data.conflicting_actions.len();
                let base_complexity = 300.0;
                base_complexity + (data_size as f64 * 0.2)
            }
        };
        
        // Cache the result
        {
            let mut cache = self.complexity_cache.write().await;
            cache.insert(task.task_type.clone(), complexity);
        }
        
        Ok(complexity)
    }
}

impl LoadBalancer {
    /// Create a new load balancer
    pub fn new() -> Self {
        Self {
            balancing_strategy: BalancingStrategy::BestPerformance,
            health_check_interval: Duration::from_secs(30),
            failover_threshold: 3,
        }
    }

    /// Select optimal distribution strategy based on complexity
    pub async fn select_distribution_strategy(&self, complexity_score: f64) -> BalancingStrategy {
        match complexity_score {
            0.0..=0.3 => {
                // Simple tasks: use round-robin for even distribution
                tracing::debug!("Selecting RoundRobin strategy for simple task (complexity: {:.2})", complexity_score);
                BalancingStrategy::RoundRobin
            },
            0.3..=0.7 => {
                // Moderate tasks: use least loaded nodes
                tracing::debug!("Selecting LeastLoaded strategy for moderate task (complexity: {:.2})", complexity_score);
                BalancingStrategy::LeastLoaded
            },
            _ => {
                // Complex tasks: use best performance nodes
                tracing::debug!("Selecting BestPerformance strategy for complex task (complexity: {:.2})", complexity_score);
                BalancingStrategy::BestPerformance
            }
        }
    }

    /// Set the balancing strategy
    pub fn set_strategy(&mut self, strategy: BalancingStrategy) {
        self.balancing_strategy = strategy.clone();
        tracing::info!("Load balancer strategy changed to {:?}", strategy);
    }

    /// Expose health check interval for monitoring loops
    pub fn health_check_interval(&self) -> Duration {
        self.health_check_interval
    }

    /// Expose failover threshold for monitoring logic
    pub fn failover_threshold(&self) -> u32 {
        self.failover_threshold
    }

    /// Get current strategy
    pub fn balancing_strategy(&self) -> BalancingStrategy {
        self.balancing_strategy.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_distributor_creation() {
        let (distributor, _) = TaskDistributor::new();
        let stats = distributor.get_stats().await;
        
        assert_eq!(stats.registered_peers, 0);
        assert_eq!(stats.active_distributions, 0);
    }

    #[tokio::test]
    async fn test_peer_registration() {
        let (distributor, _) = TaskDistributor::new();
        
        let capability = DeviceCapability {
            cpu_cores: 8,
            gpu_compute_units: Some(2048),
            memory_gb: 16.0,
            benchmark_score: 8500.0,
            current_load: 0.3,
            network_latency: Duration::from_millis(50),
            battery_level: Some(0.8),
            thermal_status: ThermalStatus::Cool,
        };
        
        distributor.register_peer("peer1".to_string(), capability).await.unwrap();
        
        let stats = distributor.get_stats().await;
        assert_eq!(stats.registered_peers, 1);
        assert!(stats.average_peer_capability > 8000.0);
    }
}
