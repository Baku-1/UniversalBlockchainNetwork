// src/web3.rs

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::sync::Arc;
use ethers::{
    providers::{Http, Provider, Middleware},
    types::{Address, U256, H256, TransactionRequest},
    abi::{encode, Token},
    utils::keccak256,
};
use crate::config::RoninConfig;
use crate::token_registry::CrossChainTokenRegistry;

/// Ronin blockchain transaction structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoninTransaction {
    pub id: Uuid,
    pub from: String,
    pub to: String,
    pub value: u64, // Value in wei
    pub gas_price: u64,
    pub gas_limit: u64,
    pub nonce: u64,
    pub data: Vec<u8>, // Contract call data
    pub chain_id: u64,
    pub created_at: std::time::SystemTime,
    pub status: TransactionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransactionStatus {
    Pending,
    Queued,
    Submitted,
    Confirmed,
    Failed(String),
}

/// NFT operation on Ronin (Axie Infinity assets)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftOperation {
    pub id: Uuid,
    pub operation_type: NftOperationType,
    pub contract_address: String,
    pub token_id: u64,
    pub from: String,
    pub to: String,
    pub created_at: std::time::SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NftOperationType {
    Transfer,
    Mint,
    Burn,
    Approve,
    SetApprovalForAll,
}

/// Smart contract interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCall {
    pub id: Uuid,
    pub contract_address: String,
    pub function_name: String,
    pub parameters: Vec<ContractParameter>,
    pub gas_estimate: u64,
    pub created_at: std::time::SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractParameter {
    pub param_type: String, // e.g., "uint256", "address", "string"
    pub value: String,
}

/// Utility transaction types for Ronin mesh network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UtilityTransaction {
    TokenTransfer {
        token_type: String,
        amount: u64,
        to_address: String,
    },
    NFTTransfer {
        contract_address: String,
        token_id: u64,
        to_address: String,
    },
    MeshSettlement {
        net_transfers: Vec<String>, // Encoded transfer data
        participant_count: u32,
    },
}

/// Network information for developer utility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub chain_id: u64,
    pub block_number: u64,
    pub gas_price: u64,
    pub rpc_url: String,
    pub is_connected: bool,
}

/// Ronin blockchain client for Web3 operations
pub struct RoninClient {
    config: RoninConfig,
    pub provider: Arc<Provider<Http>>,
    chain_id: u64,
    
    // Cross-chain token registry integration
    token_registry: Arc<CrossChainTokenRegistry>,
}

impl RoninClient {
    pub fn new(config: RoninConfig) -> Result<Self, Box<dyn std::error::Error>> {
        // Create HTTP provider for Ronin RPC
        let provider = Provider::<Http>::try_from(&config.rpc_url)?;
        let provider = Arc::new(provider);

        // Initialize cross-chain token registry
        let token_registry = Arc::new(CrossChainTokenRegistry::new());

        Ok(Self {
            config: config.clone(),
            provider,
            chain_id: config.chain_id,
            token_registry,
        })
    }

    /// Check if we have internet connectivity to Ronin network
    pub async fn check_connectivity(&self) -> bool {
        match self.provider.get_block_number().await {
            Ok(_) => {
                tracing::debug!("Ronin network connectivity confirmed");
                true
            }
            Err(e) => {
                tracing::debug!("Ronin network connectivity failed: {}", e);
                false
            }
        }
    }

    /// Submit a transaction to the Ronin network
    pub async fn submit_transaction(&self, tx: &RoninTransaction) -> Result<String, String> {
        tracing::info!("Submitting transaction {} to Ronin network", tx.id);

        if !self.check_connectivity().await {
            return Err("No connectivity to Ronin network".to_string());
        }

        // Parse addresses
        let from_addr: Address = tx.from.parse()
            .map_err(|e| format!("Invalid from address: {}", e))?;
        let to_addr: Address = tx.to.parse()
            .map_err(|e| format!("Invalid to address: {}", e))?;

        // Create transaction request
        let tx_request = TransactionRequest::new()
            .from(from_addr)
            .to(to_addr)
            .value(U256::from(tx.value))
            .gas_price(U256::from(tx.gas_price))
            .gas(U256::from(tx.gas_limit))
            .nonce(U256::from(tx.nonce))
            .data(tx.data.clone())
            .chain_id(self.chain_id);

        // Note: For actual transaction submission, you would need a wallet/signer
        // This is a read-only implementation for the utility application
        // In a real game, developers would integrate their own signing mechanism
        
        // Log transaction request details for debugging
        tracing::debug!("Transaction request prepared - From: {}, To: {}, Value: {}, Gas: {}", 
            from_addr, to_addr, tx.value, tx.gas_limit);
        tracing::debug!("Transaction request data size: {} bytes", tx_request.data.as_ref().map(|d| d.len()).unwrap_or(0));

        // For now, we'll simulate the transaction hash generation
        // Real implementation would use: provider.send_transaction(tx_request, None).await
        let simulated_hash = format!("0x{:064x}", tx.id.as_u128());

        tracing::info!("Transaction would be submitted with hash: {}", simulated_hash);
        Ok(simulated_hash)
    }

    /// Get transaction status from Ronin network
    pub async fn get_transaction_status(&self, tx_hash: &str) -> Result<TransactionStatus, String> {
        tracing::debug!("Checking status for transaction: {}", tx_hash);

        if !self.check_connectivity().await {
            return Err("No connectivity to Ronin network".to_string());
        }

        // Parse transaction hash
        let hash: H256 = tx_hash.parse()
            .map_err(|e| format!("Invalid transaction hash: {}", e))?;

        // Get transaction receipt
        match self.provider.get_transaction_receipt(hash).await {
            Ok(Some(receipt)) => {
                if receipt.status == Some(1.into()) {
                    Ok(TransactionStatus::Confirmed)
                } else {
                    Ok(TransactionStatus::Failed("Transaction reverted".to_string()))
                }
            }
            Ok(None) => {
                // Transaction not found, check if it's pending
                match self.provider.get_transaction(hash).await {
                    Ok(Some(_)) => Ok(TransactionStatus::Submitted),
                    Ok(None) => Ok(TransactionStatus::Failed("Transaction not found".to_string())),
                    Err(e) => Err(format!("Error checking transaction: {}", e)),
                }
            }
            Err(e) => Err(format!("Error getting transaction receipt: {}", e)),
        }
    }

    /// Get current gas price from Ronin network
    pub async fn get_gas_price(&self) -> Result<u64, String> {
        if !self.check_connectivity().await {
            return Err("No connectivity to Ronin network".to_string());
        }

        match self.provider.get_gas_price().await {
            Ok(gas_price) => {
                let gas_price_u64 = gas_price.as_u64();
                tracing::debug!("Current Ronin gas price: {} wei", gas_price_u64);
                Ok(gas_price_u64)
            }
            Err(e) => {
                tracing::warn!("Failed to get gas price, using config default: {}", e);
                Ok(self.config.gas_price)
            }
        }
    }

    /// Get nonce for an address
    pub async fn get_nonce(&self, address: &str) -> Result<u64, String> {
        tracing::debug!("Getting nonce for address: {}", address);

        if !self.check_connectivity().await {
            return Err("No connectivity to Ronin network".to_string());
        }

        let addr: Address = address.parse()
            .map_err(|e| format!("Invalid address: {}", e))?;

        match self.provider.get_transaction_count(addr, None).await {
            Ok(nonce) => {
                let nonce_u64 = nonce.as_u64();
                tracing::debug!("Nonce for address {}: {}", address, nonce_u64);
                Ok(nonce_u64)
            }
            Err(e) => Err(format!("Failed to get nonce: {}", e)),
        }
    }

    /// Estimate gas for a transaction
    pub async fn estimate_gas(&self, tx: &RoninTransaction) -> Result<u64, String> {
        tracing::debug!("Estimating gas for transaction: {}", tx.id);

        if !self.check_connectivity().await {
            return Err("No connectivity to Ronin network".to_string());
        }

        // Parse addresses for validation
        let from_addr: Address = tx.from.parse()
            .map_err(|e| format!("Invalid from address: {}", e))?;
        let to_addr: Address = tx.to.parse()
            .map_err(|e| format!("Invalid to address: {}", e))?;
        
        // Log address validation for debugging
        tracing::debug!("Address validation - From: {}, To: {}", from_addr, to_addr);

        // For gas estimation in this utility application, we'll use a simplified approach
        // Real game implementations would need proper transaction signing and gas estimation

        // Try to get current gas price to validate connectivity
        match self.get_gas_price().await {
            Ok(_) => {
                // If we can get gas price, estimate based on transaction type
                let base_gas = if tx.data.is_empty() {
                    21000 // Simple transfer
                } else {
                    100000 // Contract interaction
                };

                tracing::debug!("Gas estimate for transaction {}: {}", tx.id, base_gas);
                Ok(base_gas)
            }
            Err(e) => {
                tracing::warn!("Failed to estimate gas, using config default: {}", e);
                Ok(self.config.gas_limit)
            }
        }
    }

    /// Get the current block number from Ronin network
    pub async fn get_block_number(&self) -> Result<u64, String> {
        if !self.check_connectivity().await {
            return Err("No connectivity to Ronin network".to_string());
        }

        match self.provider.get_block_number().await {
            Ok(block_number) => {
                let block_u64 = block_number.as_u64();
                tracing::debug!("Current Ronin block number: {}", block_u64);
                Ok(block_u64)
            }
            Err(e) => Err(format!("Failed to get block number: {}", e)),
        }
    }

    /// Get the chain ID for this Ronin client
    pub fn get_chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Get access to the cross-chain token registry
    pub fn get_token_registry(&self) -> &Arc<CrossChainTokenRegistry> {
        &self.token_registry
    }

    /// Check if a token is supported on the current chain by checking token mappings
    pub async fn is_token_supported(&self, token_symbol: &str) -> Result<bool, String> {
        let registry = self.get_token_registry();
        let mappings = registry.token_mappings.read().await;
        
        // Check if any mapping contains this token symbol
        let is_supported = mappings.iter().any(|((_, symbol), _)| symbol == token_symbol);
        Ok(is_supported)
    }

    /// Get supported networks for a specific token
    pub async fn get_supported_networks(&self, token_symbol: &str) -> Result<Vec<String>, String> {
        let registry = self.get_token_registry();
        let networks = registry.get_supported_networks(token_symbol).await;
        
        // Convert BlockchainNetwork to String representation
        let network_strings: Vec<String> = networks.iter()
            .map(|network| format!("{:?}", network))
            .collect();
        
        Ok(network_strings)
    }

    /// Get network information for developer utility
    pub async fn get_network_info(&self) -> Result<NetworkInfo, String> {
        if !self.check_connectivity().await {
            return Err("No connectivity to Ronin network".to_string());
        }

        let block_number = self.get_block_number().await?;
        let gas_price = self.get_gas_price().await?;

        Ok(NetworkInfo {
            chain_id: self.chain_id,
            block_number,
            gas_price,
            rpc_url: self.config.rpc_url.clone(),
            is_connected: true,
        })
    }
}

/// Utility functions for Ronin blockchain operations
pub mod utils {
    use super::*;

    /// Create a simple RON token transfer transaction
    pub fn create_ron_transfer(
        from: String,
        to: String,
        amount: u64,
        nonce: u64,
        gas_price: u64,
        chain_id: u64,
    ) -> RoninTransaction {
        RoninTransaction {
            id: Uuid::new_v4(),
            from,
            to,
            value: amount,
            gas_price,
            gas_limit: 21000, // Standard transfer gas limit
            nonce,
            data: vec![], // Empty for simple transfers
            chain_id,
            created_at: std::time::SystemTime::now(),
            status: TransactionStatus::Pending,
        }
    }

    /// Create an NFT transfer transaction
    pub fn create_nft_transfer(
        contract_address: String,
        from: String,
        to: String,
        token_id: u64,
        nonce: u64,
        gas_price: u64,
        chain_id: u64,
    ) -> RoninTransaction {
        // Encode ERC-721 transferFrom call data using proper ABI encoding
        let call_data = encode_transfer_from(&from, &to, token_id);

        RoninTransaction {
            id: Uuid::new_v4(),
            from,
            to: contract_address,
            value: 0, // No ETH value for NFT transfers
            gas_price,
            gas_limit: 100000, // Higher gas limit for contract calls
            nonce,
            data: call_data,
            chain_id,
            created_at: std::time::SystemTime::now(),
            status: TransactionStatus::Pending,
        }
    }

    /// Encode ERC-721 transferFrom function call
    fn encode_transfer_from(from: &str, to: &str, token_id: u64) -> Vec<u8> {
        // Implement proper ABI encoding using ethers-rs
        // ERC-721 transferFrom function signature: transferFrom(address,address,uint256)
        
        // Parse addresses to ensure they're valid
        let from_addr = from.parse::<Address>().expect("Invalid from address");
        let to_addr = to.parse::<Address>().expect("Invalid to address");
        
        // Create function selector for transferFrom(address,address,uint256)
        let function_signature = "transferFrom(address,address,uint256)";
        let selector = &keccak256(function_signature.as_bytes())[0..4];
        
        // Encode the parameters using ethers ABI encoding
        let tokens = vec![
            Token::Address(from_addr),
            Token::Address(to_addr),
            Token::Uint(U256::from(token_id)),
        ];
        
        let encoded_params = encode(&tokens);
        
        // Combine selector + encoded parameters
        let mut call_data = Vec::new();
        call_data.extend_from_slice(selector);
        call_data.extend_from_slice(&encoded_params);
        
        call_data
    }

    /// Encode ERC-721 approve function call
    pub fn encode_approve(to: &str, token_id: u64) -> Vec<u8> {
        // ERC-721 approve function signature: approve(address,uint256)
        let to_addr = to.parse::<Address>().expect("Invalid to address");
        
        let function_signature = "approve(address,uint256)";
        let selector = &keccak256(function_signature.as_bytes())[0..4];
        
        let tokens = vec![
            Token::Address(to_addr),
            Token::Uint(U256::from(token_id)),
        ];
        
        let encoded_params = encode(&tokens);
        
        let mut call_data = Vec::new();
        call_data.extend_from_slice(selector);
        call_data.extend_from_slice(&encoded_params);
        
        call_data
    }

    /// Encode ERC-721 setApprovalForAll function call
    pub fn encode_set_approval_for_all(operator: &str, approved: bool) -> Vec<u8> {
        // ERC-721 setApprovalForAll function signature: setApprovalForAll(address,bool)
        let operator_addr = operator.parse::<Address>().expect("Invalid operator address");
        
        let function_signature = "setApprovalForAll(address,bool)";
        let selector = &keccak256(function_signature.as_bytes())[0..4];
        
        let tokens = vec![
            Token::Address(operator_addr),
            Token::Bool(approved),
        ];
        
        let encoded_params = encode(&tokens);
        
        let mut call_data = Vec::new();
        call_data.extend_from_slice(selector);
        call_data.extend_from_slice(&encoded_params);
        
        call_data
    }

    /// Encode ERC-721 mint function call (common implementation)
    pub fn encode_mint(to: &str, token_id: u64) -> Vec<u8> {
        // Common ERC-721 mint function signature: mint(address,uint256)
        let to_addr = to.parse::<Address>().expect("Invalid to address");
        
        let function_signature = "mint(address,uint256)";
        let selector = &keccak256(function_signature.as_bytes())[0..4];
        
        let tokens = vec![
            Token::Address(to_addr),
            Token::Uint(U256::from(token_id)),
        ];
        
        let encoded_params = encode(&tokens);
        
        let mut call_data = Vec::new();
        call_data.extend_from_slice(selector);
        call_data.extend_from_slice(&encoded_params);
        
        call_data
    }

    /// Encode ERC-721 burn function call (common implementation)
    pub fn encode_burn(token_id: u64) -> Vec<u8> {
        // Common ERC-721 burn function signature: burn(uint256)
        let function_signature = "burn(uint256)";
        let selector = &keccak256(function_signature.as_bytes())[0..4];
        
        let tokens = vec![
            Token::Uint(U256::from(token_id)),
        ];
        
        let encoded_params = encode(&tokens);
        
        let mut call_data = Vec::new();
        call_data.extend_from_slice(selector);
        call_data.extend_from_slice(&encoded_params);
        
        call_data
    }

    /// Validate Ronin address format
    pub fn is_valid_address(address: &str) -> bool {
        // Ronin addresses are Ethereum-compatible
        address.starts_with("0x") && address.len() == 42
    }

    /// Convert wei to RON (Ronin native token)
    pub fn wei_to_ron(wei: u64) -> f64 {
        wei as f64 / 1_000_000_000_000_000_000.0 // 18 decimals
    }

    /// Convert RON to wei
    pub fn ron_to_wei(ron: f64) -> u64 {
        (ron * 1_000_000_000_000_000_000.0) as u64
    }

}
