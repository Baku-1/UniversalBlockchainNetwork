// src/services/chaos_service_adapter.rs

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::services::chaos_encryption_service::ChaosEncryptionService;
use crate::polymorphic_matrix::PacketType;
use super::Service;

pub struct ChaosServiceAdapter {
    inner: Arc<ChaosEncryptionService>,
}

impl ChaosServiceAdapter {
    pub fn new(inner: Arc<ChaosEncryptionService>) -> Self {
        Self { inner }
    }

    pub fn default_interval() -> Duration {
        Duration::from_secs(150) // 2.5 minutes
    }
}

#[async_trait::async_trait]
impl Service for ChaosServiceAdapter {
    async fn start(&self) -> Result<()> {
        tracing::info!("🌀 Chaos encryption service adapter started");
        Ok(())
    }

    async fn tick(&self) -> Result<()> {
        // Process chaos encryption
        let sample_data = vec![10, 20, 30, 40, 50];
        let _ = self
            .inner
            .process_chaos_encryption(sample_data, PacketType::RealTransaction)
            .await;

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("🌀 Chaos encryption service adapter shutting down");
        Ok(())
    }
}


