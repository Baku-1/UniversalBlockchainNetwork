// src/main.rs

// Declare our application's modules
mod config;
mod crypto;
mod ipc;
mod p2p;
mod validator;
mod web3;
mod transaction_queue;
mod sync;
mod mesh_validation;
mod bridge_node;
mod store_forward;
mod mesh;
mod mesh_topology;
mod mesh_routing;
mod errors;
mod aura_protocol;
mod contract_integration;

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

use tokio::sync::mpsc;
use std::sync::Arc;
use std::path::Path;

/// The main entry point for the Nexus Engine.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --- 1. System Initialization ---

    // Set up logging to provide structured, informative output.
    tracing_subscriber::fmt::init();
    tracing::info!("Starting Aura Validation Network - Ronin Blockchain Mesh Utility");

    // Load configuration from Settings.toml
    let app_config = config::load_config()?;
    tracing::info!("Configuration loaded successfully");

    // Generate a unique, persistent cryptographic identity for this node.
    let node_keys = crypto::load_or_create_keypair(&app_config.keys_path)?;
    tracing::info!("Node identity loaded: {}", node_keys.node_id());

    // --- 2. Initialize Core Components ---

    // Create channels for validator communication
    let (computation_task_tx, computation_task_rx) = mpsc::channel(128);
    let (task_result_tx, task_result_rx) = mpsc::channel(128);

    // Create channels for transaction queue events
    let (queue_event_tx, mut queue_event_rx) = mpsc::channel(100);

    // Initialize offline transaction queue
    let db_path = Path::new("./data/transactions.db");
    let transaction_queue = Arc::new(
        transaction_queue::OfflineTransactionQueue::new(
            db_path,
            app_config.ronin.clone(),
            queue_event_tx,
        ).await?
    );
    tracing::info!("Transaction queue initialized");

    // Initialize Web3 sync manager
    let sync_manager = sync::Web3SyncManager::new(
        app_config.ronin.clone(),
        Arc::clone(&transaction_queue),
    )?;
    let mut sync_events = sync_manager.start_sync_service().await;
    tracing::info!("Web3 sync manager started");

    // Initialize Ronin connectivity monitor
    let ronin_client = web3::RoninClient::new(app_config.ronin.clone())?;
    let connectivity_monitor = Arc::new(ronin_client);
    let monitor_clone = Arc::clone(&connectivity_monitor);

    // Start connectivity monitoring task
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let is_connected = monitor_clone.check_connectivity().await;

            // Get additional network info if connected
            let (block_number, gas_price) = if is_connected {
                let block = monitor_clone.get_block_number().await.ok();
                let gas = monitor_clone.get_gas_price().await.ok();
                (block, gas)
            } else {
                (None, None)
            };

            // Update IPC status
            ipc::update_engine_status(|status| {
                status.ronin_connected = is_connected;
                status.ronin_block_number = block_number;
                status.ronin_gas_price = gas_price;
                if is_connected {
                    tracing::debug!("Ronin connectivity confirmed - Block: {:?}, Gas: {:?}", block_number, gas_price);
                } else {
                    tracing::debug!("Ronin connectivity lost");
                }
            }).await;
        }
    });
    tracing::info!("Ronin connectivity monitor started");

    // Initialize mesh validation system
    let mesh_validator = Arc::new(mesh_validation::MeshValidator::new(
        node_keys.clone(),
        app_config.ronin.clone(),
    ));
    tracing::info!("Mesh validator initialized");

    // Initialize AuraProtocol contract client (if contract address is configured)
    let aura_protocol_client = if let Some(contract_address) = app_config.aura_protocol_address.as_ref() {
        match contract_address.parse() {
            Ok(address) => {
                match aura_protocol::AuraProtocolClient::new(
                    address,
                    Arc::clone(&connectivity_monitor),
                    node_keys.clone(),
                    app_config.ronin.clone(),
                ) {
                    Ok(client) => {
                        tracing::info!("AuraProtocol client initialized at address: {}", contract_address);
                        Some(Arc::new(client))
                    }
                    Err(e) => {
                        tracing::warn!("Failed to initialize AuraProtocol client: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Invalid AuraProtocol contract address: {}", e);
                None
            }
        }
    } else {
        tracing::info!("No AuraProtocol contract address configured, running without contract integration");
        None
    };

    // Initialize bridge node for blockchain settlement
    let mut bridge_node = bridge_node::BridgeNode::new(
        Arc::clone(&connectivity_monitor),
        Arc::clone(&transaction_queue),
    );

    // Configure bridge node with contract integration
    if let Some(client) = aura_protocol_client.as_ref() {
        bridge_node.set_aura_protocol_client(Arc::clone(client));
    }
    bridge_node.set_mesh_validator(Arc::clone(&mesh_validator));

    let bridge_node = Arc::new(bridge_node);
    let mut bridge_events = bridge_node.start_bridge_service().await;
    tracing::info!("Bridge node started with contract integration");

    // Initialize store & forward system
    let store_forward = Arc::new(store_forward::StoreForwardManager::new(
        node_keys.clone(),
        app_config.mesh.clone(),
    ));
    let mut store_forward_events = store_forward.start_service().await;
    tracing::info!("Store & forward system started");

    // --- 3. Spawning Core Services ---

    // Spawn the validator engine
    tokio::spawn(validator::start_validator(
        computation_task_rx,
        task_result_tx,
    ));
    tracing::info!("Validator engine started");

    // Spawn the IPC WebSocket server
    tokio::spawn(ipc::start_ipc_server(app_config.ipc_port));
    tracing::info!("IPC server started on port {}", app_config.ipc_port);

    // Spawn the main P2P networking task
    tokio::spawn(p2p::start_p2p_node(
        node_keys.clone(),
        computation_task_tx,
        task_result_rx,
    ));
    tracing::info!("P2P networking started");

    // --- 4. Event Processing Loop ---

    tracing::info!("Aura Validation Network fully initialized and running");
    tracing::info!("Bluetooth mesh networking enabled for offline Ronin transactions");
    tracing::info!("Press CTRL+C to shut down");

    loop {
        tokio::select! {
            // Handle transaction queue events
            Some(queue_event) = queue_event_rx.recv() => {
                match queue_event {
                    transaction_queue::QueueEvent::TransactionAdded(tx_id) => {
                        tracing::debug!("Transaction {} added to offline queue", tx_id);
                    }
                    transaction_queue::QueueEvent::TransactionCompleted(tx_id) => {
                        tracing::info!("Transaction {} completed successfully", tx_id);
                    }
                    transaction_queue::QueueEvent::TransactionFailed(tx_id, error) => {
                        tracing::warn!("Transaction {} failed: {}", tx_id, error);
                    }
                    transaction_queue::QueueEvent::QueueSizeChanged(size) => {
                        tracing::debug!("Transaction queue size: {}", size);
                    }
                }
            }

            // Handle Web3 sync events
            Some(sync_event) = sync_events.recv() => {
                match sync_event {
                    sync::SyncEvent::SyncStarted => {
                        tracing::info!("Web3 synchronization started");
                    }
                    sync::SyncEvent::SyncCompleted => {
                        tracing::info!("Web3 synchronization completed");
                    }
                    sync::SyncEvent::SyncFailed(error) => {
                        tracing::error!("Web3 synchronization failed: {}", error);
                    }
                    sync::SyncEvent::TransactionSynced(tx_id) => {
                        tracing::debug!("Transaction {} synced to Ronin network", tx_id);
                    }
                    sync::SyncEvent::TransactionFailed(tx_id, error) => {
                        tracing::warn!("Transaction {} sync failed: {}", tx_id, error);
                    }
                }
            }

            // Handle bridge node events
            Some(bridge_event) = bridge_events.recv() => {
                match bridge_event {
                    bridge_node::BridgeEvent::ConnectivityRestored => {
                        tracing::info!("Internet connectivity restored, starting bridge operations");
                    }
                    bridge_node::BridgeEvent::MeshTransactionSettled(tx_id) => {
                        tracing::info!("Mesh transaction {} settled on Ronin blockchain", tx_id);
                    }
                    bridge_node::BridgeEvent::SettlementFailed(tx_id, error) => {
                        tracing::error!("Settlement failed for transaction {}: {}", tx_id, error);
                    }
                    bridge_node::BridgeEvent::BatchSettlementCompleted(count) => {
                        tracing::info!("Batch settlement completed: {} transactions", count);
                    }
                    bridge_node::BridgeEvent::ContractTaskReceived(task_id) => {
                        tracing::info!("Contract validation task {} received", task_id);
                    }
                    bridge_node::BridgeEvent::ContractTaskProcessed(task_id) => {
                        tracing::info!("Contract validation task {} processed", task_id);
                    }
                    bridge_node::BridgeEvent::ContractResultSubmitted(task_id, tx_hash) => {
                        tracing::info!("Contract task {} result submitted: {}", task_id, tx_hash);
                    }
                    bridge_node::BridgeEvent::ContractEventProcessed(event_type) => {
                        tracing::debug!("Contract event processed: {}", event_type);
                    }
                }
            }

            // Handle store & forward events
            Some(sf_event) = store_forward_events.recv() => {
                match sf_event {
                    store_forward::StoreForwardEvent::MessageStored(msg_id) => {
                        tracing::debug!("Message {} stored for offline user", msg_id);
                    }
                    store_forward::StoreForwardEvent::MessageDelivered(msg_id) => {
                        tracing::debug!("Stored message {} delivered", msg_id);
                    }
                    store_forward::StoreForwardEvent::IncentiveEarned(amount) => {
                        tracing::info!("Earned {} RON for store & forward service", amount);
                    }
                    store_forward::StoreForwardEvent::MessageExpired(msg_id) => {
                        tracing::debug!("Stored message {} expired", msg_id);
                    }
                    store_forward::StoreForwardEvent::DeliveryFailed(msg_id, error) => {
                        tracing::warn!("Failed to deliver message {}: {}", msg_id, error);
                    }
                }
            }

            // Handle shutdown signal
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Shutdown signal received. Terminating services...");
                break;
            }
        }
    }

    // --- 5. Graceful Shutdown ---

    tracing::info!("Performing graceful shutdown...");

    // Clean up transaction queue
    let cleanup_count = transaction_queue.cleanup().await?;
    tracing::info!("Cleaned up {} completed transactions", cleanup_count);

    // Get final statistics
    let queue_stats = transaction_queue.get_stats().await;
    tracing::info!("Final queue stats: {:?}", queue_stats);

    let sync_stats = sync_manager.get_sync_stats().await;
    tracing::info!("Final sync stats: {:?}", sync_stats);

    tracing::info!("Aura Validation Network shutdown complete");
    Ok(())
}