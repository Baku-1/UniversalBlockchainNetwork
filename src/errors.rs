// src/errors.rs

use thiserror::Error;
use uuid::Uuid;

/// Comprehensive error types for the Nexus Engine
#[derive(Error, Debug)]
pub enum NexusError {
    // Configuration errors
    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),
    
    #[error("Invalid configuration value: {field} = {value}")]
    InvalidConfig { field: String, value: String },

    // Cryptography errors
    #[error("Cryptographic operation failed: {0}")]
    Crypto(String),
    
    #[error("Invalid signature from node {node_id}")]
    InvalidSignature { node_id: String },
    
    #[error("Key generation failed: {0}")]
    KeyGeneration(String),

    // Networking errors
    #[error("Network connection failed: {0}")]
    NetworkConnection(String),
    
    #[error("Bluetooth adapter not found")]
    BluetoothAdapterNotFound,
    
    #[error("Bluetooth operation failed: {0}")]
    BluetoothError(String),
    
    #[error("Peer {peer_id} not found")]
    PeerNotFound { peer_id: String },
    
    #[error("Message routing failed for destination {destination}: {reason}")]
    RoutingFailed { destination: String, reason: String },

    // Mesh networking errors
    #[error("Mesh network topology error: {0}")]
    MeshTopology(String),
    
    #[error("Route discovery timeout for destination {destination}")]
    RouteDiscoveryTimeout { destination: String },
    
    #[error("Maximum hop count exceeded for message {message_id}")]
    MaxHopsExceeded { message_id: Uuid },

    // Game state errors
    #[error("Game state error: {0}")]
    GameState(String),
    
    #[error("Invalid player action: {action} by player {player_id}")]
    InvalidPlayerAction { action: String, player_id: String },
    
    #[error("Game session full: maximum {max_players} players")]
    GameSessionFull { max_players: usize },
    
    #[error("Player {player_id} not found in session {session_id}")]
    PlayerNotFound { player_id: String, session_id: Uuid },
    
    #[error("Game conflict detected: {conflict_type}")]
    GameConflict { conflict_type: String },

    // Blockchain/Web3 errors
    #[error("Ronin blockchain error: {0}")]
    RoninBlockchain(String),
    
    #[error("Transaction validation failed: {tx_id}")]
    TransactionValidation { tx_id: Uuid },
    
    #[error("Insufficient balance for transaction {tx_id}: required {required}, available {available}")]
    InsufficientBalance { tx_id: Uuid, required: u64, available: u64 },
    
    #[error("Gas estimation failed for transaction {tx_id}: {reason}")]
    GasEstimationFailed { tx_id: Uuid, reason: String },
    
    #[error("Transaction {tx_id} failed on chain: {reason}")]
    TransactionFailed { tx_id: Uuid, reason: String },

    // Synchronization errors
    #[error("Synchronization failed: {0}")]
    Synchronization(String),
    
    #[error("Sync timeout for peer {peer_id}")]
    SyncTimeout { peer_id: String },
    
    #[error("State merge conflict: {0}")]
    StateMergeConflict(String),

    // Validation errors
    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Block validation failed: {block_id}")]
    BlockValidation { block_id: String },

    #[error("Computation task failed: {task_id}")]
    ComputationTaskFailed { task_id: Uuid },

    #[error("Validator overloaded: {pending_tasks} tasks pending")]
    ValidatorOverloaded { pending_tasks: usize },

    // Storage errors
    #[error("Database error: {0}")]
    Database(#[from] sled::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),
    
    #[error("File I/O error: {0}")]
    Io(#[from] std::io::Error),

    // Rate limiting errors
    #[error("Rate limit exceeded for player {player_id}: {current_rate} actions/sec")]
    RateLimitExceeded { player_id: String, current_rate: f32 },
    
    #[error("Cooldown active for action {action}: {remaining_ms}ms remaining")]
    CooldownActive { action: String, remaining_ms: u64 },

    // Protocol errors
    #[error("Protocol version mismatch: local {local_version}, peer {peer_version}")]
    ProtocolVersionMismatch { local_version: String, peer_version: String },
    
    #[error("Invalid message format from peer {peer_id}")]
    InvalidMessageFormat { peer_id: String },
    
    #[error("Message too large: {size} bytes, maximum {max_size} bytes")]
    MessageTooLarge { size: usize, max_size: usize },

    // Resource errors
    #[error("Resource not available: {resource_id}")]
    ResourceNotAvailable { resource_id: String },
    
    #[error("Resource contention for {resource_id}: {contenders} contenders")]
    ResourceContention { resource_id: String, contenders: usize },

    // Generic errors
    #[error("Internal error: {0}")]
    Internal(String),
    
    #[error("Operation timeout: {operation}")]
    Timeout { operation: String },
    
    #[error("Service unavailable: {service}")]
    ServiceUnavailable { service: String },
}

/// Result type alias for Nexus operations
pub type NexusResult<T> = std::result::Result<T, NexusError>;

/// Error context for better error reporting
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub operation: String,
    pub component: String,
    pub node_id: Option<String>,
    pub session_id: Option<Uuid>,
    pub timestamp: std::time::SystemTime,
}

impl ErrorContext {
    pub fn new(operation: &str, component: &str) -> Self {
        Self {
            operation: operation.to_string(),
            component: component.to_string(),
            node_id: None,
            session_id: None,
            timestamp: std::time::SystemTime::now(),
        }
    }

    pub fn with_node_id(mut self, node_id: String) -> Self {
        self.node_id = Some(node_id);
        self
    }

    pub fn with_session_id(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }
}

/// Enhanced error with context
#[derive(Debug)]
pub struct ContextualError {
    pub error: NexusError,
    pub context: ErrorContext,
}

impl std::fmt::Display for ContextualError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}:{}] {}: {}",
            self.context.component,
            self.context.operation,
            self.error,
            if let Some(node_id) = &self.context.node_id {
                format!(" (node: {})", node_id)
            } else {
                String::new()
            }
        )
    }
}

impl std::error::Error for ContextualError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Trait for adding context to errors
pub trait ErrorContextExt<T> {
    fn with_context(self, context: ErrorContext) -> std::result::Result<T, ContextualError>;
}

impl<T> ErrorContextExt<T> for NexusResult<T> {
    fn with_context(self, context: ErrorContext) -> std::result::Result<T, ContextualError> {
        self.map_err(|error| ContextualError { error, context })
    }
}

/// Utility functions for error handling
pub mod utils {
    use super::*;

    /// Check if an error is recoverable
    pub fn is_recoverable_error(error: &NexusError) -> bool {
        match error {
            NexusError::NetworkConnection(_) => true,
            NexusError::SyncTimeout { .. } => true,
            NexusError::RouteDiscoveryTimeout { .. } => true,
            NexusError::TransactionValidation { .. } => false,
            NexusError::InvalidSignature { .. } => false,
            NexusError::GameSessionFull { .. } => false,
            NexusError::RateLimitExceeded { .. } => true,
            NexusError::CooldownActive { .. } => true,
            NexusError::ResourceContention { .. } => true,
            NexusError::Timeout { .. } => true,
            NexusError::ServiceUnavailable { .. } => true,
            _ => false,
        }
    }

    /// Get retry delay for recoverable errors
    pub fn get_retry_delay(error: &NexusError, attempt: u32) -> Option<std::time::Duration> {
        if !is_recoverable_error(error) {
            return None;
        }

        let base_delay = match error {
            NexusError::NetworkConnection(_) => std::time::Duration::from_secs(5),
            NexusError::SyncTimeout { .. } => std::time::Duration::from_secs(10),
            NexusError::RouteDiscoveryTimeout { .. } => std::time::Duration::from_secs(15),
            NexusError::RateLimitExceeded { .. } => std::time::Duration::from_secs(1),
            NexusError::CooldownActive { .. } => std::time::Duration::from_millis(100),
            NexusError::ResourceContention { .. } => std::time::Duration::from_millis(500),
            NexusError::Timeout { .. } => std::time::Duration::from_secs(2),
            NexusError::ServiceUnavailable { .. } => std::time::Duration::from_secs(30),
            _ => std::time::Duration::from_secs(1),
        };

        // Exponential backoff with jitter
        let multiplier = 2_u64.pow(attempt.min(5)); // Cap at 2^5 = 32
        let jitter = rand::random::<f64>() * 0.1; // 10% jitter
        let delay = base_delay * u32::try_from(multiplier).unwrap_or(32);
        let jittered_delay = delay + std::time::Duration::from_millis((delay.as_millis() as f64 * jitter) as u64);

        Some(jittered_delay.min(std::time::Duration::from_secs(300))) // Max 5 minutes
    }

    /// Log error with appropriate level
    pub fn log_error(error: &NexusError, context: Option<&ErrorContext>) {
        let context_str = if let Some(ctx) = context {
            format!(" [{}:{}]", ctx.component, ctx.operation)
        } else {
            String::new()
        };

        match error {
            NexusError::InvalidSignature { .. } |
            NexusError::TransactionFailed { .. } |
            NexusError::BlockValidation { .. } => {
                tracing::error!("{}{}", error, context_str);
            }
            NexusError::NetworkConnection(_) |
            NexusError::SyncTimeout { .. } |
            NexusError::PeerNotFound { .. } => {
                tracing::warn!("{}{}", error, context_str);
            }
            NexusError::RateLimitExceeded { .. } |
            NexusError::CooldownActive { .. } => {
                tracing::debug!("{}{}", error, context_str);
            }
            _ => {
                tracing::info!("{}{}", error, context_str);
            }
        }
    }
}

/// Result type alias for convenience
pub type Result<T> = std::result::Result<T, NexusError>;
