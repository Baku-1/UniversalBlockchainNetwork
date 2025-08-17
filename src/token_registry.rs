// src/token_registry.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;
use tracing;
use chrono;

/// Supported blockchain networks for cross-chain operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum BlockchainNetwork {
    Ronin,
    Ethereum,
    Polygon,
    BSC,
    Arbitrum,
    Optimism,
    Avalanche,
    Fantom,
    Solana,
}

impl BlockchainNetwork {
    /// Get chain ID for the network
    pub fn chain_id(&self) -> u64 {
        match self {
            BlockchainNetwork::Ronin => 2020,
            BlockchainNetwork::Ethereum => 1,
            BlockchainNetwork::Polygon => 137,
            BlockchainNetwork::BSC => 56,
            BlockchainNetwork::Arbitrum => 42161,
            BlockchainNetwork::Optimism => 10,
            BlockchainNetwork::Avalanche => 43114,
            BlockchainNetwork::Fantom => 250,
            BlockchainNetwork::Solana => 101, // Solana uses different numbering
        }
    }

    /// Get RPC URL for the network
    pub fn rpc_url(&self) -> &'static str {
        match self {
            BlockchainNetwork::Ronin => "https://api.roninchain.com/rpc",
            BlockchainNetwork::Ethereum => "https://mainnet.infura.io/v3/YOUR_KEY",
            BlockchainNetwork::Polygon => "https://polygon-rpc.com",
            BlockchainNetwork::BSC => "https://bsc-dataseed.binance.org",
            BlockchainNetwork::Arbitrum => "https://arb1.arbitrum.io/rpc",
            BlockchainNetwork::Optimism => "https://mainnet.optimism.io",
            BlockchainNetwork::Avalanche => "https://api.avax.network/ext/bc/C/rpc",
            BlockchainNetwork::Fantom => "https://rpc.ftm.tools",
            BlockchainNetwork::Solana => "https://api.mainnet-beta.solana.com",
        }
    }

    /// Get network name for display
    pub fn display_name(&self) -> &'static str {
        match self {
            BlockchainNetwork::Ronin => "Ronin",
            BlockchainNetwork::Ethereum => "Ethereum",
            BlockchainNetwork::Polygon => "Polygon",
            BlockchainNetwork::BSC => "BSC",
            BlockchainNetwork::Arbitrum => "Arbitrum",
            BlockchainNetwork::Optimism => "Optimism",
            BlockchainNetwork::Avalanche => "Avalanche",
            BlockchainNetwork::Fantom => "Fantom",
            BlockchainNetwork::Solana => "Solana",
        }
    }

    /// Check if network supports EVM
    pub fn is_evm_compatible(&self) -> bool {
        match self {
            BlockchainNetwork::Solana => false,
            _ => true,
        }
    }
}

/// Token mapping between different blockchain networks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMapping {
    pub source_network: BlockchainNetwork,
    pub source_address: String,
    pub source_symbol: String,
    pub source_decimals: u8,
    pub target_network: BlockchainNetwork,
    pub target_address: String,
    pub target_symbol: String,
    pub target_decimals: u8,
    pub exchange_rate: f64,
    pub is_active: bool,
    pub last_updated: chrono::DateTime<chrono::Utc>,
    pub liquidity_score: f64,
    pub bridge_fee: f64,
}

/// Bridge contract information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeContract {
    pub network: BlockchainNetwork,
    pub contract_address: String,
    pub contract_type: BridgeContractType,
    pub is_active: bool,
    pub last_verified: chrono::DateTime<chrono::Utc>,
    pub security_score: f64,
    pub supported_tokens: Vec<String>,
}

/// Bridge contract types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeContractType {
    LockAndMint,
    BurnAndMint,
    LiquidityPool,
    AtomicSwap,
    MultiHop,
}

/// Cross-chain transfer request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossChainTransfer {
    pub transfer_id: String,
    pub source_network: BlockchainNetwork,
    pub target_network: BlockchainNetwork,
    pub token_symbol: String,
    pub amount: u64,
    pub source_address: String,
    pub target_address: String,
    pub bridge_fee: u64,
    pub estimated_time: u64, // in minutes
    pub status: TransferStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Transfer status enumeration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransferStatus {
    Pending,
    Processing,
    Confirmed,
    Failed(String),
    Completed,
}

/// Cross-chain token registry for managing token mappings
pub struct CrossChainTokenRegistry {
    pub token_mappings: Arc<RwLock<HashMap<(BlockchainNetwork, String), Vec<TokenMapping>>>>,
    pub bridge_contracts: Arc<RwLock<HashMap<BlockchainNetwork, BridgeContract>>>,
    pub transfer_history: Arc<RwLock<HashMap<String, CrossChainTransfer>>>,
    pub network_status: Arc<RwLock<HashMap<BlockchainNetwork, NetworkStatus>>>,
}

/// Network status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub is_online: bool,
    pub last_checked: chrono::DateTime<chrono::Utc>,
    pub block_height: u64,
    pub gas_price: Option<u64>,
    pub congestion_level: f64, // 0.0 to 1.0
    pub error_count: u32,
}

impl CrossChainTokenRegistry {
    /// Create a new cross-chain token registry
    pub fn new() -> Self {
        let mut registry = Self {
            token_mappings: Arc::new(RwLock::new(HashMap::new())),
            bridge_contracts: Arc::new(RwLock::new(HashMap::new())),
            transfer_history: Arc::new(RwLock::new(HashMap::new())),
            network_status: Arc::new(RwLock::new(HashMap::new())),
        };

        // Initialize with default token mappings
        registry.initialize_default_mappings();
        registry.initialize_default_bridges();
        registry.initialize_network_status();

        registry
    }

    /// Initialize default token mappings for common tokens
    fn initialize_default_mappings(&mut self) {
        let mut mappings = self.token_mappings.try_write().unwrap();
        
        // USDC mappings
        let usdc_mappings = vec![
            TokenMapping {
                source_network: BlockchainNetwork::Ethereum,
                source_address: "0xA0b86a33E6441b8c4C8C0b4b4C4C4C4C4C4C4C4C".to_string(),
                source_symbol: "USDC".to_string(),
                source_decimals: 6,
                target_network: BlockchainNetwork::Polygon,
                target_address: "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174".to_string(),
                target_symbol: "USDC".to_string(),
                target_decimals: 6,
                exchange_rate: 1.0,
                is_active: true,
                last_updated: chrono::Utc::now(),
                liquidity_score: 0.95,
                bridge_fee: 0.001,
            },
            TokenMapping {
                source_network: BlockchainNetwork::Ethereum,
                source_address: "0xA0b86a33E6441b8c4C8C0b4b4C4C4C4C4C4C4C4C".to_string(),
                source_symbol: "USDC".to_string(),
                source_decimals: 6,
                target_network: BlockchainNetwork::BSC,
                target_address: "0x8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d".to_string(),
                target_symbol: "USDC".to_string(),
                target_decimals: 18,
                exchange_rate: 1.0,
                is_active: true,
                last_updated: chrono::Utc::now(),
                liquidity_score: 0.90,
                bridge_fee: 0.002,
            },
        ];

        mappings.insert(
            (BlockchainNetwork::Ethereum, "USDC".to_string()),
            usdc_mappings
        );

        // RON token mappings
        let ron_mappings = vec![
            TokenMapping {
                source_network: BlockchainNetwork::Ronin,
                source_address: "0x0000000000000000000000000000000000000000".to_string(),
                source_symbol: "RON".to_string(),
                source_decimals: 18,
                target_network: BlockchainNetwork::Ethereum,
                target_address: "0x0000000000000000000000000000000000000000".to_string(),
                target_symbol: "RON".to_string(),
                target_decimals: 18,
                exchange_rate: 1.0,
                is_active: true,
                last_updated: chrono::Utc::now(),
                liquidity_score: 0.85,
                bridge_fee: 0.005,
            },
        ];

        mappings.insert(
            (BlockchainNetwork::Ronin, "RON".to_string()),
            ron_mappings
        );
    }

    /// Initialize default bridge contracts
    fn initialize_default_bridges(&mut self) {
        let mut bridges = self.bridge_contracts.try_write().unwrap();
        
        // Ronin bridge
        bridges.insert(BlockchainNetwork::Ronin, BridgeContract {
            network: BlockchainNetwork::Ronin,
            contract_address: "0x1234567890123456789012345678901234567890".to_string(),
            contract_type: BridgeContractType::LockAndMint,
            is_active: true,
            last_verified: chrono::Utc::now(),
            security_score: 0.95,
            supported_tokens: vec!["RON".to_string(), "USDC".to_string(), "AXS".to_string()],
        });

        // Ethereum bridge
        bridges.insert(BlockchainNetwork::Ethereum, BridgeContract {
            network: BlockchainNetwork::Ethereum,
            contract_address: "0x0987654321098765432109876543210987654321".to_string(),
            contract_type: BridgeContractType::BurnAndMint,
            is_active: true,
            last_verified: chrono::Utc::now(),
            security_score: 0.90,
            supported_tokens: vec!["USDC".to_string(), "ETH".to_string(), "USDT".to_string()],
        });
    }

    /// Initialize network status
    fn initialize_network_status(&mut self) {
        let mut status = self.network_status.try_write().unwrap();
        
        for network in [
            BlockchainNetwork::Ronin,
            BlockchainNetwork::Ethereum,
            BlockchainNetwork::Polygon,
            BlockchainNetwork::BSC,
            BlockchainNetwork::Arbitrum,
            BlockchainNetwork::Optimism,
        ] {
            status.insert(network, NetworkStatus {
                is_online: true,
                last_checked: chrono::Utc::now(),
                block_height: 0,
                gas_price: None,
                congestion_level: 0.0,
                error_count: 0,
            });
        }
    }

    /// Get token mapping between networks
    pub async fn get_token_mapping(
        &self,
        source_network: &BlockchainNetwork,
        target_network: &BlockchainNetwork,
        token_symbol: &str,
    ) -> Option<TokenMapping> {
        let mappings = self.token_mappings.read().await;
        
        if let Some(network_mappings) = mappings.get(&(source_network.clone(), token_symbol.to_string())) {
            for mapping in network_mappings {
                if mapping.target_network == *target_network && mapping.is_active {
                    return Some(mapping.clone());
                }
            }
        }
        
        None
    }

    /// Get all available mappings for a token
    pub async fn get_all_token_mappings(
        &self,
        source_network: &BlockchainNetwork,
        token_symbol: &str,
    ) -> Vec<TokenMapping> {
        let mappings = self.token_mappings.read().await;
        
        mappings
            .get(&(source_network.clone(), token_symbol.to_string()))
            .cloned()
            .unwrap_or_default()
    }

    /// Add new token mapping
    pub async fn add_token_mapping(&self, mapping: TokenMapping) -> Result<()> {
        let mut mappings = self.token_mappings.write().await;
        
        let key = (mapping.source_network.clone(), mapping.source_symbol.clone());
        let network_mappings = mappings.entry(key).or_insert_with(Vec::new);
        
        // Check for duplicates
        if !network_mappings.iter().any(|m| 
            m.target_network == mapping.target_network && 
            m.target_address == mapping.target_address
        ) {
            network_mappings.push(mapping.clone());
            tracing::info!("Added new token mapping: {} -> {}", 
                mapping.source_symbol, mapping.target_symbol);
        }
        
        Ok(())
    }

    /// Update token mapping
    pub async fn update_token_mapping(
        &self,
        source_network: &BlockchainNetwork,
        source_symbol: &str,
        target_network: &BlockchainNetwork,
        updates: TokenMappingUpdate,
    ) -> Result<bool> {
        let mut mappings = self.token_mappings.write().await;
        
        if let Some(network_mappings) = mappings.get_mut(&(source_network.clone(), source_symbol.to_string())) {
            for mapping in network_mappings {
                if mapping.target_network == *target_network {
                    // Apply updates
                    if let Some(exchange_rate) = updates.exchange_rate {
                        mapping.exchange_rate = exchange_rate;
                    }
                    if let Some(is_active) = updates.is_active {
                        mapping.is_active = is_active;
                    }
                    if let Some(bridge_fee) = updates.bridge_fee {
                        mapping.bridge_fee = bridge_fee;
                    }
                    
                    mapping.last_updated = chrono::Utc::now();
                    tracing::info!("Updated token mapping: {} -> {}", source_symbol, mapping.target_symbol);
                    return Ok(true);
                }
            }
        }
        
        Ok(false)
    }

    /// Get bridge contract for a network
    pub async fn get_bridge_contract(&self, network: &BlockchainNetwork) -> Option<BridgeContract> {
        let bridges = self.bridge_contracts.read().await;
        bridges.get(network).cloned()
    }

    /// Add new bridge contract
    pub async fn add_bridge_contract(&self, contract: BridgeContract) -> Result<()> {
        let mut bridges = self.bridge_contracts.write().await;
        bridges.insert(contract.network.clone(), contract.clone());
        
        tracing::info!("Added bridge contract for {}", contract.network.display_name());
        Ok(())
    }

    /// Create cross-chain transfer
    pub async fn create_cross_chain_transfer(
        &self,
        source_network: BlockchainNetwork,
        target_network: BlockchainNetwork,
        token_symbol: String,
        amount: u64,
        source_address: String,
        target_address: String,
    ) -> Result<CrossChainTransfer> {
        // Get token mapping
        let mapping = self.get_token_mapping(&source_network, &target_network, &token_symbol).await
            .ok_or_else(|| anyhow::anyhow!("No token mapping found for {} -> {}", source_network.display_name(), target_network.display_name()))?;
        
        // Calculate bridge fee
        let bridge_fee = (amount as f64 * mapping.bridge_fee) as u64;
        
        let transfer = CrossChainTransfer {
            transfer_id: format!("transfer_{}", uuid::Uuid::new_v4().to_string().replace("-", "")),
            source_network: source_network.clone(),
            target_network: target_network.clone(),
            token_symbol: token_symbol.clone(),
            amount,
            source_address,
            target_address,
            bridge_fee,
            estimated_time: self.calculate_estimated_time(&source_network, &target_network).await,
            status: TransferStatus::Pending,
            created_at: chrono::Utc::now(),
        };
        
        // Store transfer
        {
            let mut history = self.transfer_history.write().await;
            history.insert(transfer.transfer_id.clone(), transfer.clone());
        }
        
        tracing::info!("Created cross-chain transfer: {} -> {} ({} {})", 
            source_network.display_name(), target_network.display_name(), amount, token_symbol);
        
        Ok(transfer)
    }

    /// Calculate estimated transfer time
    async fn calculate_estimated_time(&self, source: &BlockchainNetwork, target: &BlockchainNetwork) -> u64 {
        // Base time for different network combinations
        let base_time = match (source, target) {
            (BlockchainNetwork::Ronin, BlockchainNetwork::Ethereum) => 15, // 15 minutes
            (BlockchainNetwork::Ethereum, BlockchainNetwork::Ronin) => 15,
            (BlockchainNetwork::Ethereum, BlockchainNetwork::Polygon) => 5,  // 5 minutes
            (BlockchainNetwork::Polygon, BlockchainNetwork::Ethereum) => 5,
            (BlockchainNetwork::Ethereum, BlockchainNetwork::BSC) => 10,     // 10 minutes
            (BlockchainNetwork::BSC, BlockchainNetwork::Ethereum) => 10,
            _ => 20, // Default 20 minutes
        };
        
        // Adjust based on network status
        let network_status = self.network_status.read().await;
        let source_status = network_status.get(source);
        let target_status = network_status.get(target);
        
        let mut adjustment = 0;
        
        if let Some(status) = source_status {
            if status.congestion_level > 0.7 {
                adjustment += 10; // High congestion adds time
            }
        }
        
        if let Some(status) = target_status {
            if status.congestion_level > 0.7 {
                adjustment += 10;
            }
        }
        
        (base_time + adjustment) as u64
    }

    /// Update transfer status
    pub async fn update_transfer_status(&self, transfer_id: &str, status: TransferStatus) -> Result<bool> {
        let mut history = self.transfer_history.write().await;
        
        if let Some(transfer) = history.get_mut(transfer_id) {
            transfer.status = status.clone();
            tracing::info!("Updated transfer {} status to {:?}", transfer_id, status);
            return Ok(true);
        }
        
        Ok(false)
    }

    /// Get transfer history
    pub async fn get_transfer_history(&self, limit: Option<usize>) -> Vec<CrossChainTransfer> {
        let history = self.transfer_history.read().await;
        let mut transfers: Vec<_> = history.values().cloned().collect();
        
        // Sort by creation time (newest first)
        transfers.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        
        // Apply limit if specified
        if let Some(limit) = limit {
            transfers.truncate(limit);
        }
        
        transfers
    }

    /// Update network status
    pub async fn update_network_status(&self, network: BlockchainNetwork, status: NetworkStatus) -> Result<()> {
        let mut network_status = self.network_status.write().await;
        network_status.insert(network.clone(), status);
        
        tracing::debug!("Updated network status for {}", network.display_name());
        Ok(())
    }

    /// Get all network statuses
    pub async fn get_all_network_statuses(&self) -> HashMap<BlockchainNetwork, NetworkStatus> {
        self.network_status.read().await.clone()
    }

    /// Get supported networks for a token
    pub async fn get_supported_networks(&self, token_symbol: &str) -> Vec<BlockchainNetwork> {
        let mappings = self.token_mappings.read().await;
        let mut networks = Vec::new();
        
        for ((network, symbol), _) in mappings.iter() {
            if symbol == token_symbol {
                networks.push(network.clone());
            }
        }
        
        networks
    }

    /// Get bridge statistics
    pub async fn get_bridge_statistics(&self) -> BridgeStatistics {
        let mappings = self.token_mappings.read().await;
        let bridges = self.bridge_contracts.read().await;
        let history = self.transfer_history.read().await;
        
        let total_mappings = mappings.values().map(|v| v.len()).sum();
        let active_bridges = bridges.values().filter(|b| b.is_active).count();
        let total_transfers = history.len();
        let completed_transfers = history.values().filter(|t| t.status == TransferStatus::Completed).count();
        
        BridgeStatistics {
            total_token_mappings: total_mappings,
            active_bridge_contracts: active_bridges,
            total_transfers,
            completed_transfers,
            success_rate: if total_transfers > 0 {
                completed_transfers as f64 / total_transfers as f64
            } else {
                0.0
            },
        }
    }

    /// Record contract task received
    pub async fn record_contract_task(&self, task_id: u64) -> Result<()> {
        tracing::debug!("Recorded contract task received: {}", task_id);
        // Could implement task tracking logic here
        Ok(())
    }

    /// Record contract task processed
    pub async fn record_task_processed(&self, task_id: u64) -> Result<()> {
        tracing::debug!("Recorded contract task processed: {}", task_id);
        // Could implement task completion tracking here
        Ok(())
    }

    /// Record result submitted for contract task
    pub async fn record_result_submitted(&self, task_id: u64, tx_hash: String) -> Result<()> {
        tracing::info!("Recorded result submitted for task {}: {}", task_id, tx_hash);
        // Could implement result tracking logic here
        Ok(())
    }
}

/// Token mapping update structure
#[derive(Debug, Clone)]
pub struct TokenMappingUpdate {
    pub exchange_rate: Option<f64>,
    pub is_active: Option<bool>,
    pub bridge_fee: Option<f64>,
}

/// Bridge statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeStatistics {
    pub total_token_mappings: usize,
    pub active_bridge_contracts: usize,
    pub total_transfers: usize,
    pub completed_transfers: usize,
    pub success_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_mapping_creation() {
        let registry = CrossChainTokenRegistry::new();
        
        let mapping = TokenMapping {
            source_network: BlockchainNetwork::Ethereum,
            source_address: "0x123".to_string(),
            source_symbol: "TEST".to_string(),
            source_decimals: 18,
            target_network: BlockchainNetwork::Polygon,
            target_address: "0x456".to_string(),
            target_symbol: "TEST".to_string(),
            target_decimals: 18,
            exchange_rate: 1.0,
            is_active: true,
            last_updated: chrono::Utc::now(),
            liquidity_score: 0.8,
            bridge_fee: 0.001,
        };
        
        registry.add_token_mapping(mapping).await.unwrap();
        
        let retrieved = registry.get_token_mapping(
            &BlockchainNetwork::Ethereum,
            &BlockchainNetwork::Polygon,
            "TEST"
        ).await;
        
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_cross_chain_transfer() {
        let registry = CrossChainTokenRegistry::new();
        
        let transfer = registry.create_cross_chain_transfer(
            BlockchainNetwork::Ronin,
            BlockchainNetwork::Ethereum,
            "RON".to_string(),
            1000,
            "0x123".to_string(),
            "0x456".to_string(),
        ).await.unwrap();
        
        assert_eq!(transfer.source_network, BlockchainNetwork::Ronin);
        assert_eq!(transfer.target_network, BlockchainNetwork::Ethereum);
        assert_eq!(transfer.token_symbol, "RON");
        assert_eq!(transfer.amount, 1000);
    }
}
