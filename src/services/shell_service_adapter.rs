// src/services/shell_service_adapter.rs

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::services::engine_shell_service::EngineShellService;
use super::Service;

pub struct ShellServiceAdapter {
    inner: Arc<EngineShellService>,
    tick_counter: tokio::sync::RwLock<u32>,
}

impl ShellServiceAdapter {
    pub fn new(inner: Arc<EngineShellService>) -> Self {
        Self {
            inner,
            tick_counter: tokio::sync::RwLock::new(0),
        }
    }

    pub fn default_interval() -> Duration {
        Duration::from_secs(180) // 3 minutes
    }
}

#[async_trait::async_trait]
impl Service for ShellServiceAdapter {
    async fn start(&self) -> Result<()> {
        tracing::info!("🛡️ Engine shell service adapter started");
        Ok(())
    }

    async fn tick(&self) -> Result<()> {
        let mut counter = self.tick_counter.write().await;
        *counter += 1;
        let tick_num = *counter;
        drop(counter);

        // Process shell layer rotation every tick
        let _ = self.inner.process_shell_layer_rotation().await;

        // Every 2 ticks, process engine shell decryption
        if tick_num % 2 == 0 {
            let _ = self
                .inner
                .process_engine_shell_decryption(uuid::Uuid::new_v4())
                .await;
        }

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("🛡️ Engine shell service adapter shutting down");
        Ok(())
    }
}


