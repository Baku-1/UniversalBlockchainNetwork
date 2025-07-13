// src/mesh_routing.rs

use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use std::sync::Arc;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use crate::mesh::{MeshMessage, MeshMessageType};
use crate::mesh_topology::MeshTopology;

/// Message routing manager for mesh networks
pub struct MeshRouter {
    topology: Arc<RwLock<MeshTopology>>,
    message_cache: Arc<RwLock<HashMap<Uuid, CachedMessage>>>,
    pending_routes: Arc<RwLock<HashMap<String, PendingRoute>>>,
    local_node_id: String,
    route_discovery_timeout: Duration,
}

#[derive(Debug, Clone)]
struct CachedMessage {
    message: MeshMessage,
    received_at: SystemTime,
    forwarded_to: HashSet<String>,
}

#[derive(Debug, Clone)]
struct PendingRoute {
    destination: String,
    queued_messages: Vec<MeshMessage>,
    discovery_started: SystemTime,
    attempts: u32,
}

/// Route discovery protocol implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDiscoveryMessage {
    pub request_id: Uuid,
    pub source: String,
    pub destination: String,
    pub path: Vec<String>,
    pub hop_count: u8,
    pub max_hops: u8,
    pub discovery_type: DiscoveryType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscoveryType {
    Request,
    Reply,
    Error,
}

impl MeshRouter {
    /// Create a new mesh router
    pub fn new(local_node_id: String, topology: Arc<RwLock<MeshTopology>>) -> Self {
        Self {
            topology,
            message_cache: Arc::new(RwLock::new(HashMap::new())),
            pending_routes: Arc::new(RwLock::new(HashMap::new())),
            local_node_id,
            route_discovery_timeout: Duration::from_secs(30),
        }
    }

    /// Route a message to its destination
    pub async fn route_message(
        &self,
        message: MeshMessage,
        send_callback: impl Fn(MeshMessage, String) -> Result<(), Box<dyn std::error::Error>> + Send + Sync,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if message is for local node
        if let Some(target) = &message.target_id {
            if target == &self.local_node_id {
                return Ok(()); // Message is for us, don't route
            }
        }

        // Check message cache to prevent loops
        {
            let cache = self.message_cache.read().await;
            if cache.contains_key(&message.id) {
                return Ok(()); // Already seen this message
            }
        }

        // Add to cache
        {
            let mut cache = self.message_cache.write().await;
            cache.insert(message.id, CachedMessage {
                message: message.clone(),
                received_at: SystemTime::now(),
                forwarded_to: HashSet::new(),
            });
        }

        // Route the message
        match message.target_id.clone() {
            Some(destination) => {
                // Unicast routing
                self.route_unicast(message, &destination, send_callback).await
            }
            None => {
                // Broadcast routing
                self.route_broadcast(message, send_callback).await
            }
        }
    }

    /// Route a unicast message
    async fn route_unicast(
        &self,
        message: MeshMessage,
        destination: &str,
        send_callback: impl Fn(MeshMessage, String) -> Result<(), Box<dyn std::error::Error>> + Send + Sync,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if we have a route to the destination
        let next_hop = {
            let topology = self.topology.read().await;
            topology.get_next_hop(destination).cloned()
        };

        match next_hop {
            Some(next_hop) => {
                // We have a route, forward the message
                self.forward_message(message, &next_hop, send_callback).await
            }
            None => {
                // No route available, initiate route discovery
                self.initiate_route_discovery(message, destination).await
            }
        }
    }

    /// Route a broadcast message
    async fn route_broadcast(
        &self,
        message: MeshMessage,
        send_callback: impl Fn(MeshMessage, String) -> Result<(), Box<dyn std::error::Error>> + Send + Sync,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Get all neighbors except the sender
        let neighbors = {
            let topology = self.topology.read().await;
            topology.get_local_neighbors()
        };

        let sender_id = &message.sender_id;
        
        for neighbor in neighbors {
            if neighbor != *sender_id {
                // Check if we've already forwarded to this neighbor
                let should_forward = {
                    let mut cache = self.message_cache.write().await;
                    if let Some(cached) = cache.get_mut(&message.id) {
                        if !cached.forwarded_to.contains(&neighbor) {
                            cached.forwarded_to.insert(neighbor.clone());
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };

                if should_forward {
                    let mut forwarded_message = message.clone();
                    forwarded_message.hop_count += 1;
                    forwarded_message.ttl = forwarded_message.ttl.saturating_sub(1);
                    
                    if forwarded_message.ttl > 0 {
                        if let Err(e) = send_callback(forwarded_message, neighbor) {
                            tracing::warn!("Failed to forward broadcast message: {}", e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Forward a message to a specific next hop
    async fn forward_message(
        &self,
        mut message: MeshMessage,
        next_hop: &str,
        send_callback: impl Fn(MeshMessage, String) -> Result<(), Box<dyn std::error::Error>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Update message for forwarding
        message.hop_count += 1;
        message.ttl = message.ttl.saturating_sub(1);

        if message.ttl == 0 {
            tracing::warn!("Message {} TTL expired", message.id);
            return Ok(());
        }

        // Update cache
        {
            let mut cache = self.message_cache.write().await;
            if let Some(cached) = cache.get_mut(&message.id) {
                cached.forwarded_to.insert(next_hop.to_string());
            }
        }

        // Send the message
        send_callback(message, next_hop.to_string())
    }

    /// Initiate route discovery for a destination
    async fn initiate_route_discovery(
        &self,
        message: MeshMessage,
        destination: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if we already have a pending route discovery
        {
            let mut pending = self.pending_routes.write().await;
            if let Some(pending_route) = pending.get_mut(destination) {
                // Add message to queue
                pending_route.queued_messages.push(message);
                return Ok(());
            }

            // Create new pending route
            pending.insert(destination.to_string(), PendingRoute {
                destination: destination.to_string(),
                queued_messages: vec![message],
                discovery_started: SystemTime::now(),
                attempts: 1,
            });
        }

        // Start route discovery
        self.send_route_request(destination).await
    }

    /// Send a route request message
    async fn send_route_request(&self, destination: &str) -> Result<(), Box<dyn std::error::Error>> {
        let discovery_message = RouteDiscoveryMessage {
            request_id: Uuid::new_v4(),
            source: self.local_node_id.clone(),
            destination: destination.to_string(),
            path: vec![self.local_node_id.clone()],
            hop_count: 0,
            max_hops: 10, // Maximum hops for route discovery
            discovery_type: DiscoveryType::Request,
        };

        let _mesh_message = MeshMessage {
            id: Uuid::new_v4(),
            sender_id: self.local_node_id.clone(),
            target_id: None, // Broadcast
            message_type: MeshMessageType::RouteDiscovery,
            payload: bincode::serialize(&discovery_message)?,
            ttl: 10,
            hop_count: 0,
            timestamp: SystemTime::now(),
            signature: vec![], // TODO: Sign message
        };

        // TODO: Send the route discovery message
        tracing::debug!("Initiated route discovery for destination: {}", destination);
        Ok(())
    }

    /// Process a route discovery message
    pub async fn process_route_discovery(
        &self,
        discovery_message: RouteDiscoveryMessage,
        send_callback: impl Fn(MeshMessage, String) -> Result<(), Box<dyn std::error::Error>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match discovery_message.discovery_type {
            DiscoveryType::Request => {
                self.handle_route_request(discovery_message, send_callback).await
            }
            DiscoveryType::Reply => {
                self.handle_route_reply(discovery_message).await
            }
            DiscoveryType::Error => {
                self.handle_route_error(discovery_message).await
            }
        }
    }

    /// Handle incoming route request
    async fn handle_route_request(
        &self,
        mut discovery_message: RouteDiscoveryMessage,
        send_callback: impl Fn(MeshMessage, String) -> Result<(), Box<dyn std::error::Error>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if we've seen this request before
        if discovery_message.path.contains(&self.local_node_id) {
            return Ok(()); // Loop detected, ignore
        }

        // Add ourselves to the path
        discovery_message.path.push(self.local_node_id.clone());
        discovery_message.hop_count += 1;

        // Check if we are the destination
        if discovery_message.destination == self.local_node_id {
            // Send route reply
            let source_clone = discovery_message.source.clone();
            let reply = RouteDiscoveryMessage {
                request_id: discovery_message.request_id,
                source: discovery_message.destination,
                destination: discovery_message.source,
                path: discovery_message.path.clone(),
                hop_count: 0,
                max_hops: discovery_message.max_hops,
                discovery_type: DiscoveryType::Reply,
            };

            let mesh_message = MeshMessage {
                id: Uuid::new_v4(),
                sender_id: self.local_node_id.clone(),
                target_id: Some(source_clone),
                message_type: MeshMessageType::RouteDiscovery,
                payload: bincode::serialize(&reply)?,
                ttl: discovery_message.max_hops,
                hop_count: 0,
                timestamp: SystemTime::now(),
                signature: vec![],
            };

            // Send reply back along the path
            if let Some(previous_hop) = discovery_message.path.get(discovery_message.path.len() - 2) {
                send_callback(mesh_message, previous_hop.clone())?;
            }
        } else if discovery_message.hop_count < discovery_message.max_hops {
            // Forward the request to neighbors
            let neighbors = {
                let topology = self.topology.read().await;
                topology.get_local_neighbors()
            };

            for neighbor in neighbors {
                if !discovery_message.path.contains(&neighbor) {
                    let mesh_message = MeshMessage {
                        id: Uuid::new_v4(),
                        sender_id: self.local_node_id.clone(),
                        target_id: None, // Broadcast
                        message_type: MeshMessageType::RouteDiscovery,
                        payload: bincode::serialize(&discovery_message)?,
                        ttl: discovery_message.max_hops - discovery_message.hop_count,
                        hop_count: 0,
                        timestamp: SystemTime::now(),
                        signature: vec![],
                    };

                    if let Err(e) = send_callback(mesh_message, neighbor) {
                        tracing::warn!("Failed to forward route request: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle incoming route reply
    async fn handle_route_reply(
        &self,
        discovery_message: RouteDiscoveryMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Update topology with the discovered route
        {
            let mut topology = self.topology.write().await;
            
            // Add route to routing table
            if discovery_message.path.len() > 1 {
                let _destination = &discovery_message.source;
                let _next_hop = &discovery_message.path[1];
                topology.rebuild_routing_table();
            }
        }

        // Process queued messages for this destination
        let queued_messages = {
            let mut pending = self.pending_routes.write().await;
            if let Some(pending_route) = pending.remove(&discovery_message.source) {
                pending_route.queued_messages
            } else {
                Vec::new()
            }
        };

        // TODO: Send queued messages now that we have a route
        tracing::info!("Route discovered to {}, processing {} queued messages", 
                      discovery_message.source, queued_messages.len());

        Ok(())
    }

    /// Handle route error
    async fn handle_route_error(
        &self,
        discovery_message: RouteDiscoveryMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Remove the failed route from topology
        {
            let mut topology = self.topology.write().await;
            topology.rebuild_routing_table();
        }

        tracing::warn!("Route error for destination: {}", discovery_message.destination);
        Ok(())
    }

    /// Clean up expired cache entries and pending routes
    pub async fn cleanup(&self) {
        let now = SystemTime::now();
        let cache_ttl = Duration::from_secs(300); // 5 minutes
        
        // Clean message cache
        {
            let mut cache = self.message_cache.write().await;
            cache.retain(|_, cached| {
                now.duration_since(cached.received_at).unwrap_or_default() < cache_ttl
            });
        }

        // Clean pending routes
        {
            let mut pending = self.pending_routes.write().await;
            pending.retain(|_, route| {
                now.duration_since(route.discovery_started).unwrap_or_default() < self.route_discovery_timeout
            });
        }
    }

    /// Get routing statistics
    pub async fn get_routing_stats(&self) -> RoutingStats {
        let cache_size = self.message_cache.read().await.len();
        let pending_routes = self.pending_routes.read().await.len();
        
        RoutingStats {
            cached_messages: cache_size,
            pending_route_discoveries: pending_routes,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoutingStats {
    pub cached_messages: usize,
    pub pending_route_discoveries: usize,
}
