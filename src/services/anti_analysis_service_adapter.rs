use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use crate::services::anti_analysis_service::AntiAnalysisService;
use super::Service;

/// Service adapter for anti-analysis services
pub struct AntiAnalysisServiceAdapter {
    inner: Arc<AntiAnalysisService>,
}

impl AntiAnalysisServiceAdapter {
    pub fn new(anti_analysis_service: Arc<AntiAnalysisService>) -> Self {
        Self {
            inner: anti_analysis_service,
        }
    }

    pub fn default_interval() -> Duration {
        Duration::from_secs(10) // Check for analysis attempts every 10 seconds
    }
}

#[async_trait::async_trait]
impl Service for AntiAnalysisServiceAdapter {
    async fn start(&self) -> Result<()> {
        tracing::info!("🛡️ Anti-analysis service adapter started");
        Ok(())
    }

    async fn tick(&self) -> Result<()> {
        // Process comprehensive anti-analysis detection
        let _ = self.inner.process_comprehensive_detection().await;

        // Process threat intelligence update
        let _ = self.inner.process_threat_intelligence_update().await;

        // Process detection session cleanup
        let _ = self.inner.process_detection_session_cleanup().await;

        // Get comprehensive anti-analysis statistics
        let _ = self.inner.get_anti_analysis_stats().await;

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("🛡️ Anti-analysis service adapter shutting down");
        Ok(())
    }
}
