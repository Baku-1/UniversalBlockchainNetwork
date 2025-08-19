// src/shared_types.rs
// Centralized type definitions for the Universal Blockchain Network Engine

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Commands that can be sent to the engine manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineCommand {
    /// Pause all network and validation activity
    Pause,
    /// Resume all network and validation activity
    Resume,
    /// Shutdown the engine gracefully
    Shutdown,
    /// Get current engine status
    GetStatus,
}

/// Engine operating modes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EngineMode {
    /// WAN mode with internet connectivity
    WAN,
    /// Local mesh mode without internet
    Mesh,
    /// Hybrid mode with both WAN and mesh
    Hybrid,
    /// Offline mode for development/testing
    Offline,
}

/// Network connectivity status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub is_connected: bool,
    pub mode: EngineMode,
    pub peer_count: usize,
    pub last_seen: SystemTime,
}

/// Engine health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetrics {
    pub uptime_seconds: u64,
    pub tasks_processed: u64,
    pub validation_success_rate: f64,
    pub memory_usage_mb: u64,
    pub cpu_usage_percent: f64,
}

/// Engine configuration overrides
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub max_concurrent_tasks: usize,
    pub validation_timeout_seconds: u64,
    pub peer_discovery_interval_seconds: u64,
    pub enable_debug_logging: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 100,
            validation_timeout_seconds: 30,
            peer_discovery_interval_seconds: 60,
            enable_debug_logging: false,
        }
    }
}

/// Task priority levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl Default for TaskPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Task execution status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// Task metadata for tracking and management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetadata {
    pub created_at: SystemTime,
    pub started_at: Option<SystemTime>,
    pub completed_at: Option<SystemTime>,
    pub priority: TaskPriority,
    pub retry_count: u32,
    pub max_retries: u32,
    pub tags: Vec<String>,
}

impl Default for TaskMetadata {
    fn default() -> Self {
        Self {
            created_at: SystemTime::now(),
            started_at: None,
            completed_at: None,
            priority: TaskPriority::default(),
            retry_count: 0,
            max_retries: 3,
            tags: Vec::new(),
        }
    }
}
