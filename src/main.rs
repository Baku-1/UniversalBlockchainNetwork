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
mod economic_engine;
mod token_registry;
mod task_distributor;
mod gpu_processor;
mod secure_execution;
mod white_noise_crypto;
mod polymorphic_matrix;
mod lending_pools;

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
use tokio::sync::RwLock;
use std::sync::Arc;
use std::path::Path;
use uuid::Uuid;

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

    // Initialize security execution engine
    let secure_execution_engine = Arc::new(secure_execution::SecureExecutionEngine::new());
    tracing::info!("Secure execution engine initialized");

    // Initialize white noise encryption system
    let white_noise_config = white_noise_crypto::WhiteNoiseConfig {
        encryption_algorithm: white_noise_crypto::EncryptionAlgorithm::Hybrid,
        noise_layer_count: 3,
        noise_intensity: 0.7,
        noise_pattern: white_noise_crypto::NoisePattern::Chaotic,
        chaos_seed: 42,
        steganographic_enabled: true,
    };
    let white_noise_encryption = Arc::new(RwLock::new(white_noise_crypto::WhiteNoiseEncryption::new(white_noise_config)?));
    tracing::info!("White noise encryption system initialized");

    // Initialize polymorphic matrix for packet obfuscation
    let polymorphic_matrix = Arc::new(RwLock::new(polymorphic_matrix::PolymorphicMatrix::new()?));
    tracing::info!("Polymorphic matrix system initialized");

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

    // Initialize task distributor for distributed computing
    let (task_distributor, mut task_distribution_events) = task_distributor::TaskDistributor::new();
    let task_distributor = Arc::new(task_distributor);
    tracing::info!("Task distributor initialized");

    // Initialize GPU task scheduler
    let (gpu_scheduler, mut gpu_events) = gpu_processor::GPUTaskScheduler::new();
    let gpu_scheduler = Arc::new(gpu_scheduler);
    tracing::info!("GPU task scheduler initialized");

    // Initialize economic engine
    let economic_engine = Arc::new(economic_engine::EconomicEngine::new());
    tracing::info!("Economic engine initialized");

    // Initialize lending pools manager
    let (lending_pools_manager, mut lending_pool_events) = lending_pools::LendingPoolManager::new();
    let lending_pools_manager = Arc::new(lending_pools_manager);
    tracing::info!("Lending pools manager initialized");

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

    // Initialize mesh network manager
    let (mesh_event_tx, mut mesh_events) = mpsc::channel(100);
    let (mesh_to_validator, mesh_from_validator) = mpsc::channel(100);
    let (mesh_to_result, mesh_from_result) = mpsc::channel(100);
    
    let mesh_manager = mesh::BluetoothMeshManager::new(
        app_config.mesh.clone(),
        node_keys.clone(),
        mesh_to_validator,
        mesh_from_result,
        mesh_event_tx,
    ).await?;
    let mesh_manager = Arc::new(mesh_manager);
    tracing::info!("Bluetooth mesh manager initialized");
    
    // Start the mesh manager service
    let mesh_manager_clone = Arc::clone(&mesh_manager);
    tokio::spawn(async move {
        if let Err(e) = mesh_manager_clone.start().await {
            tracing::error!("Failed to start mesh manager: {}", e);
        }
    });
    tracing::info!("Bluetooth mesh manager service started");

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

    // Spawn comprehensive task distribution service with advanced features
    let task_distributor_clone = Arc::clone(&task_distributor);
    let gpu_scheduler_clone = Arc::clone(&gpu_scheduler);
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            
            // Create and distribute a sample computation task
            let sample_task = crate::validator::ComputationTask {
                id: uuid::Uuid::new_v4(),
                task_type: crate::validator::TaskType::BlockValidation(crate::validator::BlockToValidate {
                    id: "sample_block_123".to_string(),
                    data: b"Sample block data for validation".to_vec(),
                }),
                data: b"Sample block data for validation".to_vec(),
                priority: crate::validator::TaskPriority::Normal,
                created_at: std::time::SystemTime::now(),
            };
            
            // Analyze task complexity using advanced analyzer (this would need access to the ComplexityAnalyzer)
            // For now, we'll use a simple complexity calculation
            let complexity_score = match &sample_task.task_type {
                crate::validator::TaskType::BlockValidation(_) => 0.6,
                crate::validator::TaskType::GameStateUpdate(_) => 0.7,
                crate::validator::TaskType::TransactionValidation(_) => 0.5,
                crate::validator::TaskType::ConflictResolution(_) => 0.8,
            };
            tracing::debug!("Task complexity calculated: {}", complexity_score);
            
            if let Ok(task_id) = task_distributor_clone.distribute_task(sample_task).await {
                tracing::info!("Distributed sample task: {}", task_id);
                
                // Also submit to GPU scheduler for parallel processing
                let gpu_task = crate::gpu_processor::GPUProcessingTask {
                    id: task_id,
                    priority: crate::validator::TaskPriority::Normal,
                    compute_shader: "compute_shader_main".to_string(),
                    input_data: vec![1.0, 2.0, 3.0, 4.0],
                    expected_output_size: 16,
                    deadline: std::time::SystemTime::now() + std::time::Duration::from_secs(300),
                    complexity: crate::gpu_processor::TaskComplexity::Moderate,
                    assigned_node: None,
                    status: crate::gpu_processor::TaskStatus::Pending,
                };
                
                if let Err(e) = gpu_scheduler_clone.submit_task(gpu_task).await {
                    tracing::warn!("Failed to submit GPU task: {}", e);
                }
            }
            
            // Get comprehensive task distribution statistics
            let distributor_stats = task_distributor_clone.get_stats().await;
            tracing::debug!("Task distributor stats: {:?}", distributor_stats);
        }
    });
    tracing::info!("Comprehensive task distribution service started with advanced features");

    // Spawn GPU scheduler monitoring service with advanced features
    let gpu_scheduler_monitor_clone = Arc::clone(&gpu_scheduler);
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(45));
        loop {
            interval.tick().await;
            
            // Register a sample GPU node with capabilities
            let sample_gpu_capability = crate::gpu_processor::GPUCapability {
                compute_units: 2048,
                memory_gb: 8.0,
                compute_capability: 8.6,
                max_workgroup_size: 1024,
                supported_extensions: vec!["CUDA".to_string(), "Tensor Cores".to_string()],
                benchmark_score: 0.85,
            };
            
            if let Err(e) = gpu_scheduler_monitor_clone.register_gpu("sample_gpu_001".to_string(), sample_gpu_capability).await {
                tracing::warn!("Failed to register sample GPU: {}", e);
            }
            
            // Get comprehensive GPU scheduler statistics
            let scheduler_stats = gpu_scheduler_monitor_clone.get_stats().await;
            tracing::debug!("GPU scheduler stats: {:?}", scheduler_stats);
        }
    });
    tracing::info!("GPU scheduler monitoring service started with advanced features");

    // Spawn comprehensive lending pool operations with economic engine integration
    let lending_pools_manager_clone = Arc::clone(&lending_pools_manager);
    let economic_engine_clone = Arc::clone(&economic_engine);
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(120));
        loop {
            interval.tick().await;
            
            // Create a sample lending pool
            let pool_id = format!("POOL_{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());
            if let Err(e) = lending_pools_manager_clone.create_pool(
                pool_id.clone(),
                format!("Sample Pool {}", pool_id),
                0.05, // 5% interest rate
            ).await {
                tracing::warn!("Failed to create lending pool: {}", e);
            } else {
                tracing::info!("Created lending pool: {}", pool_id);
                
                // Create a sample loan
                if let Err(e) = lending_pools_manager_clone.create_loan(
                    &pool_id,
                    format!("BORROWER_{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap()),
                    1000, // 1000 RON
                    1500, // 1500 RON collateral
                ).await {
                    tracing::warn!("Failed to create loan in pool {}: {}", pool_id, e);
                } else {
                    tracing::info!("Created sample loan in pool: {}", pool_id);
                }
            }
            
            // Use economic engine to create additional lending pools
            if let Err(e) = economic_engine_clone.create_lending_pool(format!("Economic Pool {}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap())).await {
                tracing::warn!("Failed to create economic engine pool: {}", e);
            }
            
            // Get comprehensive economic statistics
            let economic_stats = economic_engine_clone.get_economic_stats().await;
            tracing::debug!("Economic engine stats: {:?}", economic_stats);
        }
    });
    tracing::info!("Comprehensive lending pool operations started with economic engine integration");

    // Spawn comprehensive mesh network operations with advanced features
    let mesh_manager_clone = Arc::clone(&mesh_manager);
    let white_noise_encryption_clone = Arc::clone(&white_noise_encryption);
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(45));
        loop {
            interval.tick().await;
            
            // Generate and broadcast encrypted health check messages
            let health_data = format!("HEALTH_CHECK_{}", uuid::Uuid::new_v4()).into_bytes();
            let mut encryption = white_noise_encryption_clone.write().await;
            if let Ok(encrypted_data) = encryption.encrypt_data(&health_data, &[1u8; 32]).await {
                let health_message = crate::mesh::MeshMessage {
                    id: uuid::Uuid::new_v4(),
                    sender_id: "node_001".to_string(),
                    target_id: None, // None for broadcast
                    message_type: crate::mesh::MeshMessageType::Heartbeat,
                    payload: encrypted_data.encrypted_content,
                    ttl: 3,
                    hop_count: 0,
                    timestamp: std::time::SystemTime::now(),
                    signature: vec![], // Placeholder signature
                };
                if let Err(e) = mesh_manager_clone.process_message(health_message).await {
                    tracing::warn!("Failed to broadcast health check: {}", e);
                } else {
                    tracing::debug!("Broadcasted encrypted health check message");
                }
            }
            
            // Simulate peer discovery and connection
            let simulated_peer = format!("PEER_{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap());
            tracing::debug!("Simulating peer discovery: {}", simulated_peer);
            
            // Update mesh routing table for better network topology
            if let Err(e) = mesh_manager_clone.update_routing_table().await {
                tracing::warn!("Failed to update routing table: {}", e);
            }
        }
    });
    tracing::info!("Comprehensive mesh network operations started with advanced features");

    // Spawn comprehensive security monitoring service with active packet generation
    let secure_execution_engine_clone = Arc::clone(&secure_execution_engine);
    let white_noise_encryption_clone = Arc::clone(&white_noise_encryption);
    let polymorphic_matrix_clone = Arc::clone(&polymorphic_matrix);
    let mesh_manager_clone = Arc::clone(&mesh_manager);
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            
            // Run comprehensive security audit
            if let Err(e) = secure_execution_engine_clone.run_security_audit().await {
                tracing::warn!("Security audit failed: {}", e);
            }
            
            // Run runtime integrity checks using the runtime integrity checker
            if let Err(e) = secure_execution_engine_clone.runtime_integrity_checker.check_integrity().await {
                tracing::warn!("Integrity check failed: {}", e);
            }
            
            // Check for debugger detection using the anti-debug protection
            if let Err(e) = secure_execution_engine_clone.anti_debug_protection.check_debugger().await {
                tracing::warn!("Debugger check failed: {}", e);
            }
            
            // Get comprehensive security status
            let security_status = secure_execution_engine_clone.get_security_status().await;
            tracing::debug!("Security status: {:?}", security_status);
            
            // Generate fake transaction packets for obfuscation
            let fake_data = format!("FAKE_TX_{}", uuid::Uuid::new_v4()).into_bytes();
            let mut matrix = polymorphic_matrix_clone.write().await;
            if let Ok(packet) = matrix.generate_polymorphic_packet(&fake_data, polymorphic_matrix::PacketType::GhostProtocol).await {
                tracing::debug!("Generated fake transaction packet with {} layers", packet.layer_count);
                
                // Broadcast fake packet through mesh network using process_message
                let broadcast_message = crate::mesh::MeshMessage {
                    id: uuid::Uuid::new_v4(),
                    sender_id: "node_001".to_string(),
                    target_id: None, // None for broadcast
                    message_type: crate::mesh::MeshMessageType::MeshTransaction,
                    payload: packet.encrypted_content,
                    ttl: 5,
                    hop_count: 0,
                    timestamp: std::time::SystemTime::now(),
                    signature: vec![], // Placeholder signature
                };
                if let Err(e) = mesh_manager_clone.process_message(broadcast_message).await {
                    tracing::warn!("Failed to broadcast fake transaction: {}", e);
                }
            }
            
            // Check encryption system health
            let encryption_stats = {
                let encryption = white_noise_encryption_clone.read().await;
                encryption.get_encryption_stats().await
            };
            tracing::debug!("Encryption system stats: {} records, {}ms avg time", 
                encryption_stats.total_encryptions, encryption_stats.average_encryption_time_ms);
            
            // Update polymorphic matrix statistics
            let matrix_stats = {
                let matrix = polymorphic_matrix_clone.read().await;
                matrix.get_statistics().clone()
            };
            tracing::debug!("Polymorphic matrix stats: {} packets, {} avg layers", 
                matrix_stats.total_packets_generated, matrix_stats.average_layers_per_packet);
        }
    });
    tracing::info!("Comprehensive security monitoring service started with active packet generation");

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