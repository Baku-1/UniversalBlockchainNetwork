// src/services/validation_service_adapter.rs

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use anyhow::Result;

use crate::services::validation_business_services::ValidationBusinessService;
use crate::validator::{ComputationTask, TaskResult};
use super::Service;

pub struct ValidationServiceAdapter {
    inner: Arc<ValidationBusinessService>,
    tick_counter: tokio::sync::RwLock<u32>,
    validator_handle: tokio::sync::RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl ValidationServiceAdapter {
    pub fn new(inner: Arc<ValidationBusinessService>) -> Self {
        Self {
            inner,
            tick_counter: tokio::sync::RwLock::new(0),
            validator_handle: tokio::sync::RwLock::new(None),
        }
    }

    /// Initialize validator event loop with channels from mesh/P2P infrastructure
    /// This integrates the legacy validator::start_validator into the adapter system
    pub async fn initialize_validator_loop(
        &self,
        task_rx: mpsc::Receiver<ComputationTask>,
        result_tx: mpsc::Sender<TaskResult>,
        p2p_result_tx: Option<mpsc::Sender<TaskResult>>,
    ) {
        tracing::info!("✅ Validation Service Adapter: Initializing validator event loop");
        let handle = self.inner.start_validator_event_loop(task_rx, result_tx, p2p_result_tx);
        let mut validator_handle = self.validator_handle.write().await;
        *validator_handle = Some(handle);
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

        // Every 7 ticks, monitor lending pool for validation purposes
        if tick_num % 7 == 0 {
            let _ = self
                .inner
                .monitor_lending_pool_for_validation()
                .await;
        }

        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("✅ Validation service adapter shutting down");
        
        // Abort validator event loop if it's running
        let mut validator_handle = self.validator_handle.write().await;
        if let Some(handle) = validator_handle.take() {
            handle.abort();
            tracing::info!("✅ Validation Service Adapter: Validator event loop stopped");
        }
        
        Ok(())
    }
}


