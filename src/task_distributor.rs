// src/task_distributor.rs

use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;
use anyhow::Result;
use tracing;
use serde::{Deserialize, Serialize};
use crate::validator::{ComputationTask, TaskType, TaskPriority, BlockToValidate, GameStateData, TransactionData, ConflictData};
use crate::gpu_processor::{GPUCapability, GPUProcessor};

/// Device capability information for task distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCapability {
    pub node_id: String,
    pub cpu_cores: u32,
    pub gpu_compute_units: Option<u32>,
    pub memory_gb: f32,
    pub benchmark_score: f64,
    pub current_load: f32, // 0.0 to 1.0
    pub network_latency_ms: u64,
    pub is_online: bool,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
    pub supported_task_types: Vec<TaskType>,
    pub gpu_capability: Option<GPUCapability>,
}

/// Task complexity analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskComplexity {
    pub score: f64,
    pub requires_gpu: bool,
    pub estimated_duration_ms: u64,
    pub memory_requirement_mb: u64,
    pub cpu_intensity: f64, // 0.0 to 1.0
    pub gpu_intensity: f64, // 0.0 to 1.0
    pub network_dependency: f64, // 0.0 to 1.0
}

/// Load balancing strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoadBalancingStrategy {
    RoundRobin,
    LeastLoaded,
    PerformanceBased,
    LatencyOptimized,
    Hybrid,
}

/// Task distribution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDistributionResult {
    pub task_id: String,
    pub assigned_nodes: Vec<String>,
    pub distribution_strategy: LoadBalancingStrategy,
    pub estimated_completion_time_ms: u64,
    pub confidence_score: f64,
    pub load_balance_score: f64,
}

/// Task distributor for intelligent task distribution
pub struct TaskDistributor {
    pub peer_capabilities: Arc<RwLock<HashMap<String, DeviceCapability>>>,
    pub task_complexity_analyzer: ComplexityAnalyzer,
    pub load_balancer: LoadBalancer,
    pub distribution_history: Arc<RwLock<Vec<TaskDistributionResult>>>,
    pub performance_metrics: Arc<RwLock<PerformanceMetrics>>,
}

/// Complexity analyzer for task requirements
pub struct ComplexityAnalyzer {
    pub complexity_weights: HashMap<TaskType, f64>,
    pub gpu_thresholds: HashMap<TaskType, f64>,
    pub memory_estimates: HashMap<TaskType, u64>,
}

/// Load balancer for optimal node selection
pub struct LoadBalancer {
    pub strategy: LoadBalancingStrategy,
    pub performance_weights: PerformanceWeights,
    pub load_threshold: f32,
}

/// Performance weights for different factors
#[derive(Debug, Clone)]
pub struct PerformanceWeights {
    pub cpu_weight: f64,
    pub gpu_weight: f64,
    pub memory_weight: f64,
    pub network_weight: f64,
    pub load_weight: f64,
}

/// Performance metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_tasks_distributed: u64,
    pub average_completion_time_ms: u64,
    pub load_balance_efficiency: f64,
    pub node_utilization: HashMap<String, f64>,
    pub task_success_rate: f64,
}

impl TaskDistributor {
    /// Create a new task distributor
    pub fn new() -> Self {
        Self {
            peer_capabilities: Arc::new(RwLock::new(HashMap::new())),
            task_complexity_analyzer: ComplexityAnalyzer::new(),
            load_balancer: LoadBalancer::new(),
            distribution_history: Arc::new(RwLock::new(Vec::new())),
            performance_metrics: Arc::new(RwLock::new(PerformanceMetrics::new())),
        }
    }

    /// Register a peer device capability
    pub async fn register_peer_capability(&self, node_id: String, capability: DeviceCapability) -> Result<()> {
        let mut capabilities = self.peer_capabilities.write().await;
        capabilities.insert(node_id.clone(), capability);
        
        tracing::info!("Registered peer capability for node: {}", node_id);
        Ok(())
    }

    /// Update peer capability
    pub async fn update_peer_capability(&self, node_id: &str, updates: DeviceCapabilityUpdate) -> Result<bool> {
        let mut capabilities = self.peer_capabilities.write().await;
        
        if let Some(capability) = capabilities.get_mut(node_id) {
            if let Some(current_load) = updates.current_load {
                capability.current_load = current_load;
            }
            if let Some(is_online) = updates.is_online {
                capability.is_online = is_online;
            }
            if let Some(benchmark_score) = updates.benchmark_score {
                capability.benchmark_score = benchmark_score;
            }
            
            capability.last_heartbeat = chrono::Utc::now();
            tracing::debug!("Updated capability for node: {}", node_id);
            return Ok(true);
        }
        
        Ok(false)
    }

    /// Distribute task across mesh network
    pub async fn distribute_task(
        &self,
        task: &ComputationTask,
        available_peers: &[String],
    ) -> Result<TaskDistributionResult> {
        // Analyze task complexity
        let complexity = self.task_complexity_analyzer.analyze_complexity(task);
        
        // Get available peer capabilities
        let capabilities = self.peer_capabilities.read().await;
        let available_capabilities: Vec<_> = available_peers
            .iter()
            .filter_map(|peer_id| capabilities.get(peer_id).cloned())
            .filter(|cap| cap.is_online && cap.current_load < self.load_balancer.load_threshold)
            .collect();

        if available_capabilities.is_empty() {
            return Err(anyhow::anyhow!("No suitable peers available for task distribution"));
        }

        // Select optimal nodes based on strategy
        let performance_metrics = self.performance_metrics.read().await;
        let selected_nodes = self.load_balancer.select_optimal_nodes(
            &available_capabilities,
            &complexity,
            &*performance_metrics,
        ).await?;

        // Create distribution result
        let result = TaskDistributionResult {
            task_id: task.id.to_string(),
            assigned_nodes: selected_nodes.iter().map(|cap| cap.node_id.clone()).collect(),
            distribution_strategy: self.load_balancer.strategy.clone(),
            estimated_completion_time_ms: self.estimate_completion_time(&selected_nodes, &complexity),
            confidence_score: self.calculate_confidence_score(&selected_nodes, &complexity),
            load_balance_score: self.calculate_load_balance_score(&selected_nodes),
        };

        // Record distribution
        {
            let mut history = self.distribution_history.write().await;
            history.push(result.clone());
            
            // Keep only last 1000 distributions
            if history.len() > 1000 {
                history.remove(0);
            }
        }

        // Update performance metrics
        self.update_performance_metrics(&result).await;

        tracing::info!("Distributed task {} to {} nodes using {:?} strategy", 
            task.id, selected_nodes.len(), self.load_balancer.strategy);

        Ok(result)
    }

    /// Estimate task completion time
    fn estimate_completion_time(&self, nodes: &[DeviceCapability], complexity: &TaskComplexity) -> u64 {
        if nodes.is_empty() {
            return complexity.estimated_duration_ms;
        }

        // Calculate parallel processing time
        let parallel_factor = if complexity.requires_gpu {
            nodes.iter().filter(|n| n.gpu_capability.is_some()).count() as f64
        } else {
            nodes.len() as f64
        };

        let base_time = complexity.estimated_duration_ms as f64;
        let parallel_time = base_time / parallel_factor.max(1.0);

        // Add network latency overhead
        let avg_latency = nodes.iter().map(|n| n.network_latency_ms as f64).sum::<f64>() / nodes.len() as f64;
        let total_time = parallel_time + avg_latency as f64;

        total_time as u64
    }

    /// Calculate confidence score for distribution
    fn calculate_confidence_score(&self, nodes: &[DeviceCapability], complexity: &TaskComplexity) -> f64 {
        if nodes.is_empty() {
            return 0.0;
        }

        let mut total_score = 0.0;
        let mut factors = 0;

        // Node reliability factor
        let reliability_score: f64 = nodes.iter()
            .map(|n| if n.is_online { 1.0 } else { 0.0 })
            .sum::<f64>() / nodes.len() as f64;
        total_score += reliability_score;
        factors += 1;

        // Performance capability factor
        let performance_score: f64 = nodes.iter()
            .map(|n| n.benchmark_score)
            .sum::<f64>() / nodes.len() as f64;
        total_score += (performance_score / 1000.0).min(1.0); // Normalize to 0-1
        factors += 1;

        // Load balance factor
        let load_variance = self.calculate_load_variance(nodes);
        let load_score = 1.0 - load_variance;
        total_score += load_score;
        factors += 1;

        // GPU availability factor (if required)
        if complexity.requires_gpu {
            let gpu_availability = nodes.iter()
                .filter(|n| n.gpu_capability.is_some())
                .count() as f64 / nodes.len() as f64;
            total_score += gpu_availability;
            factors += 1;
        }

        total_score / factors as f64
    }

    /// Calculate load balance score
    fn calculate_load_balance_score(&self, nodes: &[DeviceCapability]) -> f64 {
        if nodes.len() < 2 {
            return 1.0;
        }

        let loads: Vec<f32> = nodes.iter().map(|n| n.current_load).collect();
        let mean_load = loads.iter().sum::<f32>() / loads.len() as f32;
        
        let variance = loads.iter()
            .map(|&load| (load - mean_load).powi(2))
            .sum::<f32>() / loads.len() as f32;
        
        let std_dev = variance.sqrt();
        let coefficient_of_variation = if mean_load > 0.0 { std_dev / mean_load } else { 0.0 };
        
        // Convert to 0-1 score (lower variance = higher score)
        (1.0 - coefficient_of_variation).max(0.0) as f64
    }

    /// Calculate load variance
    fn calculate_load_variance(&self, nodes: &[DeviceCapability]) -> f64 {
        if nodes.len() < 2 {
            return 0.0;
        }

        let loads: Vec<f64> = nodes.iter().map(|n| n.current_load as f64).collect();
        let mean_load = loads.iter().sum::<f64>() / loads.len() as f64;
        
        let variance = loads.iter()
            .map(|&load| (load - mean_load).powi(2))
            .sum::<f64>() / loads.len() as f64;
        
        variance
    }

    /// Update performance metrics
    async fn update_performance_metrics(&self, result: &TaskDistributionResult) {
        let mut metrics = self.performance_metrics.write().await;
        
        metrics.total_tasks_distributed += 1;
        
        // Update average completion time
        let current_avg = metrics.average_completion_time_ms;
        let new_count = metrics.total_tasks_distributed;
        metrics.average_completion_time_ms = 
            ((current_avg * (new_count - 1) as u64) + result.estimated_completion_time_ms) / new_count;
        
        // Update load balance efficiency
        metrics.load_balance_efficiency = 
            (metrics.load_balance_efficiency + result.load_balance_score) / 2.0;
        
        // Update node utilization
        for node_id in &result.assigned_nodes {
            let utilization = metrics.node_utilization.entry(node_id.clone()).or_insert(0.0);
            *utilization = (*utilization + 0.1).min(1.0); // Increment utilization
        }
    }

    /// Get distribution statistics
    pub async fn get_distribution_stats(&self) -> DistributionStats {
        let capabilities = self.peer_capabilities.read().await;
        let history = self.distribution_history.read().await;
        let metrics = self.performance_metrics.read().await;
        
        let online_nodes = capabilities.values().filter(|cap| cap.is_online).count();
        let total_nodes = capabilities.len();
        
        DistributionStats {
            total_peers: total_nodes,
            online_peers: online_nodes,
            total_tasks_distributed: metrics.total_tasks_distributed,
            average_completion_time_ms: metrics.average_completion_time_ms,
            load_balance_efficiency: metrics.load_balance_efficiency,
            task_success_rate: metrics.task_success_rate,
            recent_distributions: history.len(),
        }
    }

    /// Get peer capabilities
    pub async fn get_peer_capabilities(&self) -> HashMap<String, DeviceCapability> {
        self.peer_capabilities.read().await.clone()
    }

    /// Remove offline peers
    pub async fn cleanup_offline_peers(&self, timeout_minutes: u64) -> Result<usize> {
        let mut capabilities = self.peer_capabilities.write().await;
        let timeout_duration = chrono::Duration::minutes(timeout_minutes as i64);
        let cutoff_time = chrono::Utc::now() - timeout_duration;
        
        let initial_count = capabilities.len();
        
        capabilities.retain(|_, cap| {
            cap.is_online && cap.last_heartbeat > cutoff_time
        });
        
        let removed_count = initial_count - capabilities.len();
        
        if removed_count > 0 {
            tracing::info!("Removed {} offline peers", removed_count);
        }
        
        Ok(removed_count)
    }
}

impl ComplexityAnalyzer {
    /// Create a new complexity analyzer
    pub fn new() -> Self {
        let mut weights = HashMap::new();
        weights.insert(TaskType::BlockValidation(BlockToValidate { id: "".to_string(), data: vec![] }), 1.0);
        weights.insert(TaskType::GameStateUpdate(GameStateData { player_id: "".to_string(), action: "".to_string(), timestamp: std::time::SystemTime::now(), state_hash: vec![] }), 0.5);
        weights.insert(TaskType::TransactionValidation(TransactionData { from: "".to_string(), to: "".to_string(), value: 0, gas_price: 0, nonce: 0, data: vec![] }), 0.3);
        weights.insert(TaskType::ConflictResolution(ConflictData { conflicting_actions: vec![], resolution_strategy: "".to_string() }), 0.8);

        let mut gpu_thresholds = HashMap::new();
        gpu_thresholds.insert(TaskType::BlockValidation(BlockToValidate { id: "".to_string(), data: vec![] }), 0.7);
        gpu_thresholds.insert(TaskType::GameStateUpdate(GameStateData { player_id: "".to_string(), action: "".to_string(), timestamp: std::time::SystemTime::now(), state_hash: vec![] }), 0.3);
        gpu_thresholds.insert(TaskType::TransactionValidation(TransactionData { from: "".to_string(), to: "".to_string(), value: 0, gas_price: 0, nonce: 0, data: vec![] }), 0.1);
        gpu_thresholds.insert(TaskType::ConflictResolution(ConflictData { conflicting_actions: vec![], resolution_strategy: "".to_string() }), 0.6);

        let mut memory_estimates = HashMap::new();
        memory_estimates.insert(TaskType::BlockValidation(BlockToValidate { id: "".to_string(), data: vec![] }), 512); // 512 MB
        memory_estimates.insert(TaskType::GameStateUpdate(GameStateData { player_id: "".to_string(), action: "".to_string(), timestamp: std::time::SystemTime::now(), state_hash: vec![] }), 256); // 256 MB
        memory_estimates.insert(TaskType::TransactionValidation(TransactionData { from: "".to_string(), to: "".to_string(), value: 0, gas_price: 0, nonce: 0, data: vec![] }), 128); // 128 MB
        memory_estimates.insert(TaskType::ConflictResolution(ConflictData { conflicting_actions: vec![], resolution_strategy: "".to_string() }), 1024); // 1 GB

        Self {
            complexity_weights: weights,
            gpu_thresholds: gpu_thresholds,
            memory_estimates: memory_estimates,
        }
    }

    /// Analyze task complexity
    pub fn analyze_complexity(&self, task: &ComputationTask) -> TaskComplexity {
        let base_complexity = self.complexity_weights.get(&task.task_type).unwrap_or(&1.0);
        let data_complexity = (task.data.len() as f64 / 1024.0).min(10.0); // Cap at 10x
        
        let complexity_score = base_complexity * data_complexity;
        
        // Determine GPU requirement
        let gpu_threshold = self.gpu_thresholds.get(&task.task_type).unwrap_or(&0.5);
        let requires_gpu = complexity_score > *gpu_threshold;
        
        // Estimate memory requirement
        let base_memory = self.memory_estimates.get(&task.task_type).unwrap_or(&256);
        let memory_requirement = base_memory * (data_complexity as u64 / 2).max(1);
        
        // Calculate intensity factors
        let cpu_intensity = if requires_gpu { 0.3 } else { 0.8 };
        let gpu_intensity = if requires_gpu { 0.8 } else { 0.1 };
        let network_dependency = match task.task_type {
            TaskType::BlockValidation(_) => 0.2,
            TaskType::GameStateUpdate(_) => 0.6,
            TaskType::TransactionValidation(_) => 0.4,
            TaskType::ConflictResolution(_) => 0.7,
        };

        TaskComplexity {
            score: complexity_score,
            requires_gpu,
            estimated_duration_ms: (complexity_score * 100.0) as u64,
            memory_requirement_mb: memory_requirement,
            cpu_intensity,
            gpu_intensity,
            network_dependency,
        }
    }
}

impl LoadBalancer {
    /// Create a new load balancer
    pub fn new() -> Self {
        Self {
            strategy: LoadBalancingStrategy::Hybrid,
            performance_weights: PerformanceWeights {
                cpu_weight: 0.25,
                gpu_weight: 0.25,
                memory_weight: 0.20,
                network_weight: 0.15,
                load_weight: 0.15,
            },
            load_threshold: 0.8, // 80% load threshold
        }
    }

    /// Select optimal nodes for task
    pub async fn select_optimal_nodes(
        &self,
        available_capabilities: &[DeviceCapability],
        complexity: &TaskComplexity,
        performance_metrics: &PerformanceMetrics,
    ) -> Result<Vec<DeviceCapability>> {
        match self.strategy {
            LoadBalancingStrategy::RoundRobin => self.round_robin_selection(available_capabilities),
            LoadBalancingStrategy::LeastLoaded => self.least_loaded_selection(available_capabilities),
            LoadBalancingStrategy::PerformanceBased => self.performance_based_selection(available_capabilities, complexity),
            LoadBalancingStrategy::LatencyOptimized => self.latency_optimized_selection(available_capabilities),
            LoadBalancingStrategy::Hybrid => self.hybrid_selection(available_capabilities, complexity, performance_metrics).await,
        }
    }

    /// Round-robin selection
    fn round_robin_selection(&self, capabilities: &[DeviceCapability]) -> Result<Vec<DeviceCapability>> {
        // Simple round-robin: take first few available nodes
        let selected_count = (capabilities.len() / 2).max(1).min(4);
        Ok(capabilities.iter().take(selected_count).cloned().collect())
    }

    /// Least loaded selection
    fn least_loaded_selection(&self, capabilities: &[DeviceCapability]) -> Result<Vec<DeviceCapability>> {
        let mut sorted_capabilities = capabilities.to_vec();
        sorted_capabilities.sort_by(|a, b| a.current_load.partial_cmp(&b.current_load).unwrap());
        
        let selected_count = (sorted_capabilities.len() / 2).max(1).min(4);
        Ok(sorted_capabilities.into_iter().take(selected_count).collect())
    }

    /// Performance-based selection
    fn performance_based_selection(
        &self,
        capabilities: &[DeviceCapability],
        complexity: &TaskComplexity,
    ) -> Result<Vec<DeviceCapability>> {
        let mut scored_capabilities: Vec<_> = capabilities.iter()
            .map(|cap| {
                let score = if complexity.requires_gpu {
                    cap.gpu_capability.as_ref().map(|gpu| gpu.benchmark_score).unwrap_or(0.0)
                } else {
                    cap.benchmark_score
                };
                (cap, score)
            })
            .collect();

        scored_capabilities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        let selected_count = (scored_capabilities.len() / 2).max(1).min(4);
        Ok(scored_capabilities.into_iter()
            .take(selected_count)
            .map(|(cap, _)| cap.clone())
            .collect())
    }

    /// Latency-optimized selection
    fn latency_optimized_selection(&self, capabilities: &[DeviceCapability]) -> Result<Vec<DeviceCapability>> {
        let mut sorted_capabilities = capabilities.to_vec();
        sorted_capabilities.sort_by(|a, b| a.network_latency_ms.cmp(&b.network_latency_ms));
        
        let selected_count = (sorted_capabilities.len() / 2).max(1).min(4);
        Ok(sorted_capabilities.into_iter().take(selected_count).collect())
    }

    /// Hybrid selection combining multiple strategies
    async fn hybrid_selection(
        &self,
        capabilities: &[DeviceCapability],
        complexity: &TaskComplexity,
        _performance_metrics: &PerformanceMetrics,
    ) -> Result<Vec<DeviceCapability>> {
        // Combine performance and load balancing
        let mut scored_capabilities: Vec<_> = capabilities.iter()
            .map(|cap| {
                let performance_score = if complexity.requires_gpu {
                    cap.gpu_capability.as_ref().map(|gpu| gpu.benchmark_score).unwrap_or(0.0)
                } else {
                    cap.benchmark_score
                };
                
                let load_score = 1.0 - cap.current_load as f64;
                let latency_score = 1.0 / (cap.network_latency_ms as f64 + 1.0);
                
                let total_score = 
                    performance_score * self.performance_weights.cpu_weight +
                    load_score * self.performance_weights.load_weight +
                    latency_score * self.performance_weights.network_weight;
                
                (cap, total_score)
            })
            .collect();

        scored_capabilities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        let selected_count = (scored_capabilities.len() / 2).max(1).min(4);
        Ok(scored_capabilities.into_iter()
            .take(selected_count)
            .map(|(cap, _)| cap.clone())
            .collect())
    }
}

impl PerformanceMetrics {
    /// Create new performance metrics
    pub fn new() -> Self {
        Self {
            total_tasks_distributed: 0,
            average_completion_time_ms: 0,
            load_balance_efficiency: 1.0,
            node_utilization: HashMap::new(),
            task_success_rate: 1.0,
        }
    }
}

/// Device capability update structure
#[derive(Debug, Clone)]
pub struct DeviceCapabilityUpdate {
    pub current_load: Option<f32>,
    pub is_online: Option<bool>,
    pub benchmark_score: Option<f64>,
}

/// Distribution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionStats {
    pub total_peers: usize,
    pub online_peers: usize,
    pub total_tasks_distributed: u64,
    pub average_completion_time_ms: u64,
    pub load_balance_efficiency: f64,
    pub task_success_rate: f64,
    pub recent_distributions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::{ComputationTask, TaskType, TaskPriority};
    use uuid::Uuid;

    #[tokio::test]
    async fn test_task_distributor_creation() {
        let distributor = TaskDistributor::new();
        let stats = distributor.get_distribution_stats().await;
        
        assert_eq!(stats.total_peers, 0);
        assert_eq!(stats.total_tasks_distributed, 0);
    }

    #[tokio::test]
    async fn test_peer_registration() {
        let distributor = TaskDistributor::new();
        
        let capability = DeviceCapability {
            node_id: "test_node".to_string(),
            cpu_cores: 4,
            gpu_compute_units: Some(2),
            memory_gb: 8.0,
            benchmark_score: 1000.0,
            current_load: 0.3,
            network_latency_ms: 50,
            is_online: true,
            last_heartbeat: chrono::Utc::now(),
            supported_task_types: vec![TaskType::BlockValidation],
            gpu_capability: None,
        };
        
        distributor.register_peer_capability("test_node".to_string(), capability).await.unwrap();
        
        let stats = distributor.get_distribution_stats().await;
        assert_eq!(stats.total_peers, 1);
        assert_eq!(stats.online_peers, 1);
    }

    #[tokio::test]
    async fn test_complexity_analysis() {
        let analyzer = ComplexityAnalyzer::new();
        
        let task = ComputationTask {
            id: Uuid::new_v4(),
            task_type: TaskType::BlockValidation,
            data: vec![0u8; 2048], // 2KB data
            priority: TaskPriority::Normal,
            created_at: std::time::SystemTime::now(),
        };
        
        let complexity = analyzer.analyze_complexity(&task);
        
        assert!(complexity.score > 0.0);
        assert_eq!(complexity.requires_gpu, true); // Block validation should require GPU
        assert!(complexity.estimated_duration_ms > 0);
    }
}
