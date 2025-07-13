// src/mesh_topology.rs

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, SystemTime};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Mesh network topology manager
pub struct MeshTopology {
    /// Network graph: node_id -> set of connected neighbor node_ids
    graph: HashMap<String, HashSet<String>>,
    /// Node information
    nodes: HashMap<String, NodeInfo>,
    /// Routing table: destination -> next_hop
    routing_table: HashMap<String, String>,
    /// Route discovery cache
    route_cache: HashMap<String, CachedRoute>,
    /// Local node ID
    local_node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_id: String,
    pub last_seen: SystemTime,
    pub hop_count: u8, // Distance from local node
    pub connection_quality: f32, // 0.0 to 1.0
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone)]
struct CachedRoute {
    path: Vec<String>,
    cost: u32,
    created_at: SystemTime,
    ttl: Duration,
}

/// Route discovery message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRequest {
    pub request_id: Uuid,
    pub source: String,
    pub destination: String,
    pub path: Vec<String>, // Nodes traversed so far
    pub hop_count: u8,
    pub max_hops: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteReply {
    pub request_id: Uuid,
    pub source: String,
    pub destination: String,
    pub path: Vec<String>, // Complete path from source to destination
    pub cost: u32,
}

impl MeshTopology {
    /// Create a new mesh topology manager
    pub fn new(local_node_id: String) -> Self {
        let mut topology = Self {
            graph: HashMap::new(),
            nodes: HashMap::new(),
            routing_table: HashMap::new(),
            route_cache: HashMap::new(),
            local_node_id: local_node_id.clone(),
        };

        // Add local node
        topology.add_node(NodeInfo {
            node_id: local_node_id,
            last_seen: SystemTime::now(),
            hop_count: 0,
            connection_quality: 1.0,
            capabilities: vec!["game_sync".to_string(), "transaction_relay".to_string()],
        });

        topology
    }

    /// Add a node to the topology
    pub fn add_node(&mut self, node_info: NodeInfo) {
        self.nodes.insert(node_info.node_id.clone(), node_info.clone());
        self.graph.entry(node_info.node_id.clone()).or_insert_with(HashSet::new);
        
        // Invalidate affected routes
        self.invalidate_routes_for_node(&node_info.node_id);
    }

    /// Remove a node from the topology
    pub fn remove_node(&mut self, node_id: &str) {
        // Remove from graph
        self.graph.remove(node_id);
        
        // Remove connections to this node
        for neighbors in self.graph.values_mut() {
            neighbors.remove(node_id);
        }
        
        // Remove node info
        self.nodes.remove(node_id);
        
        // Invalidate affected routes
        self.invalidate_routes_for_node(node_id);
        
        // Rebuild routing table
        self.rebuild_routing_table();
    }

    /// Add a connection between two nodes
    pub fn add_connection(&mut self, node1: &str, node2: &str) {
        self.graph.entry(node1.to_string()).or_default().insert(node2.to_string());
        self.graph.entry(node2.to_string()).or_default().insert(node1.to_string());
        
        // Update hop counts
        self.update_hop_counts();
        
        // Invalidate affected routes
        self.invalidate_routes_for_connection(node1, node2);
    }

    /// Remove a connection between two nodes
    pub fn remove_connection(&mut self, node1: &str, node2: &str) {
        if let Some(neighbors) = self.graph.get_mut(node1) {
            neighbors.remove(node2);
        }
        if let Some(neighbors) = self.graph.get_mut(node2) {
            neighbors.remove(node1);
        }
        
        // Update hop counts
        self.update_hop_counts();
        
        // Invalidate affected routes
        self.invalidate_routes_for_connection(node1, node2);
        
        // Rebuild routing table
        self.rebuild_routing_table();
    }

    /// Get the next hop for a destination
    pub fn get_next_hop(&self, destination: &str) -> Option<&String> {
        self.routing_table.get(destination)
    }

    /// Get all connected neighbors of the local node
    pub fn get_local_neighbors(&self) -> Vec<String> {
        self.graph.get(&self.local_node_id)
            .map(|neighbors| neighbors.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all nodes in the network
    pub fn get_all_nodes(&self) -> Vec<&NodeInfo> {
        self.nodes.values().collect()
    }

    /// Find the shortest path to a destination using Dijkstra's algorithm
    pub fn find_shortest_path(&self, destination: &str) -> Option<Vec<String>> {
        if destination == self.local_node_id {
            return Some(vec![self.local_node_id.clone()]);
        }

        // Check cache first
        if let Some(cached) = self.route_cache.get(destination) {
            if SystemTime::now().duration_since(cached.created_at).unwrap_or_default() < cached.ttl {
                return Some(cached.path.clone());
            }
        }

        // Dijkstra's algorithm
        let mut distances: HashMap<String, u32> = HashMap::new();
        let mut previous: HashMap<String, String> = HashMap::new();
        let mut unvisited: HashSet<String> = self.nodes.keys().cloned().collect();

        // Initialize distances
        for node_id in &unvisited {
            distances.insert(node_id.clone(), if node_id == &self.local_node_id { 0 } else { u32::MAX });
        }

        while !unvisited.is_empty() {
            // Find unvisited node with minimum distance
            let current = unvisited.iter()
                .min_by_key(|node| distances.get(*node).unwrap_or(&u32::MAX))
                .cloned();

            if let Some(current) = current {
                unvisited.remove(&current);

                if current == destination {
                    break;
                }

                let current_distance = *distances.get(&current).unwrap_or(&u32::MAX);
                if current_distance == u32::MAX {
                    break; // No path exists
                }

                // Check neighbors
                if let Some(neighbors) = self.graph.get(&current) {
                    for neighbor in neighbors {
                        if unvisited.contains(neighbor) {
                            let edge_weight = self.calculate_edge_weight(&current, neighbor);
                            let alt_distance = current_distance.saturating_add(edge_weight);
                            
                            if alt_distance < *distances.get(neighbor).unwrap_or(&u32::MAX) {
                                distances.insert(neighbor.clone(), alt_distance);
                                previous.insert(neighbor.clone(), current.clone());
                            }
                        }
                    }
                }
            } else {
                break;
            }
        }

        // Reconstruct path
        if previous.contains_key(destination) {
            let mut path = Vec::new();
            let mut current = destination.to_string();
            
            while current != self.local_node_id {
                path.push(current.clone());
                if let Some(prev) = previous.get(&current) {
                    current = prev.clone();
                } else {
                    return None; // Path broken
                }
            }
            path.push(self.local_node_id.clone());
            path.reverse();
            
            Some(path)
        } else {
            None
        }
    }

    /// Rebuild the entire routing table
    pub fn rebuild_routing_table(&mut self) {
        self.routing_table.clear();
        
        for node_id in self.nodes.keys() {
            if node_id != &self.local_node_id {
                if let Some(path) = self.find_shortest_path(node_id) {
                    if path.len() > 1 {
                        // Next hop is the second node in the path
                        self.routing_table.insert(node_id.clone(), path[1].clone());
                    }
                }
            }
        }
    }

    /// Update hop counts for all nodes using BFS
    fn update_hop_counts(&mut self) {
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        
        // Start from local node
        queue.push_back((self.local_node_id.clone(), 0u8));
        visited.insert(self.local_node_id.clone());
        
        while let Some((current, hops)) = queue.pop_front() {
            // Update hop count for current node
            if let Some(node_info) = self.nodes.get_mut(&current) {
                node_info.hop_count = hops;
            }
            
            // Add neighbors to queue
            if let Some(neighbors) = self.graph.get(&current) {
                for neighbor in neighbors {
                    if !visited.contains(neighbor) {
                        visited.insert(neighbor.clone());
                        queue.push_back((neighbor.clone(), hops + 1));
                    }
                }
            }
        }
    }

    /// Calculate edge weight between two nodes
    fn calculate_edge_weight(&self, _node1: &str, node2: &str) -> u32 {
        // Base weight
        let mut weight = 1u32;
        
        // Adjust based on connection quality
        if let Some(node_info) = self.nodes.get(node2) {
            weight = ((1.0 - node_info.connection_quality) * 10.0) as u32 + 1;
        }
        
        weight
    }

    /// Invalidate cached routes for a specific node
    fn invalidate_routes_for_node(&mut self, node_id: &str) {
        self.route_cache.retain(|dest, _| dest != node_id);
        self.routing_table.remove(node_id);
    }

    /// Invalidate cached routes affected by a connection change
    fn invalidate_routes_for_connection(&mut self, _node1: &str, _node2: &str) {
        // For simplicity, clear all cached routes when topology changes
        self.route_cache.clear();
    }

    /// Clean up expired route cache entries
    pub fn cleanup_route_cache(&mut self) {
        let now = SystemTime::now();
        self.route_cache.retain(|_, route| {
            now.duration_since(route.created_at).unwrap_or_default() < route.ttl
        });
    }

    /// Get network statistics
    pub fn get_network_stats(&self) -> NetworkStats {
        NetworkStats {
            total_nodes: self.nodes.len(),
            connected_nodes: self.graph.values().map(|neighbors| if neighbors.is_empty() { 0 } else { 1 }).sum(),
            total_connections: self.graph.values().map(|neighbors| neighbors.len()).sum::<usize>() / 2,
            average_hop_count: {
                let total_hops: u32 = self.nodes.values().map(|node| node.hop_count as u32).sum();
                if self.nodes.len() > 1 {
                    total_hops as f32 / (self.nodes.len() - 1) as f32 // Exclude local node
                } else {
                    0.0
                }
            },
            routing_table_size: self.routing_table.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub total_nodes: usize,
    pub connected_nodes: usize,
    pub total_connections: usize,
    pub average_hop_count: f32,
    pub routing_table_size: usize,
}
