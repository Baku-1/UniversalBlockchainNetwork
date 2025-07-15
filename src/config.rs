// src/config.rs

use serde::Deserialize;
use config::{Config, ConfigError, File};
use std::path::PathBuf;

/// Defines the structure for all engine settings.
/// This struct must match the structure of the Settings.toml file.
#[derive(Debug, Deserialize, Clone)]
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

/// Bluetooth mesh networking configuration
#[derive(Debug, Deserialize, Clone)]
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

/// Ronin blockchain configuration
#[derive(Debug, Deserialize, Clone)]
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

/// Game-specific configuration
#[derive(Debug, Deserialize, Clone)]
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
pub fn load_config() -> Result<AppConfig, ConfigError> {
    tracing::info!("Loading configuration from Settings.toml...");

    // Create a new configuration builder
    let builder = Config::builder()
        .add_source(File::with_name("Settings.toml").required(false))
        .add_source(config::Environment::with_prefix("AURA"));

    // Build the configuration
    let settings = builder.build()?;

    // Try to deserialize into our AppConfig struct, falling back to defaults
    match settings.try_deserialize::<AppConfig>() {
        Ok(config) => {
            tracing::info!("Configuration loaded successfully");
            Ok(config)
        }
        Err(e) => {
            tracing::warn!("Failed to load configuration, using defaults: {}", e);
            Ok(AppConfig::default())
        }
    }
}