// src/config.rs

use serde::{Deserialize, Serialize};
use config::{Config, File};
use std::path::PathBuf;
use anyhow::{Result, Context};
use thiserror::Error;

/// Configuration validation errors
#[derive(Error, Debug)]
pub enum ConfigValidationError {
    #[error("Invalid port number: {0}")]
    InvalidPort(u16),
    #[error("Invalid URL format: {0}")]
    InvalidUrl(String),
    #[error("Invalid chain ID: {0}")]
    InvalidChainId(u64),
    #[error("Invalid UUID format: {0}")]
    InvalidUuid(String),
    #[error("Configuration validation failed: {0}")]
    ValidationFailed(String),
}

/// Defines the structure for all engine settings.
/// This struct must match the structure of the Settings.toml file.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub ipc_port: u16,
    pub p2p_port: u16,
    pub keys_path: PathBuf,
    pub bootstrap_nodes: Vec<String>,

    // AuraProtocol contract address (optional)
    pub aura_protocol_address: Option<String>,

    // Bluetooth mesh networking configuration
    pub mesh: MeshConfig,

    // Ronin blockchain configuration
    pub ronin: RoninConfig,

    // Game-specific configuration
    pub game: GameConfig,
}

impl AppConfig {
    /// Validate the configuration settings
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        // Validate port numbers
        if self.ipc_port == 0 {
            return Err(ConfigValidationError::InvalidPort(self.ipc_port));
        }
        if self.p2p_port == 0 {
            return Err(ConfigValidationError::InvalidPort(self.p2p_port));
        }

        // Validate contract address format if provided
        if let Some(ref address) = self.aura_protocol_address {
            if !address.starts_with("0x") || address.len() != 42 {
                return Err(ConfigValidationError::ValidationFailed(
                    format!("Invalid contract address format: {}", address)
                ));
            }
        }

        // Validate nested configurations
        self.mesh.validate()?;
        self.ronin.validate()?;
        self.game.validate()?;

        Ok(())
    }

    /// Save configuration to file
    pub fn save_to_file(&self, path: &str) -> Result<()> {
        let toml_string = toml::to_string_pretty(self)
            .context("Failed to serialize configuration to TOML")?;

        std::fs::write(path, toml_string)
            .with_context(|| format!("Failed to write configuration to {}", path))?;

        Ok(())
    }
}

/// Bluetooth mesh networking configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MeshConfig {
    /// Service UUID for Aura mesh network discovery
    pub service_uuid: String,
    /// Maximum number of concurrent peer connections
    pub max_peers: usize,
    /// Connection timeout in seconds
    pub connection_timeout_secs: u64,
    /// Mesh message TTL (time-to-live) for hop limiting
    pub message_ttl: u8,
    /// Scan interval in milliseconds
    pub scan_interval_ms: u64,
    /// Advertisement interval in milliseconds
    pub advertisement_interval_ms: u64,
}

impl MeshConfig {
    /// Validate mesh configuration
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        // Validate UUID format (basic check)
        if self.service_uuid.len() != 36 || !self.service_uuid.contains('-') {
            return Err(ConfigValidationError::InvalidUuid(self.service_uuid.clone()));
        }

        // Validate reasonable limits
        if self.max_peers == 0 || self.max_peers > 100 {
            return Err(ConfigValidationError::ValidationFailed(
                format!("max_peers must be between 1 and 100, got {}", self.max_peers)
            ));
        }

        if self.message_ttl == 0 || self.message_ttl > 50 {
            return Err(ConfigValidationError::ValidationFailed(
                format!("message_ttl must be between 1 and 50, got {}", self.message_ttl)
            ));
        }

        if self.scan_interval_ms < 100 || self.scan_interval_ms > 60000 {
            return Err(ConfigValidationError::ValidationFailed(
                format!("scan_interval_ms must be between 100 and 60000, got {}", self.scan_interval_ms)
            ));
        }

        Ok(())
    }
}

/// Ronin blockchain configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RoninConfig {
    /// Ronin RPC endpoint URL
    pub rpc_url: String,
    /// Chain ID for Ronin network (2020 for mainnet)
    pub chain_id: u64,
    /// Gas price in wei
    pub gas_price: u64,
    /// Gas limit for transactions
    pub gas_limit: u64,
    /// Maximum offline transaction queue size
    pub max_offline_transactions: usize,
    /// Transaction sync retry interval in seconds
    pub sync_retry_interval_secs: u64,
}

impl RoninConfig {
    /// Validate Ronin configuration
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        // Validate URL format
        if !self.rpc_url.starts_with("http://") && !self.rpc_url.starts_with("https://") {
            return Err(ConfigValidationError::InvalidUrl(self.rpc_url.clone()));
        }

        // Validate chain ID (Ronin specific)
        if self.chain_id != 2020 && self.chain_id != 2021 {
            return Err(ConfigValidationError::InvalidChainId(self.chain_id));
        }

        // Validate gas parameters
        if self.gas_price == 0 {
            return Err(ConfigValidationError::ValidationFailed(
                "gas_price must be greater than 0".to_string()
            ));
        }

        if self.gas_limit < 21000 {
            return Err(ConfigValidationError::ValidationFailed(
                "gas_limit must be at least 21000".to_string()
            ));
        }

        // Validate queue size
        if self.max_offline_transactions == 0 || self.max_offline_transactions > 10000 {
            return Err(ConfigValidationError::ValidationFailed(
                format!("max_offline_transactions must be between 1 and 10000, got {}", self.max_offline_transactions)
            ));
        }

        Ok(())
    }
}

/// Game-specific configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GameConfig {
    /// Maximum number of players in a mesh game session
    pub max_players: usize,
    /// Game state sync interval in milliseconds
    pub sync_interval_ms: u64,
    /// Conflict resolution timeout in seconds
    pub conflict_resolution_timeout_secs: u64,
    /// Maximum game actions per player per second (rate limiting)
    pub max_actions_per_second: u32,
}

impl GameConfig {
    /// Validate game configuration
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        // Validate player limits
        if self.max_players == 0 || self.max_players > 100 {
            return Err(ConfigValidationError::ValidationFailed(
                format!("max_players must be between 1 and 100, got {}", self.max_players)
            ));
        }

        // Validate sync interval
        if self.sync_interval_ms < 10 || self.sync_interval_ms > 10000 {
            return Err(ConfigValidationError::ValidationFailed(
                format!("sync_interval_ms must be between 10 and 10000, got {}", self.sync_interval_ms)
            ));
        }

        // Validate timeout
        if self.conflict_resolution_timeout_secs == 0 || self.conflict_resolution_timeout_secs > 300 {
            return Err(ConfigValidationError::ValidationFailed(
                format!("conflict_resolution_timeout_secs must be between 1 and 300, got {}", self.conflict_resolution_timeout_secs)
            ));
        }

        // Validate rate limiting
        if self.max_actions_per_second == 0 || self.max_actions_per_second > 1000 {
            return Err(ConfigValidationError::ValidationFailed(
                format!("max_actions_per_second must be between 1 and 1000, got {}", self.max_actions_per_second)
            ));
        }

        Ok(())
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ipc_port: 9898,
            p2p_port: 4001,
            keys_path: PathBuf::from("./aura_node_identity.key"),
            bootstrap_nodes: vec![],
            aura_protocol_address: None,
            mesh: MeshConfig::default(),
            ronin: RoninConfig::default(),
            game: GameConfig::default(),
        }
    }
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            service_uuid: "6E400001-B5A3-F393-E0A9-E50E24DCCA9E".to_string(),
            max_peers: 8,
            connection_timeout_secs: 30,
            message_ttl: 5,
            scan_interval_ms: 1000,
            advertisement_interval_ms: 2000,
        }
    }
}

impl Default for RoninConfig {
    fn default() -> Self {
        Self {
            rpc_url: "https://api.roninchain.com/rpc".to_string(),
            chain_id: 2020, // Ronin mainnet
            gas_price: 20_000_000_000, // 20 gwei
            gas_limit: 21_000,
            max_offline_transactions: 1000,
            sync_retry_interval_secs: 60,
        }
    }
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            max_players: 4,
            sync_interval_ms: 100,
            conflict_resolution_timeout_secs: 10,
            max_actions_per_second: 10,
        }
    }
}

/// Loads the application configuration from the "Settings.toml" file.
/// Enhanced with validation and better error handling.
pub fn load_config() -> Result<AppConfig> {
    load_config_from_file("Settings.toml")
}

/// Load configuration from a specific file
pub fn load_config_from_file(filename: &str) -> Result<AppConfig> {
    tracing::info!("Loading configuration from {}...", filename);

    // Create a new configuration builder
    let builder = Config::builder()
        .add_source(File::with_name(filename).required(false))
        .add_source(config::Environment::with_prefix("AURA"));

    // Build the configuration
    let settings = builder.build()
        .context("Failed to build configuration")?;

    // Try to deserialize into our AppConfig struct
    let config = match settings.try_deserialize::<AppConfig>() {
        Ok(config) => {
            tracing::info!("Configuration loaded successfully from {}", filename);
            config
        }
        Err(e) => {
            tracing::warn!("Failed to load configuration from {}, using defaults: {}", filename, e);
            AppConfig::default()
        }
    };

    // Validate the configuration
    config.validate()
        .context("Configuration validation failed")?;

    tracing::info!("Configuration validated successfully");
    Ok(config)
}

/// Create a default configuration file
pub fn create_default_config_file(filename: &str) -> Result<()> {
    let default_config = AppConfig::default();
    default_config.save_to_file(filename)
        .with_context(|| format!("Failed to create default configuration file: {}", filename))?;

    tracing::info!("Created default configuration file: {}", filename);
    Ok(())
}