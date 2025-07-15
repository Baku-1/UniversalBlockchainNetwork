// tests/integration_tests.rs

use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

// Import modules from the main crate
use aura_validation_network::config::{AppConfig, MeshConfig, RoninConfig, GameConfig};
use aura_validation_network::crypto::NodeKeypair;
use aura_validation_network::transaction_queue::{OfflineTransactionQueue, TransactionType, TransactionPriority};
use aura_validation_network::web3::{RoninTransaction, TransactionStatus};
use aura_validation_network::mesh_topology::MeshTopology;

#[tokio::test]
async fn test_config_loading() {
    // Test configuration loading with defaults
    let config = AppConfig::default();
    
    assert_eq!(config.ipc_port, 9898);
    assert_eq!(config.p2p_port, 4001);
    assert_eq!(config.mesh.max_peers, 8);
    assert_eq!(config.ronin.chain_id, 2020);
    assert_eq!(config.game.max_players, 4);
}

#[tokio::test]
async fn test_node_keypair_generation() {
    // Test keypair generation and node ID
    let temp_dir = tempfile::tempdir().unwrap();
    let key_path = temp_dir.path().join("test_key.key");
    
    let keypair = aura_validation_network::crypto::load_or_create_keypair(&key_path).unwrap();
    let node_id = keypair.node_id();
    
    assert!(!node_id.is_empty());
    assert_eq!(node_id.len(), 64); // 32 bytes as hex string
    
    // Test loading existing keypair
    let keypair2 = aura_validation_network::crypto::load_or_create_keypair(&key_path).unwrap();
    assert_eq!(keypair.node_id(), keypair2.node_id());
}

// Game state management test removed - game components not implemented in current architecture

#[tokio::test]
async fn test_offline_transaction_queue() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_transactions.db");
    
    let config = RoninConfig::default();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    
    let queue = OfflineTransactionQueue::new(&db_path, config, tx).await.unwrap();
    
    // Test adding a transaction
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
        created_at: std::time::SystemTime::now(),
        status: TransactionStatus::Pending,
    };
    
    let tx_id = queue.add_transaction(
        TransactionType::Ronin(transaction),
        TransactionPriority::Normal,
        vec![],
    ).await.unwrap();
    
    // Check that event was sent
    let event = timeout(Duration::from_secs(1), rx.recv()).await.unwrap().unwrap();
    match event {
        aura_validation_network::transaction_queue::QueueEvent::TransactionAdded(id) => {
            assert_eq!(id, tx_id);
        }
        _ => panic!("Expected TransactionAdded event"),
    }
    
    // Test queue statistics before getting transaction
    let stats = queue.get_stats().await;
    assert_eq!(stats.total, 1);
    assert_eq!(stats.queued, 1);

    // Test getting next transaction
    let next_tx = queue.get_next_transaction().await;
    assert!(next_tx.is_some());

    // Test queue statistics after getting transaction (transaction is removed from queue)
    let stats_after = queue.get_stats().await;
    assert_eq!(stats_after.total, 0); // Total count decreases because transaction is removed
    assert_eq!(stats_after.queued, 0); // Queued count also decreases
}

#[tokio::test]
async fn test_mesh_topology() {
    let local_node_id = "node1".to_string();
    let mut topology = MeshTopology::new(local_node_id.clone());
    
    // Add nodes to the topology
    let node2_info = aura_validation_network::mesh_topology::NodeInfo {
        node_id: "node2".to_string(),
        last_seen: std::time::SystemTime::now(),
        hop_count: 1,
        connection_quality: 0.9,
        capabilities: vec!["game_sync".to_string()],
    };

    let node3_info = aura_validation_network::mesh_topology::NodeInfo {
        node_id: "node3".to_string(),
        last_seen: std::time::SystemTime::now(),
        hop_count: 2,
        connection_quality: 0.8,
        capabilities: vec!["transaction_relay".to_string()],
    };
    
    topology.add_node(node2_info);
    topology.add_node(node3_info);
    
    // Add connections
    topology.add_connection(&local_node_id, "node2");
    topology.add_connection("node2", "node3");
    
    // Test routing
    topology.rebuild_routing_table();
    let next_hop = topology.get_next_hop("node3");
    assert_eq!(next_hop, Some(&"node2".to_string()));
    
    // Test path finding
    let path = topology.find_shortest_path("node3");
    assert!(path.is_some());
    let path = path.unwrap();
    assert_eq!(path, vec!["node1", "node2", "node3"]);
    
    // Test network statistics
    let stats = topology.get_network_stats();
    assert_eq!(stats.total_nodes, 3);
    assert_eq!(stats.total_connections, 2);
}

#[tokio::test]
async fn test_web3_utilities() {
    // Test address validation
    assert!(aura_validation_network::web3::utils::is_valid_address("0x1234567890123456789012345678901234567890"));
    assert!(!aura_validation_network::web3::utils::is_valid_address("invalid_address"));
    assert!(!aura_validation_network::web3::utils::is_valid_address("0x123")); // Too short

    // Test currency conversion
    let wei_amount = aura_validation_network::web3::utils::ron_to_wei(1.0);
    assert_eq!(wei_amount, 1_000_000_000_000_000_000);

    let ron_amount = aura_validation_network::web3::utils::wei_to_ron(1_000_000_000_000_000_000);
    assert_eq!(ron_amount, 1.0);

    // Test transaction creation
    let tx = aura_validation_network::web3::utils::create_ron_transfer(
        "0x1111111111111111111111111111111111111111".to_string(),
        "0x2222222222222222222222222222222222222222".to_string(),
        1_000_000_000_000_000_000, // 1 RON
        1,
        20_000_000_000,
        2020,
    );
    
    assert_eq!(tx.value, 1_000_000_000_000_000_000);
    assert_eq!(tx.chain_id, 2020);
    assert_eq!(tx.gas_limit, 21000);
}

// Conflict resolution test removed - conflict resolution module not implemented in current architecture

#[tokio::test]
async fn test_error_handling() {
    use aura_validation_network::errors::{NexusError, ErrorContext, ErrorContextExt};

    // Test error creation
    let error = NexusError::PeerNotFound {
        peer_id: "test_peer".to_string(),
    };

    let context = ErrorContext::new("test_operation", "test_component")
        .with_node_id("test_node".to_string());

    // Test error with context
    let result: Result<(), NexusError> = Err(error);
    let contextual_result = result.with_context(context);

    assert!(contextual_result.is_err());

    // Test error recovery
    let network_error = NexusError::NetworkConnection("Connection failed".to_string());
    assert!(aura_validation_network::errors::utils::is_recoverable_error(&network_error));

    let validation_error = NexusError::TransactionValidation { tx_id: Uuid::new_v4() };
    assert!(!aura_validation_network::errors::utils::is_recoverable_error(&validation_error));
}

// Helper function to create a temporary directory for tests
fn create_temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("Failed to create temporary directory")
}

// Integration test for the complete system
#[tokio::test]
async fn test_system_integration() {
    // This test verifies that all components can be initialized together
    let config = AppConfig::default();
    
    // Test that we can create all major components without errors
    let temp_dir = create_temp_dir();
    let key_path = temp_dir.path().join("integration_test.key");
    
    let node_keys = aura_validation_network::crypto::load_or_create_keypair(&key_path).unwrap();
    assert!(!node_keys.node_id().is_empty());
    
    let topology = MeshTopology::new(node_keys.node_id());
    let stats = topology.get_network_stats();
    assert_eq!(stats.total_nodes, 1); // Just the local node
    
    // Test transaction queue initialization
    let db_path = temp_dir.path().join("integration_transactions.db");
    let (tx, _rx) = tokio::sync::mpsc::channel(10);
    let _queue = OfflineTransactionQueue::new(&db_path, config.ronin, tx).await.unwrap();
    
    // If we get here without panicking, the integration is successful
    assert!(true);
}
