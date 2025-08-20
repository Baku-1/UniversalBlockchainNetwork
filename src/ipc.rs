// src/ipc.rs

use futures_util::{StreamExt, SinkExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use std::time::Instant;
use serde::{Deserialize, Serialize};
use anyhow::Result;

/// Enhanced engine status for the UI
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EngineStatus {
    pub is_connected: bool,
    pub is_running: bool,
    pub mode: String,
    pub mesh_mode: bool,
    pub node_id: String,

    // Mesh networking
    pub mesh_peers: Vec<MeshPeerInfo>,
    pub total_peers: usize,
    pub connected_peers: usize,

    // Web3 Users state
    pub validation_session: Option<String>,
    pub web3_users: Vec<Web3UserInfo>,
    pub total_users: usize,

    // Blockchain/Web3
    pub ronin_connected: bool,
    pub ronin_block_number: Option<u64>,
    pub ronin_gas_price: Option<u64>,
    pub transaction_queue: usize,
    pub pending_transactions: usize,
    pub last_sync_time: Option<String>,

    // Validation
    pub last_task: String,
    pub tasks_processed: u64,
    pub validation_rate: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MeshPeerInfo {
    pub id: String,
    pub is_connected: bool,
    pub connection_quality: f32,
    pub last_seen: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Web3UserInfo {
    pub id: String,
    pub is_online: bool,
    pub address: String,
    pub last_action: String,
}

/// IPC message types for communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcMessage {
    /// Ping message for connectivity testing
    Ping {
        timestamp: u64,
        sequence: u64,
    },
    /// Pong response to ping
    Pong {
        timestamp: u64,
        sequence: u64,
        latency_ms: u64,
    },
    /// Request for engine status
    StatusRequest,
    /// Engine status response
    StatusResponse {
        status: EngineStatus,
    },
    /// Command to execute
    Command {
        command: String,
        params: Option<serde_json::Value>,
    },
    /// Command response
    CommandResponse {
        success: bool,
        message: Option<String>,
        data: Option<serde_json::Value>,
    },
    /// Error message
    Error {
        message: String,
        code: Option<u32>,
    },
}

/// Enhanced IPC events for full client communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnhancedIpcEvent {
    /// Client connected to enhanced IPC
    ClientConnected(String),
    /// Client disconnected from enhanced IPC
    ClientDisconnected(String),
    /// Command received from client
    CommandReceived(IpcMessage),
}

/// Connection statistics for monitoring
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionStats {
    pub connected_clients: usize,
    pub total_messages_sent: u64,
    pub total_messages_received: u64,
    pub average_latency_ms: f64,
    pub last_ping_time: Option<String>,
    pub uptime_seconds: u64,
}

/// IPC server state
pub struct IpcServer {
    pub stats: Arc<RwLock<ConnectionStats>>,
    pub start_time: Instant,
}

impl Default for EngineStatus {
    fn default() -> Self {
        Self {
            is_connected: true,
            is_running: true,
            mode: "Ready".to_string(),
            mesh_mode: false,
            node_id: "nexus_node_001".to_string(),
            mesh_peers: vec![
                // Demo data for visualization
                MeshPeerInfo {
                    id: "peer_001".to_string(),
                    is_connected: true,
                    connection_quality: 0.95,
                    last_seen: "2024-01-01T00:00:00Z".to_string(),
                },
                MeshPeerInfo {
                    id: "peer_002".to_string(),
                    is_connected: false,
                    connection_quality: 0.75,
                    last_seen: "2024-01-01T00:00:00Z".to_string(),
                },
            ],
            total_peers: 2,
            connected_peers: 1,
            validation_session: Some("validation_001".to_string()),
            web3_users: vec![
                Web3UserInfo {
                    id: "user_001".to_string(),
                    is_online: true,
                    address: "0x1234567890123456789012345678901234567890".to_string(),
                    last_action: "Validation".to_string(),
                },
            ],
            total_users: 1,
            ronin_connected: false,
            ronin_block_number: None,
            ronin_gas_price: None,
            transaction_queue: 5,
            pending_transactions: 3,
            last_sync_time: None,
            last_task: "Block Validation".to_string(),
            tasks_processed: 42,
            validation_rate: 15.5,
        }
    }
}

/// Global engine status
static ENGINE_STATUS: once_cell::sync::Lazy<Arc<RwLock<EngineStatus>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(EngineStatus::default())));

// Old basic IPC server removed - replaced by enhanced version below

// Old basic handle_connection removed - replaced by enhanced version below

async fn handle_command(command: serde_json::Value) -> String {
    let cmd = command.get("command").and_then(|c| c.as_str()).unwrap_or("unknown");

    match cmd {
        "GetStatus" | "GetFullStatus" => {
            let status = ENGINE_STATUS.read().await.clone();
            json!({
                "type": "status",
                "data": status
            }).to_string()
        }
        "ResumeEngine" => {
            tracing::info!("Engine resume requested via IPC");
            {
                let mut status = ENGINE_STATUS.write().await;
                status.is_running = true;
            }
            json!({"type": "command_response", "success": true}).to_string()
        }
        "PauseEngine" => {
            tracing::info!("Engine pause requested via IPC");
            {
                let mut status = ENGINE_STATUS.write().await;
                status.is_running = false;
            }
            json!({"type": "command_response", "success": true}).to_string()
        }
        "EnableMeshMode" => {
            tracing::info!("Mesh mode enable requested via IPC");
            {
                let mut status = ENGINE_STATUS.write().await;
                status.mesh_mode = true;
                status.mode = "Mesh".to_string();
            }
            json!({"type": "command_response", "success": true}).to_string()
        }
        "DisableMeshMode" => {
            tracing::info!("Mesh mode disable requested via IPC");
            {
                let mut status = ENGINE_STATUS.write().await;
                status.mesh_mode = false;
                status.mode = "P2P".to_string();
            }
            json!({"type": "command_response", "success": true}).to_string()
        }
        "ForceSync" => {
            tracing::info!("Force sync requested via IPC");
            {
                let mut status = ENGINE_STATUS.write().await;
                status.last_sync_time = Some(chrono::Utc::now().to_rfc3339());
            }
            json!({"type": "command_response", "success": true, "message": "Sync initiated"}).to_string()
        }
        "AddWeb3User" => {
            tracing::info!("Adding Web3 user via IPC");
            if let Some(user_data) = command.get("user") {
                let mut status = ENGINE_STATUS.write().await;

                // Create new Web3 user
                let user = Web3UserInfo {
                    id: user_data.get("id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                    address: user_data.get("wallet").and_then(|v| v.as_str()).unwrap_or("0x0000000000000000000000000000000000000000").to_string(),
                    is_online: true,
                    last_action: format!("Joined mesh ({})", user_data.get("type").and_then(|v| v.as_str()).unwrap_or("unknown")),
                };

                // Add to users list
                status.web3_users.push(user);
                status.total_users = status.web3_users.len();

                tracing::info!("Added Web3 user, total users: {}", status.total_users);
            }
            json!({"type": "command_response", "success": true}).to_string()
        }
        "StartValidation" => {
            tracing::info!("Starting validation session via IPC");
            if let Some(task_data) = command.get("task") {
                let mut status = ENGINE_STATUS.write().await;
                status.last_task = task_data.get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Validation Task")
                    .to_string();
                status.tasks_processed += 1;
                status.validation_rate = 95.5; // Simulate high validation rate
            }
            json!({"type": "command_response", "success": true}).to_string()
        }
        "QueueTransaction" => {
            tracing::info!("Queuing transaction via IPC");
            {
                let mut status = ENGINE_STATUS.write().await;
                status.transaction_queue += 1;
                status.pending_transactions += 1;
            }
            json!({"type": "command_response", "success": true}).to_string()
        }
        "ProcessTransaction" => {
            tracing::info!("Processing transaction via IPC");
            {
                let mut status = ENGINE_STATUS.write().await;
                if status.pending_transactions > 0 {
                    status.pending_transactions -= 1;
                }
                if status.transaction_queue > 0 {
                    status.transaction_queue -= 1;
                }
                status.last_sync_time = Some(chrono::Utc::now().to_rfc3339());
            }
            json!({"type": "command_response", "success": true}).to_string()
        }
        "GetRoninInfo" => {
            tracing::info!("Ronin network info requested via IPC");
            // This would be called with a reference to the RoninClient
            // For now, return current status
            let status = ENGINE_STATUS.read().await.clone();
            json!({
                "type": "ronin_info",
                "data": {
                    "connected": status.ronin_connected,
                    "block_number": status.ronin_block_number,
                    "gas_price": status.ronin_gas_price,
                    "chain_id": 2020,
                    "rpc_url": "https://api.roninchain.com/rpc"
                }
            }).to_string()
        }
        "EnableIdleValidation" => {
            tracing::info!("Idle validation enabled via IPC - user device is idle");
            {
                let mut status = ENGINE_STATUS.write().await;
                status.is_running = true;
                status.mode = "Idle Validation".to_string();
                status.validation_rate = 25.0; // Higher rate during idle
            }
            json!({"type": "command_response", "success": true, "message": "Idle validation enabled"}).to_string()
        }
        "DisableIdleValidation" => {
            tracing::info!("Idle validation disabled via IPC - user is active");
            {
                let mut status = ENGINE_STATUS.write().await;
                status.mode = if status.mesh_mode { "Mesh" } else { "Ready" }.to_string();
                status.validation_rate = 15.5; // Normal rate when user is active
            }
            json!({"type": "command_response", "success": true, "message": "Idle validation disabled"}).to_string()
        }
        "RequestPermission" => {
            tracing::info!("Permission request initiated via IPC");
            json!({
                "type": "permission_request",
                "data": {
                    "title": "Aura Validation Network - Permission Request",
                    "description": "Allow this device to participate in Ronin blockchain validation and earn rewards?",
                    "benefits": [
                        "Earn $RON tokens for contributing idle resources",
                        "Support offline Web3 gaming infrastructure",
                        "Help secure the Ronin blockchain network",
                        "Enable mesh networking for better connectivity"
                    ],
                    "privacy_guarantees": [
                        "No access to personal files or browsing history",
                        "Only uses idle system resources",
                        "Secure, sandboxed execution environment",
                        "Full user control with instant disable option"
                    ]
                }
            }).to_string()
        }
        "EnableBluetoothMesh" => {
            tracing::info!("Bluetooth mesh networking enabled via IPC");
            {
                let mut status = ENGINE_STATUS.write().await;
                status.mesh_mode = true;
                status.mode = "Bluetooth Mesh".to_string();
                // Simulate discovering nearby devices
                status.mesh_peers = vec![
                    MeshPeerInfo {
                        id: "mobile_001".to_string(),
                        is_connected: true,
                        connection_quality: 0.85,
                        last_seen: chrono::Utc::now().to_rfc3339(),
                    },
                    MeshPeerInfo {
                        id: "mobile_002".to_string(),
                        is_connected: true,
                        connection_quality: 0.92,
                        last_seen: chrono::Utc::now().to_rfc3339(),
                    },
                ];
                status.connected_peers = status.mesh_peers.len();
                status.total_peers = status.mesh_peers.len();
            }
            json!({"type": "command_response", "success": true, "message": "Bluetooth mesh enabled"}).to_string()
        }
        "DisableBluetoothMesh" => {
            tracing::info!("Bluetooth mesh networking disabled via IPC");
            {
                let mut status = ENGINE_STATUS.write().await;
                status.mesh_mode = false;
                status.mode = "WiFi Only".to_string();
                status.mesh_peers.clear();
                status.connected_peers = 0;
                status.total_peers = 0;
            }
            json!({"type": "command_response", "success": true, "message": "Bluetooth mesh disabled"}).to_string()
        }
        "ScanNearbyDevices" => {
            tracing::info!("Scanning for nearby Bluetooth devices via IPC");
            // Simulate device discovery
            let nearby_devices = vec![
                json!({
                    "id": "device_001",
                    "name": "Gaming Phone",
                    "type": "mobile",
                    "signal_strength": -45,
                    "aura_enabled": true,
                    "connected": false
                }),
                json!({
                    "id": "device_002",
                    "name": "Tablet Pro",
                    "type": "tablet",
                    "signal_strength": -52,
                    "aura_enabled": true,
                    "connected": false
                }),
                json!({
                    "id": "device_003",
                    "name": "Laptop",
                    "type": "desktop",
                    "signal_strength": -38,
                    "aura_enabled": true,
                    "connected": false
                })
            ];

            json!({
                "type": "nearby_devices",
                "data": {
                    "devices": nearby_devices,
                    "scan_complete": true,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            }).to_string()
        }
        "ConnectToDevice" => {
            tracing::info!("Connecting to Bluetooth device via IPC");
            if let Some(device_id) = command.get("device_id").and_then(|v| v.as_str()) {
                let mut status = ENGINE_STATUS.write().await;

                // Add new peer to mesh
                let new_peer = MeshPeerInfo {
                    id: device_id.to_string(),
                    is_connected: true,
                    connection_quality: 0.88,
                    last_seen: chrono::Utc::now().to_rfc3339(),
                };

                status.mesh_peers.push(new_peer);
                status.connected_peers = status.mesh_peers.len();
                status.total_peers = status.mesh_peers.len();

                json!({
                    "type": "device_connected",
                    "data": {
                        "device_id": device_id,
                        "success": true,
                        "message": "Device connected to mesh"
                    }
                }).to_string()
            } else {
                json!({"type": "error", "message": "Device ID required"}).to_string()
            }
        }
        "GetMeshStatus" => {
            tracing::info!("Mesh network status requested via IPC");
            let status = ENGINE_STATUS.read().await.clone();
            json!({
                "type": "mesh_status",
                "data": {
                    "mesh_enabled": status.mesh_mode,
                    "connected_peers": status.connected_peers,
                    "total_peers": status.total_peers,
                    "mesh_peers": status.mesh_peers,
                    "network_quality": if status.connected_peers > 0 {
                        status.mesh_peers.iter().map(|p| p.connection_quality).sum::<f32>() / status.mesh_peers.len() as f32
                    } else { 0.0 },
                    "is_mobile": true, // This would be detected from user agent
                    "bluetooth_available": true
                }
            }).to_string()
        }
        _ => {
            json!({"type": "error", "message": "Unknown command"}).to_string()
        }
    }
}

/// Enhanced IPC server with ping/pong functionality
impl IpcServer {
    /// Create a new IPC server
    pub fn new() -> Self {
        Self {
            stats: Arc::new(RwLock::new(ConnectionStats {
                connected_clients: 0,
                total_messages_sent: 0,
                total_messages_received: 0,
                average_latency_ms: 0.0,
                last_ping_time: None,
                uptime_seconds: 0,
            })),
            start_time: Instant::now(),
        }
    }

    /// Handle ping message and return pong
    pub async fn handle_ping(&self, timestamp: u64, sequence: u64) -> IpcMessage {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let latency_ms = now.saturating_sub(timestamp);

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.total_messages_received += 1;
            stats.last_ping_time = Some(chrono::Utc::now().to_rfc3339());

            // Update average latency (simple moving average)
            if stats.total_messages_received > 1 {
                stats.average_latency_ms = (stats.average_latency_ms + latency_ms as f64) / 2.0;
            } else {
                stats.average_latency_ms = latency_ms as f64;
            }
        }

        IpcMessage::Pong {
            timestamp: now,
            sequence,
            latency_ms,
        }
    }

    /// Process IPC message and return response
    pub async fn process_message(&self, message: IpcMessage) -> Option<IpcMessage> {
        match message {
            IpcMessage::Ping { timestamp, sequence } => {
                Some(self.handle_ping(timestamp, sequence).await)
            }
            IpcMessage::StatusRequest => {
                let status = ENGINE_STATUS.read().await.clone();
                Some(IpcMessage::StatusResponse { status })
            }
            IpcMessage::Command { command, params } => {
                let response = handle_command_enhanced(&command, params).await;
                Some(response)
            }
            _ => None, // Other message types handled elsewhere
        }
    }

    /// Get connection statistics
    pub async fn get_stats(&self) -> ConnectionStats {
        let mut stats = self.stats.read().await.clone();
        stats.uptime_seconds = self.start_time.elapsed().as_secs();
        stats
    }

    /// Update client connection count
    pub async fn update_client_count(&self, delta: i32) {
        let mut stats = self.stats.write().await;
        if delta > 0 {
            stats.connected_clients += delta as usize;
        } else {
            stats.connected_clients = stats.connected_clients.saturating_sub((-delta) as usize);
        }
    }

    /// Process command from enhanced IPC
    pub async fn process_command(&self, command: IpcMessage) -> Result<(), Box<dyn std::error::Error>> {
        match command {
            IpcMessage::Command { command, params } => {
                let response = handle_command_enhanced(&command, params).await;
                tracing::debug!("Processed enhanced IPC command: {:?}", response);
                Ok(())
            }
            _ => {
                tracing::warn!("Unknown command type in enhanced IPC: {:?}", command);
                Ok(())
            }
        }
    }
}

/// Command handler with comprehensive functionality
async fn handle_command_enhanced(command: &str, params: Option<serde_json::Value>) -> IpcMessage {
    match command {
        "ping" => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            IpcMessage::Pong {
                timestamp,
                sequence: 0,
                latency_ms: 0,
            }
        }
        "status" | "GetStatus" => {
            let status = ENGINE_STATUS.read().await.clone();
            IpcMessage::StatusResponse { status }
        }
        "ResumeEngine" => {
            {
                let mut status = ENGINE_STATUS.write().await;
                status.is_running = true;
            }
            IpcMessage::CommandResponse {
                success: true,
                message: Some("Engine resumed".to_string()),
                data: None,
            }
        }
        "PauseEngine" => {
            {
                let mut status = ENGINE_STATUS.write().await;
                status.is_running = false;
            }
            IpcMessage::CommandResponse {
                success: true,
                message: Some("Engine paused".to_string()),
                data: None,
            }
        }
        _ => {
            // For all other commands, use the comprehensive handler and convert response
            let command_json = if let Some(params) = params {
                json!({
                    "command": command,
                    "params": params
                })
            } else {
                json!({
                    "command": command
                })
            };

            let response_string = handle_command(command_json).await;

            // Convert string response to IpcMessage
            if let Ok(response_json) = serde_json::from_str::<serde_json::Value>(&response_string) {
                if response_json.get("type") == Some(&json!("command_response")) {
                    IpcMessage::CommandResponse {
                        success: response_json.get("success").and_then(|v| v.as_bool()).unwrap_or(false),
                        message: response_json.get("message").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        data: response_json.get("data").cloned(),
                    }
                } else {
                    IpcMessage::CommandResponse {
                        success: true,
                        message: Some("Command processed".to_string()),
                        data: Some(response_json),
                    }
                }
            } else {
                IpcMessage::Error {
                    message: format!("Failed to process command: {}", command),
                    code: Some(500),
                }
            }
        }
    }
}

/// Start IPC server with ping/pong functionality and event processing
pub async fn start_ipc_server(port: u16) -> Result<mpsc::Receiver<EnhancedIpcEvent>> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    let server = Arc::new(IpcServer::new());
    
    // Create channel for enhanced IPC events
    let (event_tx, event_rx) = mpsc::channel(100);

    tracing::info!("Enhanced IPC server listening on {}", addr);

    // Spawn server task
    let event_tx_clone = event_tx.clone();
    tokio::spawn(async move {
        while let Ok((stream, addr)) = listener.accept().await {
            let server_clone = Arc::clone(&server);
            let event_tx = event_tx_clone.clone();
            
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, server_clone, event_tx).await {
                    tracing::error!("Error handling connection from {}: {}", addr, e);
                }
            });
        }
    });

    Ok(event_rx)
}

/// Handle WebSocket connection with ping/pong and client tracking
async fn handle_connection(stream: TcpStream, server: Arc<IpcServer>, event_tx: mpsc::Sender<EnhancedIpcEvent>) -> Result<()> {
    let websocket = accept_async(stream).await?;
    let (mut ws_sender, mut ws_receiver) = websocket.split();

    // Update client count
    server.update_client_count(1).await;

            // Send client connection event
        let _ = event_tx.send(EnhancedIpcEvent::ClientConnected(
            uuid::Uuid::new_v4().to_string()
        )).await;

    // Send initial status
    let status = ENGINE_STATUS.read().await.clone();
    let initial_message = IpcMessage::StatusResponse { status };
    let message_text = serde_json::to_string(&initial_message)?;
    ws_sender.send(Message::Text(message_text)).await?;

    // Handle incoming messages
    while let Some(msg_result) = ws_receiver.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                // Try to parse as IpcMessage first
                if let Ok(ipc_message) = serde_json::from_str::<IpcMessage>(&text) {
                    if let Some(response) = server.process_message(ipc_message).await {
                        let response_text = serde_json::to_string(&response)?;
                        ws_sender.send(Message::Text(response_text)).await?;
                    }
                } else {
                    // Fall back to legacy command handling
                    let response = handle_command_enhanced(&text, None).await;
                    let response_text = serde_json::to_string(&response)?;
                    ws_sender.send(Message::Text(response_text)).await?;
                }
            }
            Ok(Message::Ping(data)) => {
                ws_sender.send(Message::Pong(data)).await?;
            }
            Ok(Message::Close(_)) => {
                tracing::info!("Client disconnected");
                break;
            }
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    // Update client count on disconnect
    server.update_client_count(-1).await;
    
            // Send client disconnection event
        let _ = event_tx.send(EnhancedIpcEvent::ClientDisconnected(
            "disconnected".to_string()
        )).await;
    
    Ok(())
}

/// Update engine status (called from other modules)
pub async fn update_engine_status<F>(updater: F)
where
    F: FnOnce(&mut EngineStatus),
{
    let mut status = ENGINE_STATUS.write().await;
    updater(&mut *status);
}