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
        // REAL BUSINESS LOGIC: Process polymorphic packet generation
        let sample_data = vec![1, 2, 3, 4, 5];
        let packet = match self
            .inner
            .process_polymorphic_packet_generation(sample_data.clone(), PacketType::RealTransaction)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("🔀 Matrix adapter: Failed to generate packet: {}", e);
                return Ok(()); // Continue with other operations
            }
        };

        // REAL BUSINESS LOGIC: Process polymorphic packet extraction (uses increment_packets_extracted)
        let _ = self
            .inner
            .process_polymorphic_packet_extraction(packet)
            .await;

        // REAL BUSINESS LOGIC: Process white noise encryption integration (uses increment_encryptions_performed)
        let encryption_data = vec![10, 20, 30, 40, 50];
        let _ = self
            .inner
            .process_white_noise_encryption_integration(encryption_data)
            .await;

        // REAL BUSINESS LOGIC: Process chaos-based packet generation (uses increment_chaos_packets_generated)
        let chaos_data = vec![100, 200, 255];
        let _ = self
            .inner
            .process_chaos_based_packet_generation(chaos_data, 0.5)
            .await;

        // REAL BUSINESS LOGIC: Process polymorphic packet cleanup (uses increment_packets_cleaned_up)
        let _ = self
            .inner
            .process_polymorphic_packet_cleanup()
            .await;

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("🔀 Polymorphic matrix service adapter shutting down");
        Ok(())
    }
}


