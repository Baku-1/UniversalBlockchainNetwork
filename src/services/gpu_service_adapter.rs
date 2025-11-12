// src/services/gpu_service_adapter.rs

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::services::gpu_business_services::GPUBusinessService;
use crate::validator::{TaskType, BlockToValidate};
use super::Service;

pub struct GPUServiceAdapter {
    inner: Arc<GPUBusinessService>,
    tick_counter: tokio::sync::RwLock<u32>,
}

impl GPUServiceAdapter {
    pub fn new(inner: Arc<GPUBusinessService>) -> Self {
        Self {
            inner,
            tick_counter: tokio::sync::RwLock::new(0),
        }
    }

    pub fn default_interval() -> Duration {
        Duration::from_secs(240) // 4 minutes
    }
}

#[async_trait::async_trait]
impl Service for GPUServiceAdapter {
    async fn start(&self) -> Result<()> {
        tracing::info!("🖥️ GPU service adapter started");
        Ok(())
    }

    async fn tick(&self) -> Result<()> {
        let mut counter = self.tick_counter.write().await;
        *counter += 1;
        let tick_num = *counter;
        drop(counter);

        // Process distributed GPU tasks every tick
        let task_data = vec![1, 2, 3, 4, 5];
        let block_to_validate = BlockToValidate {
            id: "0x1234567890abcdef".to_string(),
            data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        };
        let task_type = TaskType::BlockValidation(block_to_validate);

        let _ = self
            .inner
            .process_distributed_gpu_task(task_data, task_type)
            .await;

        // Every 2 ticks, process GPU reward distribution
        if tick_num % 2 == 0 {
            let _ = self
                .inner
                .process_gpu_reward_distribution(
                    uuid::Uuid::new_v4(),
                    "gpu_node_1".to_string(),
                    1000,
                )
                .await;
        }

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("🖥️ GPU service adapter shutting down");
        Ok(())
    }
}


