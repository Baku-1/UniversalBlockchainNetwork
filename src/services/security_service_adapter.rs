// src/services/security_service_adapter.rs

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::services::security_orchestration_service::SecurityOrchestrationService;
use super::Service;

pub struct SecurityServiceAdapter {
    inner: Arc<SecurityOrchestrationService>,
    audit_every: u32,
    counter: tokio::sync::RwLock<u32>,
}

impl SecurityServiceAdapter {
    pub fn new(inner: Arc<SecurityOrchestrationService>, audit_every: u32) -> Self {
        Self { inner, audit_every, counter: tokio::sync::RwLock::new(0) }
    }

    pub fn default_interval() -> Duration { Duration::from_secs(5) }
}

#[async_trait::async_trait]
impl Service for SecurityServiceAdapter {
    async fn start(&self) -> Result<()> {
        // Initial orchestration pass on boot
        let _ = self.inner.process_security_orchestration().await?;
        Ok(())
    }

    async fn tick(&self) -> Result<()> {
        // Periodic orchestration
        let _ = self.inner.process_security_orchestration().await?;

        // Every N ticks, run an audit as well
        {
            let mut c = self.counter.write().await;
            *c += 1;
            if *c % self.audit_every == 0 {
                let _ = self.inner.process_security_audit_orchestration().await?;
            }
        }
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        // Cleanup sessions on shutdown
        let _ = self.inner.process_security_session_cleanup().await?;
        Ok(())
    }
}






