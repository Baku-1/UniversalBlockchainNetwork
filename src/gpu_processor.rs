// src/gpu_processor.rs

use std::collections::HashMap;
use tokio::sync::{mpsc, RwLock};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use anyhow::Result;
use tracing;
use uuid::Uuid;
use crate::validator::TaskPriority;
use serde::{Deserialize, Serialize};

/// GPU processing capabilities for distributed computing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GPUCapability {
    pub compute_units: u32,
    pub memory_gb: f32,
    pub compute_capability: f32,
    pub max_workgroup_size: u32,
    pub supported_extensions: Vec<String>,
    pub benchmark_score: f64,
}

/// GPU processing task for distributed computation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GPUProcessingTask {
    pub id: Uuid,
    pub priority: TaskPriority,
    pub compute_shader: String,
    pub input_data: Vec<f32>,
    pub expected_output_size: usize,
    pub deadline: SystemTime,
    pub complexity: TaskComplexity,
    pub assigned_node: Option<String>,
    pub status: TaskStatus,
}

/// Task complexity levels for GPU processing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskComplexity {
    Simple,      // Basic vector operations
    Moderate,    // Matrix operations, simple algorithms
    Complex,     // Advanced algorithms, multiple passes
    Extreme,     // Machine learning, complex simulations
}

/// Task status for GPU processing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Assigned,
    Processing,
    Completed,
    Failed(String),
}

/// GPU task scheduler for distributed processing
pub struct GPUTaskScheduler {
    available_gpus: Arc<RwLock<HashMap<String, GPUCapability>>>,
    task_queue: Arc<RwLock<Vec<GPUProcessingTask>>>,
    active_tasks: Arc<RwLock<HashMap<Uuid, GPUProcessingTask>>>,
    completed_tasks: Arc<RwLock<HashMap<Uuid, TaskResult>>>,
    scheduler_events: mpsc::Sender<SchedulerEvent>,
}

/// Task result from GPU processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: Uuid,
    pub result_data: Vec<f32>,
    pub processing_time: Duration,
    pub node_id: String,
    pub confidence_score: f64,
    pub metadata: HashMap<String, String>,
}

/// Scheduler events for monitoring
#[derive(Debug, Clone)]
pub enum SchedulerEvent {
    TaskAssigned(Uuid, String),
    TaskCompleted(Uuid, TaskResult),
    TaskFailed(Uuid, String),
    GPURegistered(String, GPUCapability),
    GPURemoved(String),
}

impl GPUTaskScheduler {
    /// Create a new GPU task scheduler
    pub fn new() -> (Self, mpsc::Receiver<SchedulerEvent>) {
        let (scheduler_events, scheduler_rx) = mpsc::channel(100);
        
        let scheduler = Self {
            available_gpus: Arc::new(RwLock::new(HashMap::new())),
            task_queue: Arc::new(RwLock::new(Vec::new())),
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
            completed_tasks: Arc::new(RwLock::new(HashMap::new())),
            scheduler_events,
        };
        
        (scheduler, scheduler_rx)
    }
    
    /// Register a GPU node with the scheduler
    pub async fn register_gpu(&self, node_id: String, capability: GPUCapability) -> Result<()> {
        {
            let mut gpus = self.available_gpus.write().await;
            gpus.insert(node_id.clone(), capability.clone());
        }
        
        // Send registration event
        if let Err(e) = self.scheduler_events.send(SchedulerEvent::GPURegistered(node_id.clone(), capability.clone())).await {
            tracing::warn!("Failed to send GPU registration event: {}", e);
        }
        
        tracing::info!("Registered GPU node {} with {} compute units", node_id, capability.compute_units);
        Ok(())
    }
    
    /// Remove a GPU node from the scheduler
    pub async fn remove_gpu(&self, node_id: String) -> Result<()> {
        // Check if GPU has active tasks
        let has_active_tasks = {
            let active_tasks = self.active_tasks.read().await;
            active_tasks.values().any(|task| task.assigned_node.as_ref() == Some(&node_id))
        };
        
        if has_active_tasks {
            return Err(anyhow::anyhow!("Cannot remove GPU {} while it has active tasks", node_id));
        }
        
        // Remove from available GPUs
        {
            let mut gpus = self.available_gpus.write().await;
            gpus.remove(&node_id);
        }
        
        // Send removal event
        if let Err(e) = self.scheduler_events.send(SchedulerEvent::GPURemoved(node_id.clone())).await {
            tracing::warn!("Failed to send GPU removal event: {}", e);
        }
        
        tracing::info!("Removed GPU node {}", node_id);
        Ok(())
    }
    
    /// Submit a GPU processing task
    pub async fn submit_task(&self, task: GPUProcessingTask) -> Result<()> {
        {
            let mut queue = self.task_queue.write().await;
            queue.push(task.clone());
        }
        
        tracing::info!("Submitted GPU task {} with {:?} complexity", task.id, task.complexity);
        
        // Try to assign task immediately
        self.try_assign_pending_tasks().await?;
        
        Ok(())
    }
    
    /// Try to assign pending tasks to available GPUs
    async fn try_assign_pending_tasks(&self) -> Result<()> {
        let mut queue = self.task_queue.write().await;
        let gpus = self.available_gpus.read().await;
        let mut active_tasks = self.active_tasks.write().await;
        
        let mut assigned_count = 0;
        
        // Sort tasks by priority and deadline
        queue.sort_by(|a, b| {
            // First by deadline (earlier deadline = higher priority)
            let deadline_cmp = a.deadline.cmp(&b.deadline);
            if deadline_cmp != std::cmp::Ordering::Equal {
                return deadline_cmp;
            }
            
            // Then by task ID for consistency
            a.id.cmp(&b.id)
        });
        
        let mut i = 0;
        while i < queue.len() {
            let task_id = queue[i].id;
            let task = &queue[i];
            
            // Find best GPU for this task
            if let Some((node_id, capability)) = self.find_best_gpu_for_task(task, &gpus).await {
                // Check if GPU can handle the task
                if self.can_gpu_handle_task(task, &capability).await {
                    // Assign task to GPU
                    let mut assigned_task = task.clone();
                    assigned_task.assigned_node = Some(node_id.clone());
                    assigned_task.status = TaskStatus::Assigned;
                    
                    // Add to active tasks
                    active_tasks.insert(task_id, assigned_task);
                    
                    // Start processing the task (this will emit TaskCompleted/TaskFailed events)
                    self.start_task_processing(task_id, node_id.clone()).await?;
                    
                    // Remove from queue
                    queue.remove(i);
                    
                    // Send assignment event
                    if let Err(e) = self.scheduler_events.send(SchedulerEvent::TaskAssigned(task_id, node_id)).await {
                        tracing::warn!("Failed to send task assignment event: {}", e);
                    }
                    
                    assigned_count += 1;
                    continue; // Don't increment i since we removed an element
                }
            }
            
            i += 1;
        }
        
        if assigned_count > 0 {
            tracing::info!("Assigned {} GPU tasks to available nodes", assigned_count);
        }
        
        Ok(())
    }
    
    /// Start processing a task on the assigned GPU
    async fn start_task_processing(&self, task_id: Uuid, node_id: String) -> Result<()> {
        // Get the task details
        let task = {
            let active_tasks = self.active_tasks.read().await;
            active_tasks.get(&task_id).cloned()
        };
        
        if let Some(task) = task {
            // Get the GPU capability
            let gpu_capability = {
                let gpus = self.available_gpus.read().await;
                gpus.get(&node_id).cloned()
            };
            
            if let Some(capability) = gpu_capability {
                // Create a GPU processor for this task
                let (mut processor, _) = GPUProcessor::new(node_id.clone(), capability).await?;
                
                // Process the task in a separate task to avoid blocking
                let scheduler_events = self.scheduler_events.clone();
                let task_id_clone = task_id;
                
                tokio::spawn(async move {
                    match processor.process_task(task).await {
                        Ok(result) => {
                            // Send task completed event
                            if let Err(e) = scheduler_events.send(SchedulerEvent::TaskCompleted(task_id_clone, result)).await {
                                tracing::warn!("Failed to send task completed event: {}", e);
                            }
                        }
                        Err(e) => {
                            // Send task failed event
                            if let Err(e) = scheduler_events.send(SchedulerEvent::TaskFailed(task_id_clone, e.to_string())).await {
                                tracing::warn!("Failed to send task failed event: {}", e);
                            }
                        }
                    }
                });
            }
        }
        
        Ok(())
    }
    
    /// Find the best GPU for a given task
    async fn find_best_gpu_for_task(
        &self,
        task: &GPUProcessingTask,
        gpus: &HashMap<String, GPUCapability>
    ) -> Option<(String, GPUCapability)> {
        let mut best_gpu: Option<(String, GPUCapability)> = None;
        let mut best_score = 0.0;
        
        for (node_id, capability) in gpus.iter() {
            let score = self.calculate_gpu_fitness_score(task, capability).await;
            
            if score > best_score {
                best_score = score;
                best_gpu = Some((node_id.clone(), capability.clone()));
            }
        }
        
        best_gpu
    }
    
    /// Calculate fitness score for GPU-task pairing
    async fn calculate_gpu_fitness_score(&self, task: &GPUProcessingTask, capability: &GPUCapability) -> f64 {
        let score = capability.benchmark_score;
        
        // Complexity matching bonus
        let complexity_bonus = match (task.complexity.clone(), capability.compute_units) {
            (TaskComplexity::Simple, _) => 1.0,
            (TaskComplexity::Moderate, units) if units >= 2 => 1.2,
            (TaskComplexity::Complex, units) if units >= 4 => 1.5,
            (TaskComplexity::Extreme, units) if units >= 8 => 2.0,
            _ => 0.5, // Penalty for mismatched complexity
        };
        
        // Memory requirement bonus
        let memory_bonus = if capability.memory_gb >= 4.0 { 1.1 } else { 0.9 };
        
        score * complexity_bonus * memory_bonus
    }
    
    /// Check if GPU can handle the task
    async fn can_gpu_handle_task(&self, task: &GPUProcessingTask, capability: &GPUCapability) -> bool {
        // Check compute units requirement
        let required_units = match task.complexity {
            TaskComplexity::Simple => 1,
            TaskComplexity::Moderate => 2,
            TaskComplexity::Complex => 4,
            TaskComplexity::Extreme => 8,
        };
        
        if capability.compute_units < required_units {
            return false;
        }
        
        // Check memory requirement (rough estimate)
        let estimated_memory_gb = (task.input_data.len() * 4) as f32 / 1_000_000_000.0; // 4 bytes per f32
        if capability.memory_gb < estimated_memory_gb * 2.0 { // 2x buffer
            return false;
        }
        
        true
    }
    
    /// Get scheduler statistics
    pub async fn get_stats(&self) -> SchedulerStats {
        let queue = self.task_queue.read().await;
        let active = self.active_tasks.read().await;
        let completed = self.completed_tasks.read().await;
        let gpus = self.available_gpus.read().await;
        
        SchedulerStats {
            pending_tasks: queue.len(),
            active_tasks: active.len(),
            completed_tasks: completed.len(),
            available_gpus: gpus.len(),
            total_compute_units: gpus.values().map(|g| g.compute_units).sum(),
            average_benchmark_score: gpus.values().map(|g| g.benchmark_score).sum::<f64>() / gpus.len() as f64,
        }
    }
}

/// Scheduler statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerStats {
    pub pending_tasks: usize,
    pub active_tasks: usize,
    pub completed_tasks: usize,
    pub available_gpus: usize,
    pub total_compute_units: u32,
    pub average_benchmark_score: f64,
}

/// GPU processor for individual nodes
pub struct GPUProcessor {
    node_id: String,
    capability: GPUCapability,
    current_task: Option<GPUProcessingTask>,
    task_events: mpsc::Sender<TaskEvent>,
}

/// Task events for GPU processing
#[derive(Debug, Clone)]
pub enum TaskEvent {
    TaskStarted(Uuid),
    TaskProgress(Uuid, f32), // Progress percentage
    TaskCompleted(Uuid, TaskResult),
    TaskFailed(Uuid, String),
}

impl GPUProcessor {
    /// Create a new GPU processor
    pub async fn new(node_id: String, capability: GPUCapability) -> Result<(Self, mpsc::Receiver<TaskEvent>)> {
        let (task_events, task_events_rx) = mpsc::channel(100);
        
        Ok((Self {
            node_id,
            capability,
            current_task: None,
            task_events,
        }, task_events_rx))
    }
    
    /// Process a GPU task
    pub async fn process_task(&mut self, task: GPUProcessingTask) -> Result<TaskResult> {
        // Send task started event
        if let Err(e) = self.task_events.send(TaskEvent::TaskStarted(task.id)).await {
            tracing::warn!("Failed to send task started event: {}", e);
        }
        
        self.current_task = Some(task.clone());
        
        let start_time = SystemTime::now();
        
        // Simulate GPU processing based on task complexity
        let processing_time = self.simulate_gpu_processing(&task).await?;
        
        // Send progress updates during processing
        let progress_interval = processing_time.div_f32(4.0); // 4 progress updates
        for i in 1..=4 {
            let progress = i as f32 / 4.0;
            if let Err(e) = self.task_events.send(TaskEvent::TaskProgress(task.id, progress)).await {
                tracing::warn!("Failed to send task progress event: {}", e);
            }
            tokio::time::sleep(progress_interval).await;
        }
        
        // Generate result data
        let result_data = self.generate_result_data(&task).await?;
        
        let end_time = SystemTime::now();
        let actual_processing_time = end_time.duration_since(start_time).unwrap();
        
        let result = TaskResult {
            task_id: task.id,
            result_data,
            processing_time: actual_processing_time,
            node_id: self.node_id.clone(),
            confidence_score: self.calculate_confidence_score(&task).await,
            metadata: HashMap::new(),
        };
        
        // Send task completed event
        if let Err(e) = self.task_events.send(TaskEvent::TaskCompleted(task.id, result.clone())).await {
            tracing::warn!("Failed to send task completed event: {}", e);
        }
        
        self.current_task = None;
        
        tracing::info!("Completed GPU task {} in {:?}", task.id, actual_processing_time);
        Ok(result)
    }
    
    /// Simulate GPU processing time
    async fn simulate_gpu_processing(&self, task: &GPUProcessingTask) -> Result<Duration> {
        let base_time = match task.complexity {
            TaskComplexity::Simple => Duration::from_millis(100),
            TaskComplexity::Moderate => Duration::from_millis(500),
            TaskComplexity::Complex => Duration::from_millis(2000),
            TaskComplexity::Extreme => Duration::from_millis(5000),
        };
        
        // Adjust based on GPU capability
        let capability_factor = 1.0 / (self.capability.benchmark_score / 1000.0);
        let adjusted_time = base_time.mul_f64(capability_factor);
        
        // Simulate actual processing
        tokio::time::sleep(adjusted_time).await;
        
        Ok(adjusted_time)
    }
    
    /// Generate result data for the task
    async fn generate_result_data(&self, task: &GPUProcessingTask) -> Result<Vec<f32>> {
        // Simulate GPU computation result
        let mut result = Vec::with_capacity(task.expected_output_size);
        
        for i in 0..task.expected_output_size {
            // Simple computation simulation
            let input_value = if i < task.input_data.len() {
                task.input_data[i]
        } else {
                0.0
            };
            
            // Apply some computation (e.g., multiply by GPU capability factor)
            let computed_value = input_value * (self.capability.benchmark_score / 1000.0) as f32;
            result.push(computed_value);
        }
        
        Ok(result)
    }
    
    /// Calculate confidence score for the result
    async fn calculate_confidence_score(&self, task: &GPUProcessingTask) -> f64 {
        let base_confidence = 0.95;
        
        // Adjust based on GPU capability
        let capability_bonus = (self.capability.benchmark_score / 1000.0).min(0.05);
        
        // Adjust based on task complexity
        let complexity_bonus = match task.complexity {
            TaskComplexity::Simple => 0.02,
            TaskComplexity::Moderate => 0.01,
            TaskComplexity::Complex => 0.0,
            TaskComplexity::Extreme => -0.01,
        };
        
        (base_confidence + capability_bonus + complexity_bonus).max(0.8).min(1.0)
    }
    
    /// Get current GPU status
    pub fn get_status(&self) -> GPUStatus {
        GPUStatus {
            node_id: self.node_id.clone(),
            capability: self.capability.clone(),
            is_processing: self.current_task.is_some(),
            current_task_id: self.current_task.as_ref().map(|t| t.id),
        }
    }
}

/// GPU status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GPUStatus {
    pub node_id: String,
    pub capability: GPUCapability,
    pub is_processing: bool,
    pub current_task_id: Option<Uuid>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gpu_scheduler_creation() {
        let (scheduler, _) = GPUTaskScheduler::new();
        let stats = scheduler.get_stats().await;
        
        assert_eq!(stats.pending_tasks, 0);
        assert_eq!(stats.available_gpus, 0);
    }

    #[tokio::test]
    async fn test_gpu_registration() {
        let (scheduler, _) = GPUTaskScheduler::new();
        
        let capability = GPUCapability {
            compute_units: 8,
            memory_gb: 16.0,
            compute_capability: 8.6,
            max_workgroup_size: 1024,
            supported_extensions: vec!["compute".to_string()],
            benchmark_score: 8500.0,
        };
        
        scheduler.register_gpu("gpu1".to_string(), capability).await.unwrap();
        
        let stats = scheduler.get_stats().await;
        assert_eq!(stats.available_gpus, 1);
        assert_eq!(stats.total_compute_units, 8);
    }
}
