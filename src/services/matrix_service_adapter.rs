// src/services/matrix_service_adapter.rs

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::services::polymorphic_matrix_service::PolymorphicMatrixService;
use crate::polymorphic_matrix::PacketType;
use super::Service;

pub struct MatrixServiceAdapter {
    inner: Arc<PolymorphicMatrixService>,
}

impl MatrixServiceAdapter {
    pub fn new(inner: Arc<PolymorphicMatrixService>) -> Self {
        Self { inner }
    }

    pub fn default_interval() -> Duration {
        Duration::from_secs(60) // 1 minute
    }
}

#[async_trait::async_trait]
impl Service for MatrixServiceAdapter {
    async fn start(&self) -> Result<()> {
        tracing::info!("🔀 Polymorphic matrix service adapter started");
        Ok(())
    }

    async fn tick(&self) -> Result<()> {
        // Process polymorphic packet generation
        let sample_data = vec![1, 2, 3, 4, 5];
        let _ = self
            .inner
            .process_polymorphic_packet_generation(sample_data, PacketType::RealTransaction)
            .await;

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("🔀 Polymorphic matrix service adapter shutting down");
        Ok(())
    }
}


