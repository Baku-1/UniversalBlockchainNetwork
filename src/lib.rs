// src/lib.rs - Library interface for AuraValidationNetwork

// Declare our application's modules
pub mod config;
pub mod crypto;
pub mod ipc;
pub mod p2p;
pub mod validator;
pub mod errors;
pub mod shared_types;

// NEW MODULES FOR FINAL 5% IMPLEMENTATION
pub mod economic_engine;
pub mod token_registry;
pub mod lending_pools;
pub mod gpu_processor;
pub mod task_distributor;
pub mod secure_execution;
pub mod white_noise_crypto;
pub mod polymorphic_matrix;
pub mod engine_shell;

// Modules that depend on the above modules
pub mod web3;
pub mod contract_integration;
pub mod bridge_node;
pub mod store_forward;
pub mod mesh;
pub mod mesh_topology;
pub mod mesh_routing;
pub mod aura_protocol;
pub mod transaction_queue;
pub mod sync;
pub mod mesh_validation;

// Re-export public APIs for external use
pub use config::*;
pub use contract_integration::{ContractIntegration, ContractTask, TaskStatus as ContractTaskStatus, TaskResult as ContractTaskResult};
pub use crypto::*;
pub use mesh_validation::{MeshValidator, MeshTransaction, ValidationResult};
pub use bridge_node::{BridgeNode, BridgeEvent};
pub use store_forward::{StoreForwardManager, ForwardedMessage};
pub use transaction_queue::*;
pub use web3::{RoninClient, RoninTransaction, TransactionStatus};
pub use mesh_topology::*;
pub use errors::{NexusError, ErrorContext};
pub use aura_protocol::{AuraProtocolClient, ValidationTask, TaskStatus, TaskResult};

// NEW EXPORTS FOR FINAL 5% IMPLEMENTATION
pub use economic_engine::{EconomicEngine, InterestRateEngine, NetworkStats, EconomicStats};
pub use token_registry::{CrossChainTokenRegistry, BlockchainNetwork, TokenMapping, CrossChainTransfer};
pub use lending_pools::{LendingPoolManager, LendingPool, LoanDetails, PoolStats, ManagerStats};
pub use gpu_processor::{GPUProcessor, GPUTaskScheduler, GPUCapability, GPUProcessingTask};
pub use task_distributor::{TaskDistributor, DeviceCapability, BalancingStrategy};
pub use secure_execution::{SecureExecutionEngine, SecurityStatus, SecurityLevel, SecurityAuditResult};
pub use white_noise_crypto::{WhiteNoiseEncryption, WhiteNoiseConfig, EncryptionAlgorithm, NoisePattern};
pub use polymorphic_matrix::{PolymorphicMatrix, PacketRecipe, PolymorphicPacket, PacketType, LayerType};
pub use engine_shell::{EngineShellEncryption, EngineShellConfig, EncryptedEngineShell, EngineShellLayer};
