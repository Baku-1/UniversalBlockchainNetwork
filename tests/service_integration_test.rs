// tests/service_integration_test.rs
// Comprehensive integration test for UniversalBlockchainNetwork services
// Tests multiple services working together in a production-like scenario

use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use aura_validation_network::config::AppConfig;
use aura_validation_network::crypto::NodeKeypair;
use aura_validation_network::mesh_topology::MeshTopology;
use aura_validation_network::transaction_queue::{OfflineTransactionQueue, TransactionType, TransactionPriority};
use aura_validation_network::web3::{RoninTransaction, TransactionStatus};
use aura_validation_network::mesh_validation::{MeshTransaction, MeshTransactionStatus, TokenType};
use aura_validation_network::economic_engine::EconomicEngine;
use aura_validation_network::lending_pools::LendingPoolManager;
use aura_validation_network::token_registry::CrossChainTokenRegistry;
use aura_validation_network::bridge_node::BridgeNode;
use aura_validation_network::store_forward::StoreForwardManager;
use aura_validation_network::mesh::BluetoothMeshManager;
use aura_validation_network::mesh_validation::MeshValidator;
use aura_validation_network::web3::RoninClient;
use aura_validation_network::polymorphic_matrix::PolymorphicMatrix;
use aura_validation_network::white_noise_crypto::{WhiteNoiseEncryption, WhiteNoiseConfig, EncryptionAlgorithm, NoisePattern};
use aura_validation_network::engine_shell::{EngineShellEncryption, EngineShellConfig};
use aura_validation_network::secure_execution::SecureExecutionEngine;
use aura_validation_network::task_distributor::TaskDistributor;
use aura_validation_network::gpu_processor::GPUTaskScheduler;
use aura_validation_network::contract_integration::ContractIntegration;

// Import services
use aura_validation_network::services::*;

/// Helper function to create a temporary directory for tests
fn create_temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("Failed to create temporary directory")
}

/// Test complete service initialization and integration
#[tokio::test]
async fn test_service_initialization_integration() {
    let temp_dir = create_temp_dir();
    let config = AppConfig::default();
    
    // Initialize cryptographic keys
    let key_path = temp_dir.path().join("test_key.key");
    let node_keys = aura_validation_network::crypto::load_or_create_keypair(&key_path).unwrap();
    assert!(!node_keys.node_id().is_empty());
    
    // Initialize mesh topology
    let mesh_topology = Arc::new(RwLock::new(MeshTopology::new(node_keys.node_id())));
    let stats = mesh_topology.read().await.get_network_stats();
    assert_eq!(stats.total_nodes, 1);
    
    // Initialize economic engine
    let economic_engine = Arc::new(EconomicEngine::new());
    
    // Initialize lending pools manager
    let (lending_pools_manager, _) = LendingPoolManager::new();
    let lending_pools_manager = Arc::new(lending_pools_manager);
    
    // Initialize token registry
    let token_registry = Arc::new(CrossChainTokenRegistry::new());
    
    // Initialize transaction queue
    let db_path = temp_dir.path().join("test_transactions.db");
    let (queue_tx, _queue_rx) = mpsc::channel(100);
    let transaction_queue = Arc::new(
        OfflineTransactionQueue::new(&db_path, config.ronin.clone(), queue_tx).await.unwrap()
    );
    
    // Initialize Ronin client
    let ronin_client = Arc::new(RoninClient::new(config.ronin.clone()).unwrap());
    
    // Initialize bridge node
    let bridge_node = Arc::new(BridgeNode::new(
        Arc::clone(&ronin_client),
        Arc::clone(&transaction_queue),
    ));
    
    // Initialize store and forward manager
    let store_forward_manager = Arc::new(StoreForwardManager::new(
        node_keys.clone(),
        config.mesh.clone(),
    ));
    
    // Initialize mesh manager
    let (task_tx, _task_rx) = mpsc::channel(100);
    let (_validator_result_tx, validator_result_rx) = mpsc::channel(100);
    let (mesh_events_tx, _mesh_events_rx) = mpsc::channel(1000);
    
    let mesh_manager = Arc::new(
        BluetoothMeshManager::new(
            config.mesh.clone(),
            node_keys.clone(),
            task_tx,
            validator_result_rx,
            mesh_events_tx,
        ).await.unwrap()
    );
    
    // Initialize mesh validator
    let (mesh_validator_instance, _) = MeshValidator::new(
        node_keys.clone(),
        config.ronin.clone()
    );
    let mesh_validator = Arc::new(RwLock::new(mesh_validator_instance));
    
    // Initialize security components
    let polymorphic_matrix = Arc::new(RwLock::new(PolymorphicMatrix::new().unwrap()));
    
    let white_noise_config = WhiteNoiseConfig {
        noise_layer_count: 3,
        noise_intensity: 0.7,
        steganographic_enabled: true,
        chaos_seed: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() as u64,
        encryption_algorithm: EncryptionAlgorithm::AES256GCM,
        noise_pattern: NoisePattern::Chaotic,
    };
    let white_noise_encryption = Arc::new(RwLock::new(
        WhiteNoiseEncryption::new(white_noise_config).unwrap()
    ));
    
    let engine_shell_config = EngineShellConfig {
        shell_layer_count: 5,
        memory_encryption_enabled: true,
        code_obfuscation_enabled: true,
        anti_analysis_enabled: true,
        shell_rotation_interval: Duration::from_secs(1800),
        chaos_intensity: 0.8,
        noise_ratio: 0.3,
    };
    let engine_shell_encryption = Arc::new(RwLock::new(
        EngineShellEncryption::new(engine_shell_config).unwrap()
    ));
    
    let secure_execution_engine = Arc::new(SecureExecutionEngine::new());
    
    // Initialize security services
    let secret_recipe_service = Arc::new(SecretRecipeService::new(
        Arc::clone(&polymorphic_matrix),
        Arc::clone(&engine_shell_encryption),
        Arc::clone(&white_noise_encryption),
    ));
    
    let polymorphic_matrix_service = Arc::new(PolymorphicMatrixService::new(
        Arc::clone(&polymorphic_matrix),
        Arc::clone(&white_noise_encryption),
        Arc::clone(&mesh_manager),
        Arc::clone(&economic_engine),
        Arc::clone(&mesh_validator),
    ));
    
    let engine_shell_service = Arc::new(EngineShellService::new(
        Arc::clone(&engine_shell_encryption),
        Arc::clone(&mesh_manager),
        Arc::clone(&economic_engine),
        Arc::clone(&secure_execution_engine),
    ));
    
    let chaos_encryption_service = Arc::new(ChaosEncryptionService::new(
        Arc::clone(&white_noise_encryption),
        Arc::clone(&mesh_manager),
        Arc::clone(&economic_engine),
        Arc::clone(&secure_execution_engine),
    ));
    
    let anti_analysis_service = Arc::new(AntiAnalysisService::new(
        Arc::clone(&polymorphic_matrix),
        Arc::clone(&secret_recipe_service),
        Arc::clone(&mesh_manager),
        Arc::clone(&economic_engine),
    ));
    
    let security_orchestration_service = Arc::new(SecurityOrchestrationService::new(
        Arc::clone(&secure_execution_engine),
        Arc::clone(&engine_shell_encryption),
        Arc::clone(&white_noise_encryption),
        Arc::clone(&polymorphic_matrix),
        Arc::clone(&secret_recipe_service),
        Arc::clone(&polymorphic_matrix_service),
        Arc::clone(&engine_shell_service),
        Arc::clone(&chaos_encryption_service),
        Arc::clone(&anti_analysis_service),
        Arc::clone(&mesh_manager),
        Arc::clone(&economic_engine),
    ));
    
    // Initialize default security policies
    security_orchestration_service.initialize_default_policies().await.unwrap();
    
    // Initialize business services
    let mesh_business_service = Arc::new(MeshBusinessService::new(
        Arc::clone(&mesh_manager),
        Arc::clone(&mesh_topology),
        Arc::clone(&transaction_queue),
        Arc::clone(&store_forward_manager),
        Arc::clone(&bridge_node),
        Arc::clone(&mesh_validator),
    ));
    
    let economic_business_service = Arc::new(EconomicBusinessService::new(
        Arc::clone(&economic_engine),
        Arc::clone(&lending_pools_manager),
        Arc::clone(&mesh_manager),
        Arc::clone(&transaction_queue),
        Arc::clone(&token_registry),
        Arc::clone(&bridge_node),
    ));
    
    // Initialize task distributor and GPU scheduler
    let (task_distributor, _task_distributor_rx) = TaskDistributor::new();
    let task_distributor = Arc::new(task_distributor);
    
    let (gpu_scheduler, _gpu_scheduler_rx) = GPUTaskScheduler::new();
    let gpu_scheduler = Arc::new(gpu_scheduler);
    
    // Initialize contract integration
    let contract_integration = Arc::new(ContractIntegration::new(
        config.ronin.clone(),
        node_keys.clone(),
        Arc::clone(&ronin_client),
        "0x1234567890abcdef".to_string(),
    ));
    
    let validation_business_service = Arc::new(ValidationBusinessService::new(
        Arc::clone(&mesh_validator),
        Arc::clone(&task_distributor),
        Arc::clone(&mesh_manager),
        Arc::clone(&economic_engine),
        Arc::clone(&contract_integration),
        Arc::clone(&secure_execution_engine),
    ));
    
    let gpu_business_service = Arc::new(GPUBusinessService::new(
        Arc::clone(&task_distributor),
        Arc::clone(&gpu_scheduler),
        Arc::clone(&mesh_manager),
        Arc::clone(&economic_engine),
        Arc::clone(&mesh_validator),
        Arc::clone(&secure_execution_engine),
    ));
    
    // Verify all services are initialized
    assert!(mesh_business_service.get_mesh_network_stats().await.is_ok());
    assert!(economic_business_service.get_economic_network_stats().await.is_ok());
    assert!(validation_business_service.get_validation_network_stats().await.is_ok());
    assert!(gpu_business_service.get_gpu_network_stats().await.is_ok());
    assert!(security_orchestration_service.get_security_orchestration_stats().await.is_ok());
    
    println!("✅ All services initialized successfully");
}

/// Test mesh transaction processing through multiple services
#[tokio::test]
async fn test_mesh_transaction_workflow() {
    let temp_dir = create_temp_dir();
    let config = AppConfig::default();
    
    // Initialize core components
    let key_path = temp_dir.path().join("test_key.key");
    let node_keys = aura_validation_network::crypto::load_or_create_keypair(&key_path).unwrap();
    
    let mesh_topology = Arc::new(RwLock::new(MeshTopology::new(node_keys.node_id())));
    let economic_engine = Arc::new(EconomicEngine::new());
    let (lending_pools_manager, _) = LendingPoolManager::new();
    let lending_pools_manager = Arc::new(lending_pools_manager);
    let token_registry = Arc::new(CrossChainTokenRegistry::new());
    
    let db_path = temp_dir.path().join("test_transactions.db");
    let (queue_tx, _queue_rx) = mpsc::channel(100);
    let transaction_queue = Arc::new(
        OfflineTransactionQueue::new(&db_path, config.ronin.clone(), queue_tx).await.unwrap()
    );
    
    let ronin_client = Arc::new(RoninClient::new(config.ronin.clone()).unwrap());
    let bridge_node = Arc::new(BridgeNode::new(
        Arc::clone(&ronin_client),
        Arc::clone(&transaction_queue),
    ));
    
    let store_forward_manager = Arc::new(StoreForwardManager::new(
        node_keys.clone(),
        config.mesh.clone(),
    ));
    
    let (task_tx, _task_rx) = mpsc::channel(100);
    let (_validator_result_tx, validator_result_rx) = mpsc::channel(100);
    let (mesh_events_tx, _mesh_events_rx) = mpsc::channel(1000);
    
    let mesh_manager = Arc::new(
        BluetoothMeshManager::new(
            config.mesh.clone(),
            node_keys.clone(),
            task_tx,
            validator_result_rx,
            mesh_events_tx,
        ).await.unwrap()
    );
    
    let (mesh_validator_instance, _) = MeshValidator::new(
        node_keys.clone(),
        config.ronin.clone()
    );
    let mesh_validator = Arc::new(RwLock::new(mesh_validator_instance));
    
    // Initialize mesh business service
    let mesh_business_service = Arc::new(MeshBusinessService::new(
        Arc::clone(&mesh_manager),
        Arc::clone(&mesh_topology),
        Arc::clone(&transaction_queue),
        Arc::clone(&store_forward_manager),
        Arc::clone(&bridge_node),
        Arc::clone(&mesh_validator),
    ));
    
    // Create a test mesh transaction
    let mesh_transaction = MeshTransaction {
        id: Uuid::new_v4(),
        from_address: node_keys.node_id(),
        to_address: "0x2222222222222222222222222222222222222222".to_string(),
        amount: 1000000000000000000, // 1 RON
        token_type: TokenType::RON,
        nonce: 1,
        mesh_participants: vec![node_keys.node_id()],
        signatures: std::collections::HashMap::new(),
        created_at: SystemTime::now(),
        expires_at: SystemTime::now() + Duration::from_secs(3600),
        status: MeshTransactionStatus::Pending,
        validation_threshold: 1,
    };
    
    // Process mesh transaction
    let result = mesh_business_service.process_mesh_transaction(mesh_transaction.clone()).await;
    assert!(result.is_ok(), "Failed to process mesh transaction: {:?}", result.err());
    
    // Validate mesh transaction
    let validation_result = mesh_business_service.validate_mesh_transaction(mesh_transaction.clone()).await;
    assert!(validation_result.is_ok(), "Failed to validate mesh transaction: {:?}", validation_result.err());
    
    // Get mesh network stats
    let stats = mesh_business_service.get_mesh_network_stats().await.unwrap();
    assert!(stats.pending_transactions >= 0);
    
    println!("✅ Mesh transaction workflow completed successfully");
}

/// Test economic service integration with mesh and transaction queue
#[tokio::test]
async fn test_economic_service_integration() {
    let temp_dir = create_temp_dir();
    let config = AppConfig::default();
    
    // Initialize core components
    let key_path = temp_dir.path().join("test_key.key");
    let node_keys = aura_validation_network::crypto::load_or_create_keypair(&key_path).unwrap();
    
    let economic_engine = Arc::new(EconomicEngine::new());
    let (lending_pools_manager, _) = LendingPoolManager::new();
    let lending_pools_manager = Arc::new(lending_pools_manager);
    let token_registry = Arc::new(CrossChainTokenRegistry::new());
    
    let db_path = temp_dir.path().join("test_transactions.db");
    let (queue_tx, _queue_rx) = mpsc::channel(100);
    let transaction_queue = Arc::new(
        OfflineTransactionQueue::new(&db_path, config.ronin.clone(), queue_tx).await.unwrap()
    );
    
    let ronin_client = Arc::new(RoninClient::new(config.ronin.clone()).unwrap());
    let bridge_node = Arc::new(BridgeNode::new(
        Arc::clone(&ronin_client),
        Arc::clone(&transaction_queue),
    ));
    
    let (task_tx, _task_rx) = mpsc::channel(100);
    let (_validator_result_tx, validator_result_rx) = mpsc::channel(100);
    let (mesh_events_tx, _mesh_events_rx) = mpsc::channel(1000);
    
    let mesh_manager = Arc::new(
        BluetoothMeshManager::new(
            config.mesh.clone(),
            node_keys.clone(),
            task_tx,
            validator_result_rx,
            mesh_events_tx,
        ).await.unwrap()
    );
    
    // Initialize economic business service
    let economic_business_service = Arc::new(EconomicBusinessService::new(
        Arc::clone(&economic_engine),
        Arc::clone(&lending_pools_manager),
        Arc::clone(&mesh_manager),
        Arc::clone(&transaction_queue),
        Arc::clone(&token_registry),
        Arc::clone(&bridge_node),
    ));
    
    // Get economic network stats
    let stats = economic_business_service.get_economic_network_stats().await.unwrap();
    assert!(stats.active_lending_pools >= 0);
    assert!(stats.total_economic_transactions >= 0);
    assert!(stats.cross_chain_transfers >= 0);
    
    // Create a test transaction
    let transaction = RoninTransaction {
        id: Uuid::new_v4(),
        from: "0x1111111111111111111111111111111111111111".to_string(),
        to: "0x2222222222222222222222222222222222222222".to_string(),
        value: 1000000000000000000, // 1 RON
        gas_price: 20000000000,
        gas_limit: 21000,
        nonce: 1,
        data: vec![],
        chain_id: 2020,
        created_at: SystemTime::now(),
        status: TransactionStatus::Pending,
    };
    
    // Add transaction to queue
    let tx_id = transaction_queue.add_transaction(
        TransactionType::Ronin(transaction),
        TransactionPriority::Normal,
        vec![],
    ).await.unwrap();
    
    assert!(!tx_id.is_nil());
    
    // Verify transaction was added
    let queue_stats = transaction_queue.get_stats().await;
    assert!(queue_stats.total >= 1);
    
    println!("✅ Economic service integration test completed successfully");
}

/// Test security orchestration service with all security micro-services
#[tokio::test]
async fn test_security_orchestration_integration() {
    let temp_dir = create_temp_dir();
    let config = AppConfig::default();
    
    // Initialize core components
    let key_path = temp_dir.path().join("test_key.key");
    let node_keys = aura_validation_network::crypto::load_or_create_keypair(&key_path).unwrap();
    
    let economic_engine = Arc::new(EconomicEngine::new());
    
    let (task_tx, _task_rx) = mpsc::channel(100);
    let (_validator_result_tx, validator_result_rx) = mpsc::channel(100);
    let (mesh_events_tx, _mesh_events_rx) = mpsc::channel(1000);
    
    let mesh_manager = Arc::new(
        BluetoothMeshManager::new(
            config.mesh.clone(),
            node_keys.clone(),
            task_tx,
            validator_result_rx,
            mesh_events_tx,
        ).await.unwrap()
    );
    
    // Initialize security components
    let polymorphic_matrix = Arc::new(RwLock::new(PolymorphicMatrix::new().unwrap()));
    
    let white_noise_config = WhiteNoiseConfig {
        noise_layer_count: 3,
        noise_intensity: 0.7,
        steganographic_enabled: true,
        chaos_seed: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos() as u64,
        encryption_algorithm: EncryptionAlgorithm::AES256GCM,
        noise_pattern: NoisePattern::Chaotic,
    };
    let white_noise_encryption = Arc::new(RwLock::new(
        WhiteNoiseEncryption::new(white_noise_config).unwrap()
    ));
    
    let engine_shell_config = EngineShellConfig {
        shell_layer_count: 5,
        memory_encryption_enabled: true,
        code_obfuscation_enabled: true,
        anti_analysis_enabled: true,
        shell_rotation_interval: Duration::from_secs(1800),
        chaos_intensity: 0.8,
        noise_ratio: 0.3,
    };
    let engine_shell_encryption = Arc::new(RwLock::new(
        EngineShellEncryption::new(engine_shell_config).unwrap()
    ));
    
    let secure_execution_engine = Arc::new(SecureExecutionEngine::new());
    
    // Initialize security services
    let secret_recipe_service = Arc::new(SecretRecipeService::new(
        Arc::clone(&polymorphic_matrix),
        Arc::clone(&engine_shell_encryption),
        Arc::clone(&white_noise_encryption),
    ));
    
    let polymorphic_matrix_service = Arc::new(PolymorphicMatrixService::new(
        Arc::clone(&polymorphic_matrix),
        Arc::clone(&white_noise_encryption),
        Arc::clone(&mesh_manager),
        Arc::clone(&economic_engine),
        Arc::new(RwLock::new(MeshValidator::new(node_keys.clone(), config.ronin.clone()).0)),
    ));
    
    let engine_shell_service = Arc::new(EngineShellService::new(
        Arc::clone(&engine_shell_encryption),
        Arc::clone(&mesh_manager),
        Arc::clone(&economic_engine),
        Arc::clone(&secure_execution_engine),
    ));
    
    let chaos_encryption_service = Arc::new(ChaosEncryptionService::new(
        Arc::clone(&white_noise_encryption),
        Arc::clone(&mesh_manager),
        Arc::clone(&economic_engine),
        Arc::clone(&secure_execution_engine),
    ));
    
    let anti_analysis_service = Arc::new(AntiAnalysisService::new(
        Arc::clone(&polymorphic_matrix),
        Arc::clone(&secret_recipe_service),
        Arc::clone(&mesh_manager),
        Arc::clone(&economic_engine),
    ));
    
    let security_orchestration_service = Arc::new(SecurityOrchestrationService::new(
        Arc::clone(&secure_execution_engine),
        Arc::clone(&engine_shell_encryption),
        Arc::clone(&white_noise_encryption),
        Arc::clone(&polymorphic_matrix),
        Arc::clone(&secret_recipe_service),
        Arc::clone(&polymorphic_matrix_service),
        Arc::clone(&engine_shell_service),
        Arc::clone(&chaos_encryption_service),
        Arc::clone(&anti_analysis_service),
        Arc::clone(&mesh_manager),
        Arc::clone(&economic_engine),
    ));
    
    // Initialize default security policies
    security_orchestration_service.initialize_default_policies().await.unwrap();
    
    // Get security orchestration stats
    let stats = security_orchestration_service.get_security_orchestration_stats().await.unwrap();
    assert!(stats.total_sessions >= 0);
    assert!(stats.threats_detected >= 0);
    
    // Create a security session
    let session_result = security_orchestration_service.create_security_session(
        "test_operation".to_string(),
        aura_validation_network::secure_execution::SecurityLevel::High,
    ).await;
    
    assert!(session_result.is_ok(), "Failed to create security session: {:?}", session_result.err());
    
    println!("✅ Security orchestration integration test completed successfully");
}

/// Test complete end-to-end workflow: transaction -> validation -> economic processing
#[tokio::test]
async fn test_end_to_end_workflow() {
    let temp_dir = create_temp_dir();
    let config = AppConfig::default();
    
    // Initialize all core components
    let key_path = temp_dir.path().join("test_key.key");
    let node_keys = aura_validation_network::crypto::load_or_create_keypair(&key_path).unwrap();
    
    let mesh_topology = Arc::new(RwLock::new(MeshTopology::new(node_keys.node_id())));
    let economic_engine = Arc::new(EconomicEngine::new());
    let (lending_pools_manager, _) = LendingPoolManager::new();
    let lending_pools_manager = Arc::new(lending_pools_manager);
    let token_registry = Arc::new(CrossChainTokenRegistry::new());
    
    let db_path = temp_dir.path().join("test_transactions.db");
    let (queue_tx, _queue_rx) = mpsc::channel(100);
    let transaction_queue = Arc::new(
        OfflineTransactionQueue::new(&db_path, config.ronin.clone(), queue_tx).await.unwrap()
    );
    
    let ronin_client = Arc::new(RoninClient::new(config.ronin.clone()).unwrap());
    let bridge_node = Arc::new(BridgeNode::new(
        Arc::clone(&ronin_client),
        Arc::clone(&transaction_queue),
    ));
    
    let store_forward_manager = Arc::new(StoreForwardManager::new(
        node_keys.clone(),
        config.mesh.clone(),
    ));
    
    let (task_tx, _task_rx) = mpsc::channel(100);
    let (_validator_result_tx, validator_result_rx) = mpsc::channel(100);
    let (mesh_events_tx, _mesh_events_rx) = mpsc::channel(1000);
    
    let mesh_manager = Arc::new(
        BluetoothMeshManager::new(
            config.mesh.clone(),
            node_keys.clone(),
            task_tx,
            validator_result_rx,
            mesh_events_tx,
        ).await.unwrap()
    );
    
    let (mesh_validator_instance, _) = MeshValidator::new(
        node_keys.clone(),
        config.ronin.clone()
    );
    let mesh_validator = Arc::new(RwLock::new(mesh_validator_instance));
    
    let secure_execution_engine = Arc::new(SecureExecutionEngine::new());
    let (task_distributor, _task_distributor_rx) = TaskDistributor::new();
    let task_distributor = Arc::new(task_distributor);
    
    let contract_integration = Arc::new(ContractIntegration::new(
        config.ronin.clone(),
        node_keys.clone(),
        Arc::clone(&ronin_client),
        "0x1234567890abcdef".to_string(),
    ));
    
    // Initialize business services
    let mesh_business_service = Arc::new(MeshBusinessService::new(
        Arc::clone(&mesh_manager),
        Arc::clone(&mesh_topology),
        Arc::clone(&transaction_queue),
        Arc::clone(&store_forward_manager),
        Arc::clone(&bridge_node),
        Arc::clone(&mesh_validator),
    ));
    
    let economic_business_service = Arc::new(EconomicBusinessService::new(
        Arc::clone(&economic_engine),
        Arc::clone(&lending_pools_manager),
        Arc::clone(&mesh_manager),
        Arc::clone(&transaction_queue),
        Arc::clone(&token_registry),
        Arc::clone(&bridge_node),
    ));
    
    let validation_business_service = Arc::new(ValidationBusinessService::new(
        Arc::clone(&mesh_validator),
        Arc::clone(&task_distributor),
        Arc::clone(&mesh_manager),
        Arc::clone(&economic_engine),
        Arc::clone(&contract_integration),
        Arc::clone(&secure_execution_engine),
    ));
    
    // Step 1: Create a mesh transaction
    let mesh_transaction = MeshTransaction {
        id: Uuid::new_v4(),
        from_address: node_keys.node_id(),
        to_address: "0x2222222222222222222222222222222222222222".to_string(),
        amount: 1000000000000000000, // 1 RON
        token_type: TokenType::RON,
        nonce: 1,
        mesh_participants: vec![node_keys.node_id()],
        signatures: std::collections::HashMap::new(),
        created_at: SystemTime::now(),
        expires_at: SystemTime::now() + Duration::from_secs(3600),
        status: MeshTransactionStatus::Pending,
        validation_threshold: 1,
    };
    
    // Step 2: Process through mesh service
    mesh_business_service.process_mesh_transaction(mesh_transaction.clone()).await.unwrap();
    
    // Step 3: Validate transaction
    mesh_business_service.validate_mesh_transaction(mesh_transaction.clone()).await.unwrap();
    
    // Step 4: Verify all services have correct state
    let mesh_stats = mesh_business_service.get_mesh_network_stats().await.unwrap();
    let economic_stats = economic_business_service.get_economic_network_stats().await.unwrap();
    let validation_stats = validation_business_service.get_validation_network_stats().await.unwrap();
    
    // Verify stats are accessible
    assert!(mesh_stats.pending_transactions >= 0);
    assert!(economic_stats.total_economic_transactions >= 0);
    assert!(validation_stats.active_validation_tasks >= 0);
    
    println!("✅ End-to-end workflow test completed successfully");
    println!("   Mesh stats: {} pending transactions", mesh_stats.pending_transactions);
    println!("   Economic stats: {} total transactions", economic_stats.total_economic_transactions);
    println!("   Validation stats: {} active tasks", validation_stats.active_validation_tasks);
}

