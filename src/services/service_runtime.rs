// src/services/service_runtime.rs

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

#[async_trait::async_trait]
pub trait Service: Send + Sync + 'static {
    async fn start(&self) -> Result<()> { Ok(()) }
    async fn tick(&self) -> Result<()> { Ok(()) }
    async fn shutdown(&self) -> Result<()> { Ok(()) }
}

pub struct ServiceHandle;

impl ServiceHandle {
    pub fn spawn_periodic<S>(service: Arc<S>, interval: Duration, name: &'static str) -> tokio::task::JoinHandle<()>
    where
        S: Service,
    {
        tokio::spawn(async move {
            if let Err(e) = service.start().await {
                tracing::warn!("{} start failed: {}", name, e);
            }

            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                if let Err(e) = service.tick().await {
                    tracing::warn!("{} tick failed: {}", name, e);
                }
            }
        })
    }
}






