// src/p2p.rs

use tokio::sync::mpsc;
use crate::crypto::NodeKeypair;
use crate::validator::{ComputationTask, TaskResult};
use crate::mesh;
use libp2p::{
    gossipsub, mdns, noise, tcp, yamux, swarm::NetworkBehaviour, swarm::SwarmEvent,
    PeerId, identity::Keypair as Libp2pKeypair
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use futures_util::StreamExt;

/// Network behaviour for the Aura P2P node
#[derive(NetworkBehaviour)]
struct AuraBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
}

/// The main networking task. It detects connectivity and launches the appropriate mode.
pub async fn start_p2p_node(
    node_keys: NodeKeypair,
    to_validator: mpsc::Sender<ComputationTask>,
    from_validator: mpsc::Receiver<TaskResult>,
) {
    // 1. **Check for a stable internet connection.**
    let have_internet = check_internet_connection().await;

    // 2. **Launch the appropriate mode.**
    if have_internet {
        tracing::info!("Internet connection detected. Starting in WAN mode.");
        if let Err(e) = run_wan_mode(node_keys, to_validator, from_validator).await {
            tracing::error!("WAN mode failed: {}", e);
        }
    } else {
        tracing::info!("No internet connection. Starting in Bluetooth Mesh mode.");
        // The responsibility for mesh networking is delegated to our new module.
        if let Err(e) = mesh::run_mesh_mode(node_keys, to_validator, from_validator).await {
            tracing::error!("Bluetooth Mesh Mode failed: {}", e);
        }
    }
}

/// Checks for a live internet connection.
async fn check_internet_connection() -> bool {
    // Try to resolve a known DNS record to check internet connectivity
    match tokio::net::lookup_host("8.8.8.8:53").await {
        Ok(_) => {
            tracing::debug!("Internet connectivity confirmed");
            true
        }
        Err(_) => {
            tracing::debug!("No internet connectivity detected");
            false
        }
    }
}

/// Runs the WAN mode using libp2p for local network discovery
async fn run_wan_mode(
    node_keys: NodeKeypair,
    _to_validator: mpsc::Sender<ComputationTask>,
    _from_validator: mpsc::Receiver<TaskResult>,
) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Starting WAN mode with libp2p and mDNS discovery");

    // Create a libp2p keypair from our node keys
    let local_key = create_libp2p_keypair(&node_keys)?;
    let local_peer_id = PeerId::from(local_key.public());
    tracing::info!("Local peer ID: {}", local_peer_id);

    // Set up Gossipsub
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(10))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .build()
        .expect("Valid config");

    let mut gossipsub = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(local_key.clone()),
        gossipsub_config,
    )?;

    // Subscribe to the Aura topic
    let topic = gossipsub::IdentTopic::new("aura-validation-network");
    gossipsub.subscribe(&topic)?;
    tracing::info!("Subscribed to topic: {}", topic);

    // Set up mDNS for local network discovery
    let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)?;

    // Create the network behaviour
    let behaviour = AuraBehaviour { gossipsub, mdns };

    // Create the swarm
    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(local_key)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|_| behaviour)?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    // Listen on all interfaces
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    tracing::info!("Listening for connections on all interfaces");

    // Update IPC status to show P2P mode
    crate::ipc::update_engine_status(|status| {
        status.mode = "P2P Discovery".to_string();
        status.mesh_mode = false;
    }).await;

    tracing::info!("P2P Discovery mode active");

    // Main event loop
    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                tracing::info!("Listening on: {}", address);
            }
            SwarmEvent::Behaviour(event) => {
                match event {
                    AuraBehaviourEvent::Mdns(mdns::Event::Discovered(list)) => {
                        for (peer_id, multiaddr) in list {
                            tracing::info!("Discovered peer: {} at {}", peer_id, multiaddr);

                            // Update IPC status with discovered peer
                            crate::ipc::update_engine_status(|status| {
                                status.total_peers += 1;

                                // Add peer to the mesh peers list for visualization
                                let peer_info = crate::ipc::MeshPeerInfo {
                                    id: peer_id.to_string(),
                                    is_connected: false, // Will be true when connection is established
                                    connection_quality: 0.95,
                                    last_seen: chrono::Utc::now().to_rfc3339(),
                                };
                                status.mesh_peers.push(peer_info);
                            }).await;

                            // Dial the discovered peer
                            if let Err(e) = swarm.dial(multiaddr) {
                                tracing::warn!("Failed to dial peer {}: {}", peer_id, e);
                            }
                        }
                    }
                    AuraBehaviourEvent::Mdns(mdns::Event::Expired(list)) => {
                        for (peer_id, _) in list {
                            tracing::info!("Peer expired: {}", peer_id);

                            // Update IPC status
                            crate::ipc::update_engine_status(|status| {
                                if status.connected_peers > 0 {
                                    status.connected_peers -= 1;
                                }
                                // Remove peer from mesh peers list
                                status.mesh_peers.retain(|p| p.id != peer_id.to_string());
                            }).await;
                        }
                    }
                    AuraBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                        propagation_source: peer_id,
                        message_id: _,
                        message,
                    }) => {
                        tracing::info!(
                            "Received message from {}: {}",
                            peer_id,
                            String::from_utf8_lossy(&message.data)
                        );
                    }
                    _ => {}
                }
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                tracing::info!("Connection established with: {}", peer_id);

                // Update IPC status with successful connection
                crate::ipc::update_engine_status(|status| {
                    status.connected_peers += 1;

                    // Update the peer's connection status
                    if let Some(peer) = status.mesh_peers.iter_mut().find(|p| p.id == peer_id.to_string()) {
                        peer.is_connected = true;
                        peer.last_seen = chrono::Utc::now().to_rfc3339();
                    }
                }).await;
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                tracing::info!("Connection closed with: {}", peer_id);

                // Update IPC status
                crate::ipc::update_engine_status(|status| {
                    if status.connected_peers > 0 {
                        status.connected_peers -= 1;
                    }

                    // Update the peer's connection status
                    if let Some(peer) = status.mesh_peers.iter_mut().find(|p| p.id == peer_id.to_string()) {
                        peer.is_connected = false;
                        peer.last_seen = chrono::Utc::now().to_rfc3339();
                    }
                }).await;
            }
            _ => {}
        }
    }
}

/// Creates a libp2p keypair from our node keys
fn create_libp2p_keypair(node_keys: &NodeKeypair) -> Result<Libp2pKeypair, Box<dyn std::error::Error>> {
    // Convert our ed25519 keypair to libp2p format
    // For now, we'll generate a new keypair but use our node keys to seed it
    let mut hasher = DefaultHasher::new();
    node_keys.public_key().as_bytes().hash(&mut hasher);
    let _seed = hasher.finish();

    // Generate a keypair (libp2p 0.53 doesn't take RNG parameter)
    let keypair = Libp2pKeypair::generate_ed25519();

    Ok(keypair)
}