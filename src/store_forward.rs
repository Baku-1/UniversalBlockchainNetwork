// src/store_forward.rs

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock};
use std::sync::Arc;
use uuid::Uuid;
use anyhow::Result;
use crate::crypto::NodeKeypair;
use crate::config::MeshConfig;
use crate::mesh_validation::MeshTransaction;

/// Store & forward manager for holding messages for offline users
pub struct StoreForwardManager {
    node_keys: NodeKeypair,
    config: MeshConfig,
    stored_messages: Arc<RwLock<HashMap<String, VecDeque<ForwardedMessage>>>>, // target_id -> messages
    delivery_attempts: Arc<RwLock<HashMap<Uuid, DeliveryAttempt>>>,
    incentive_balance: Arc<RwLock<u64>>, // RON earned for store & forward services
    sf_events: mpsc::Sender<StoreForwardEvent>,
}

/// Message stored for offline user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardedMessage {
    pub id: Uuid,
    pub target_user_id: String,
    pub sender_id: String,
    pub message_type: ForwardedMessageType,
    pub payload: Vec<u8>,
    pub stored_at: SystemTime,
    pub expires_at: SystemTime,
    pub delivery_attempts: u32,
    pub max_attempts: u32,
    pub incentive_amount: u64, // RON reward for successful delivery
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ForwardedMessageType {
    MeshTransaction(MeshTransaction),
    ValidationRequest,
    BalanceUpdate,
    UserMessage,
}

#[derive(Debug, Clone)]
pub struct DeliveryAttempt {
    pub message_id: Uuid,
    pub target_user_id: String,
    pub last_attempt: SystemTime,
    pub next_attempt: SystemTime,
    pub attempt_count: u32,
}

#[derive(Debug, Clone)]
pub enum StoreForwardEvent {
    MessageStored(Uuid),
    MessageDelivered(Uuid),
    MessageExpired(Uuid),
    IncentiveEarned(u64),
    DeliveryFailed(Uuid, String),
}

impl StoreForwardManager {
    /// Create a new store & forward manager
    pub fn new(node_keys: NodeKeypair, config: MeshConfig) -> Self {
        let (sf_events, _) = mpsc::channel(100);
        
        Self {
            node_keys,
            config,
            stored_messages: Arc::new(RwLock::new(HashMap::new())),
            delivery_attempts: Arc::new(RwLock::new(HashMap::new())),
            incentive_balance: Arc::new(RwLock::new(0)),
            sf_events,
        }
    }

    /// Start the store & forward service
    pub async fn start_service(&self) -> mpsc::Receiver<StoreForwardEvent> {
        let (tx, rx) = mpsc::channel(100);
        
        let sf_manager = self.clone();
        tokio::spawn(async move {
            sf_manager.service_loop(tx).await;
        });
        
        rx
    }

    /// Main service loop
    async fn service_loop(&self, _event_tx: mpsc::Sender<StoreForwardEvent>) {
        let mut cleanup_interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
        let mut delivery_interval = tokio::time::interval(Duration::from_secs(30)); // 30 seconds

        loop {
            tokio::select! {
                _ = cleanup_interval.tick() => {
                    self.cleanup_expired_messages().await;
                }
                
                _ = delivery_interval.tick() => {
                    self.attempt_message_deliveries().await;
                }
            }
        }
    }

    /// Store a message for an offline user
    pub async fn store_message(
        &self,
        target_user_id: String,
        sender_id: String,
        message_type: ForwardedMessageType,
        payload: Vec<u8>,
        incentive_amount: u64,
    ) -> Result<Uuid> {
        let message_id = Uuid::new_v4();
        
        let forwarded_message = ForwardedMessage {
            id: message_id,
            target_user_id: target_user_id.clone(),
            sender_id,
            message_type,
            payload,
            stored_at: SystemTime::now(),
            expires_at: SystemTime::now() + Duration::from_secs(86400), // 24 hours
            delivery_attempts: 0,
            max_attempts: 5,
            incentive_amount,
        };

        // Store the message
        {
            let mut stored = self.stored_messages.write().await;
            stored.entry(target_user_id.clone()).or_insert_with(VecDeque::new).push_back(forwarded_message);
        }

        // Create delivery attempt record
        {
            let mut attempts = self.delivery_attempts.write().await;
            attempts.insert(message_id, DeliveryAttempt {
                message_id,
                target_user_id,
                last_attempt: SystemTime::now(),
                next_attempt: SystemTime::now() + Duration::from_secs(30),
                attempt_count: 0,
            });
        }

        tracing::info!("Stored message {} for offline user", message_id);
        let _ = self.sf_events.send(StoreForwardEvent::MessageStored(message_id)).await;

        Ok(message_id)
    }

    /// Attempt to deliver stored messages
    async fn attempt_message_deliveries(&self) {
        let now = SystemTime::now();
        let ready_attempts: Vec<DeliveryAttempt> = {
            let attempts = self.delivery_attempts.read().await;
            attempts.values()
                .filter(|attempt| attempt.next_attempt <= now)
                .cloned()
                .collect()
        };

        for attempt in ready_attempts {
            if let Err(e) = self.try_deliver_message(&attempt).await {
                let error_msg = e.to_string();
                let attempt_id = attempt.message_id;
                drop(e); // Drop the error before any await
                tracing::warn!("Failed to deliver message {}: {}", attempt_id, error_msg);

                // Update delivery attempt
                self.update_delivery_attempt(attempt).await;
            }
        }
    }

    /// Try to deliver a specific message
    async fn try_deliver_message(&self, attempt: &DeliveryAttempt) -> Result<()> {
        // Check if target user is now online (this would integrate with mesh peer discovery)
        if self.is_user_online(&attempt.target_user_id).await? {
            // Retrieve and deliver the message
            let message = self.retrieve_message(&attempt.target_user_id, attempt.message_id).await?;
            
            if let Some(msg) = message {
                let incentive_amount = msg.incentive_amount;

                // Simulate message delivery (in real implementation, this would send via mesh)
                self.deliver_message_to_user(msg.clone()).await?;

                // Remove from storage
                self.remove_stored_message(&attempt.target_user_id, attempt.message_id).await?;

                // Remove delivery attempt
                {
                    let mut attempts = self.delivery_attempts.write().await;
                    attempts.remove(&attempt.message_id);
                }

                // Award incentive
                self.award_incentive(incentive_amount).await?;

                let _ = self.sf_events.send(StoreForwardEvent::MessageDelivered(attempt.message_id)).await;
                let _ = self.sf_events.send(StoreForwardEvent::IncentiveEarned(msg.incentive_amount)).await;
            }
        } else {
            return Err(anyhow::anyhow!("User still offline"));
        }

        Ok(())
    }

    /// Update delivery attempt with backoff
    async fn update_delivery_attempt(&self, mut attempt: DeliveryAttempt) {
        attempt.attempt_count += 1;
        attempt.last_attempt = SystemTime::now();
        
        // Exponential backoff: 30s, 1m, 2m, 4m, 8m
        let backoff_seconds = 30 * (2_u64.pow(attempt.attempt_count.min(5)));
        attempt.next_attempt = SystemTime::now() + Duration::from_secs(backoff_seconds);

        // Check if max attempts reached
        if attempt.attempt_count >= 5 {
            // Mark as failed and remove
            {
                let mut attempts = self.delivery_attempts.write().await;
                attempts.remove(&attempt.message_id);
            }
            
            self.remove_stored_message(&attempt.target_user_id, attempt.message_id).await.ok();
            let _ = self.sf_events.send(StoreForwardEvent::DeliveryFailed(attempt.message_id, "Max attempts reached".to_string())).await;
        } else {
            // Update attempt record
            let mut attempts = self.delivery_attempts.write().await;
            attempts.insert(attempt.message_id, attempt);
        }
    }

    /// Check if user is online (integrate with mesh peer discovery)
    async fn is_user_online(&self, _user_id: &str) -> Result<bool> {
        // TODO: Integrate with actual mesh peer discovery
        // For now, simulate random online status
        Ok(rand::random::<f32>() > 0.7) // 30% chance user is online
    }

    /// Retrieve a specific message for delivery
    async fn retrieve_message(&self, user_id: &str, message_id: Uuid) -> Result<Option<ForwardedMessage>> {
        let stored = self.stored_messages.read().await;
        
        if let Some(user_messages) = stored.get(user_id) {
            for message in user_messages {
                if message.id == message_id {
                    return Ok(Some(message.clone()));
                }
            }
        }
        
        Ok(None)
    }

    /// Deliver message to user (simulate actual delivery)
    async fn deliver_message_to_user(&self, message: ForwardedMessage) -> Result<()> {
        tracing::info!("Delivering stored message {} to user {}", message.id, message.target_user_id);
        
        // TODO: Implement actual message delivery via mesh network
        // This would send the message through the mesh to the now-online user
        
        Ok(())
    }

    /// Remove stored message after delivery
    async fn remove_stored_message(&self, user_id: &str, message_id: Uuid) -> Result<()> {
        let mut stored = self.stored_messages.write().await;
        
        if let Some(user_messages) = stored.get_mut(user_id) {
            user_messages.retain(|msg| msg.id != message_id);
            
            // Remove empty user entry
            if user_messages.is_empty() {
                stored.remove(user_id);
            }
        }
        
        Ok(())
    }

    /// Award incentive for successful delivery
    async fn award_incentive(&self, amount: u64) -> Result<()> {
        let mut balance = self.incentive_balance.write().await;
        *balance += amount;
        
        tracing::info!("Earned {} RON for store & forward service", amount);
        Ok(())
    }

    /// Clean up expired messages
    async fn cleanup_expired_messages(&self) {
        let now = SystemTime::now();
        let mut expired_messages = Vec::new();

        {
            let mut stored = self.stored_messages.write().await;
            
            for (user_id, messages) in stored.iter_mut() {
                let original_len = messages.len();
                messages.retain(|msg| {
                    if msg.expires_at <= now {
                        expired_messages.push((user_id.clone(), msg.id));
                        false
                    } else {
                        true
                    }
                });
                
                if messages.len() != original_len {
                    tracing::info!("Cleaned up {} expired messages for user {}", 
                        original_len - messages.len(), user_id);
                }
            }
            
            // Remove empty user entries
            stored.retain(|_, messages| !messages.is_empty());
        }

        // Remove delivery attempts for expired messages
        {
            let mut attempts = self.delivery_attempts.write().await;
            for (_, message_id) in &expired_messages {
                attempts.remove(message_id);
            }
        }

        // Send expiration events
        for (_, message_id) in expired_messages {
            let _ = self.sf_events.send(StoreForwardEvent::MessageExpired(message_id)).await;
        }
    }

    /// Get current incentive balance
    pub async fn get_incentive_balance(&self) -> u64 {
        *self.incentive_balance.read().await
    }

    /// Get stored message count for a user
    pub async fn get_stored_message_count(&self, user_id: &str) -> usize {
        let stored = self.stored_messages.read().await;
        stored.get(user_id).map(|msgs| msgs.len()).unwrap_or(0)
    }

    /// Get total stored messages across all users
    pub async fn get_total_stored_messages(&self) -> usize {
        let stored = self.stored_messages.read().await;
        stored.values().map(|msgs| msgs.len()).sum()
    }
}

impl Clone for StoreForwardManager {
    fn clone(&self) -> Self {
        Self {
            node_keys: self.node_keys.clone(),
            config: self.config.clone(),
            stored_messages: Arc::clone(&self.stored_messages),
            delivery_attempts: Arc::clone(&self.delivery_attempts),
            incentive_balance: Arc::clone(&self.incentive_balance),
            sf_events: self.sf_events.clone(),
        }
    }
}
