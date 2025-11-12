use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use crate::services::mesh_business_services::MeshBusinessService;
use super::Service;

/// Service adapter for mesh business services
pub struct MeshServiceAdapter {
    inner: Arc<MeshBusinessService>,
}

impl MeshServiceAdapter {
    pub fn new(mesh_services: Arc<MeshBusinessService>) -> Self {
        Self {
            inner: mesh_services,
        }
    }

    pub fn default_interval() -> Duration {
        Duration::from_secs(15) // Mesh operations every 15 seconds
    }
}

#[async_trait::async_trait]
impl Service for MeshServiceAdapter {
    async fn start(&self) -> Result<()> {
        tracing::info!("🕸️ Mesh service adapter started");
        Ok(())
    }

    async fn tick(&self) -> Result<()> {
        // Handle network topology changes - update routing table
        let _ = self.inner.handle_network_topology_changed().await;

        // Get mesh network statistics from all integrated components
        let _ = self.inner.get_mesh_network_stats().await;

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("🕸️ Mesh service adapter shutting down");
        Ok(())
    }
}
