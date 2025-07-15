// src/lib.rs - Library interface for AuraValidationNetwork

// Declare our application's modules
pub mod config;
pub mod crypto;
pub mod ipc;
pub mod p2p;
pub mod validator;
pub mod web3;
pub mod transaction_queue;
pub mod sync;
pub mod mesh_validation;
pub mod bridge_node;
pub mod store_forward;
pub mod mesh;
pub mod mesh_topology;
pub mod mesh_routing;
pub mod errors;
pub mod aura_protocol;

// Re-export public APIs for external use
pub use config::*;
pub use crypto::*;
pub use mesh_validation::{MeshValidator, MeshTransaction, ValidationResult};
pub use bridge_node::{BridgeNode, BridgeEvent};
pub use store_forward::{StoreForwardManager, ForwardedMessage};
pub use transaction_queue::*;
pub use web3::{RoninClient, RoninTransaction, TransactionStatus};
pub use mesh_topology::*;
pub use errors::{NexusError, ErrorContext};
pub use aura_protocol::{AuraProtocolClient, ValidationTask, TaskStatus, TaskResult};
