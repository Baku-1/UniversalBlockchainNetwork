use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use crate::services::sync_business_services::SyncBusinessService;
use super::Service;

/// Service adapter for sync business services
pub struct SyncServiceAdapter {
    inner: Arc<SyncBusinessService>,
}

impl SyncServiceAdapter {
    pub fn new(sync_services: Arc<SyncBusinessService>) -> Self {
        Self {
            inner: sync_services,
        }
    }

    pub fn default_interval() -> Duration {
        Duration::from_secs(30) // Sync every 30 seconds
    }
}

#[async_trait::async_trait]
impl Service for SyncServiceAdapter {
    async fn start(&self) -> Result<()> {
        tracing::info!("🔄 Sync service adapter started");
        Ok(())
    }

    async fn tick(&self) -> Result<()> {
        // Process blockchain synchronization with mesh network broadcasting
        let _ = self.inner.process_blockchain_sync().await;

        // Process sync transactions through transaction queue
        let _ = self.inner.process_sync_transactions().await;

        // Process economic state synchronization
        let _ = self.inner.process_economic_state_sync().await;

        // Process mesh network synchronization
        let _ = self.inner.process_mesh_sync().await;

        // Process validation state synchronization
        let _ = self.inner.process_validation_sync().await;

        // Get comprehensive sync network statistics from all integrated components
        let _ = self.inner.get_sync_network_stats().await;

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("🔄 Sync service adapter shutting down");
        Ok(())
    }
}
