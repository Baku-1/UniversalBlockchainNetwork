// tests/mesh_integration.rs

use std::time::SystemTime;
use tokio::sync::mpsc;

use aura_validation_network::mesh::{BluetoothMeshManager, MeshMessage, MeshMessageType, MeshEvent};
use aura_validation_network::validator::{ComputationTask, TaskResult};
use aura_validation_network::config::AppConfig;
use aura_validation_network::crypto::NodeKeypair;

#[tokio::test]
async fn test_mesh_message_roundtrip_non_ble() {
    // Prepare config and channels
    let config = AppConfig::default().mesh;
    let keypair = NodeKeypair::from_bytes(&[0u8; 32]).unwrap();
    let (to_validator_tx, _to_validator_rx) = mpsc::channel::<ComputationTask>(8);
    let (_from_validator_tx, from_validator_rx) = mpsc::channel::<TaskResult>(8);
    let (events_tx, mut events_rx) = mpsc::channel::<MeshEvent>(16);

    // Create manager in simulation mode (likely no adapter)
    let manager = BluetoothMeshManager::new(
        config,
        keypair,
        to_validator_tx,
        from_validator_rx,
        events_tx,
    ).await.expect("manager");

    // Start (spawns background consumers)
    manager.start().await.expect("start");

    // Send a message to ourselves via handle_direct_message
    let msg = MeshMessage {
        id: uuid::Uuid::new_v4(),
        sender_id: "self".to_string(),
        target_id: Some(manager.get_mesh_router().local_node_id().to_string()),
        message_type: MeshMessageType::Heartbeat,
        payload: vec![],
        ttl: 3,
        hop_count: 0,
        timestamp: SystemTime::now(),
        signature: vec![1u8; 64],
    };

    // Process inbound as if received over BLE
    manager.process_message(msg).await.expect("process");

    // Drain a few events to confirm no panic and path exercised
    let _ = tokio::time::timeout(std::time::Duration::from_millis(200), events_rx.recv()).await;
}






