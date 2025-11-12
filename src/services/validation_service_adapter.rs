// src/services/validation_service_adapter.rs

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::services::validation_business_services::ValidationBusinessService;
use super::Service;

pub struct ValidationServiceAdapter {
    inner: Arc<ValidationBusinessService>,
    tick_counter: tokio::sync::RwLock<u32>,
}

impl ValidationServiceAdapter {
    pub fn new(inner: Arc<ValidationBusinessService>) -> Self {
        Self {
            inner,
            tick_counter: tokio::sync::RwLock::new(0),
        }
    }

    pub fn default_interval() -> Duration {
        Duration::from_secs(90) // 1.5 minutes
    }
}

#[async_trait::async_trait]
impl Service for ValidationServiceAdapter {
    async fn start(&self) -> Result<()> {
        // Initial validation pass on boot - could run smart contract validation
        tracing::info!("✅ Validation service adapter started");
        Ok(())
    }

    async fn tick(&self) -> Result<()> {
        let mut counter = self.tick_counter.write().await;
        *counter += 1;
        let tick_num = *counter;
        drop(counter);

        // Process smart contract validation every tick
        let _ = self
            .inner
            .process_smart_contract_validation("0x1234567890abcdef".to_string())
            .await;

        // Every 2 ticks, process distributed validation task
        if tick_num % 2 == 0 {
            let validation_data = vec![1, 2, 3, 4, 5];
            let _ = self
                .inner
                .process_distributed_validation_task(validation_data)
                .await;
        }

        // Every 3 ticks, process mesh network validation
        if tick_num % 3 == 0 {
            let mesh_validation_data = vec![10, 20, 30, 40, 50];
            let _ = self
                .inner
                .process_mesh_network_validation(mesh_validation_data)
                .await;
        }

        // Every 5 ticks, process secure validation execution
        if tick_num % 5 == 0 {
            let task_id = uuid::Uuid::new_v4();
            let _ = self
                .inner
                .process_secure_validation_execution(task_id)
                .await;
        }

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("✅ Validation service adapter shutting down");
        Ok(())
    }
}


