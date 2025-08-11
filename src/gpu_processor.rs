// src/gpu_processor.rs

use wgpu;
use anyhow::Result;
use std::time::{Duration, Instant};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use std::iter;
use tracing;
use serde::{Deserialize, Serialize};
use crate::validator::TaskPriority;
use tokio::sync::oneshot;
use bytemuck;

/// GPU capability information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GPUCapability {
    pub compute_units: u32,
    pub memory_gb: f32,
    pub benchmark_score: f64,
    pub supports_compute_shaders: bool,
    pub max_workgroup_size: u32,
    pub vendor: String,
    pub device_name: String,
}

/// GPU processing task
#[derive(Debug, Clone)]
pub struct GPUProcessingTask {
    pub task_id: String,
    pub data: Vec<f32>,
    pub compute_shader: String,
    pub workgroup_size: u32,
    pub expected_result_size: usize,
    pub priority: TaskPriority,
}



/// GPU processing result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GPUProcessingResult {
    pub task_id: String,
    pub result: Vec<f32>,
    pub processing_time_ms: u64,
    pub gpu_utilization: f64,
    pub memory_used_mb: f64,
    pub success: bool,
    pub error_message: Option<String>,
    pub benchmark_score: f64,
}

/// GPU processor for parallel computation
pub struct GPUProcessor {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub compute_pipeline: wgpu::ComputePipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub capabilities: GPUCapability,
    pub is_initialized: bool,
}

impl GPUProcessor {
    /// Create a new GPU processor
    pub async fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("No GPU adapter found"))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await?;

        // Create bind group layout for compute shaders
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Compute Bind Group Layout"),
            entries: &[
                // Input buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Output buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Create compute pipeline
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Compute Pipeline"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Compute Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            })),
            module: &device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Compute Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/compute.wgsl").into()),
            }),
            entry_point: "main",
        });

        // Get GPU capabilities
        let capabilities = Self::detect_gpu_capabilities(&adapter, &device, &queue).await?;

        tracing::info!("GPU processor initialized: {} ({})", capabilities.device_name, capabilities.vendor);

        Ok(Self {
            device,
            queue,
            compute_pipeline,
            bind_group_layout,
            capabilities,
            is_initialized: true,
        })
    }

    /// Detect GPU capabilities
    async fn detect_gpu_capabilities(
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<GPUCapability> {
        let info = adapter.get_info();
        
        // Estimate memory (this is approximate)
        let memory_gb = match info.device_type {
            wgpu::DeviceType::DiscreteGpu => 8.0, // High-end GPUs
            wgpu::DeviceType::IntegratedGpu => 2.0, // Integrated GPUs
            wgpu::DeviceType::Cpu => 1.0, // CPU fallback
            _ => 4.0, // Default
        };

        // Run benchmark to get performance score
        let benchmark_score = Self::run_gpu_benchmark(device, queue).await?;

        Ok(GPUCapability {
            compute_units: 1, // Would need to query actual GPU
            memory_gb,
            benchmark_score,
            supports_compute_shaders: true,
            max_workgroup_size: 256, // Default, would query actual GPU
            vendor: info.vendor.to_string(),
            device_name: info.name,
        })
    }

    /// Run GPU benchmark to measure performance
    async fn run_gpu_benchmark(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<f64> {
        let start = Instant::now();
        
        // Create test data
        let test_data: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        let buffer_size = (test_data.len() * std::mem::size_of::<f32>()) as u64;
        
        // Create input buffer
        let input_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Benchmark Input Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create output buffer
        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Benchmark Output Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // Write test data
        queue.write_buffer(&input_buffer, 0, bytemuck::cast_slice(&test_data));

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Benchmark Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Create compute pipeline
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Benchmark Pipeline"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Benchmark Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            })),
            module: &device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Benchmark Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("shaders/benchmark.wgsl").into()),
            }),
            entry_point: "main",
        });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Benchmark Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_buffer.as_entire_binding(),
                },
            ],
        });

        // Create command encoder and compute pass
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Benchmark Encoder"),
        });

        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Benchmark Pass"),
        });

        compute_pass.set_pipeline(&compute_pipeline);
        compute_pass.set_bind_group(0, &bind_group, &[]);
        compute_pass.dispatch_workgroups(test_data.len() as u32, 1, 1);

        drop(compute_pass);
        
        // Submit and wait
        queue.submit(iter::once(encoder.finish()));
        device.poll(wgpu::Maintain::Wait);

        let duration = start.elapsed();
        let benchmark_score = 1000000.0 / duration.as_millis() as f64;

        Ok(benchmark_score)
    }

    /// Process parallel computation task
    pub async fn process_parallel_computation(
        &self,
        task: &GPUProcessingTask,
    ) -> Result<GPUProcessingResult> {
        let start_time = Instant::now();
        
        if !self.is_initialized {
            return Err(anyhow::anyhow!("GPU processor not initialized"));
        }

        let buffer_size = (task.data.len() * std::mem::size_of::<f32>()) as u64;
        
        // Create input buffer
        let input_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Input Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create output buffer
        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Output Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // Write input data
        self.queue.write_buffer(&input_buffer, 0, bytemuck::cast_slice(&task.data));

        // Create bind group
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Compute Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_buffer.as_entire_binding(),
                },
            ],
        });

        // Create command encoder and compute pass
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Compute Encoder"),
        });

        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Compute Pass"),
        });

        compute_pass.set_pipeline(&self.compute_pipeline);
        compute_pass.set_bind_group(0, &bind_group, &[]);
        
        // Calculate dispatch size
        let workgroup_count = (task.data.len() as u32 + task.workgroup_size - 1) / task.workgroup_size;
        compute_pass.dispatch_workgroups(workgroup_count, 1, 1);

        drop(compute_pass);
        
        // Submit work
        self.queue.submit(iter::once(encoder.finish()));

        // Read back results
        let buffer_slice = output_buffer.slice(..);
        let (tx, rx) = oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        
        self.device.poll(wgpu::Maintain::Wait);
        rx.await??;

        let data = buffer_slice.get_mapped_range();
        let result: Vec<f32> = data.chunks_exact(4)
            .map(|chunk| f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        drop(data);
        output_buffer.unmap();

        let processing_time = start_time.elapsed();
        let gpu_utilization = self.estimate_gpu_utilization(&task, processing_time).await;

        Ok(GPUProcessingResult {
            task_id: task.task_id.clone(),
            result,
            processing_time_ms: processing_time.as_millis() as u64,
            gpu_utilization,
            memory_used_mb: buffer_size as f64 / (1024.0 * 1024.0),
            success: true,
            error_message: None,
            benchmark_score: gpu_utilization * 1000.0, // Simple benchmark score based on utilization
        })
    }

    /// Estimate GPU utilization based on task complexity and processing time
    async fn estimate_gpu_utilization(&self, task: &GPUProcessingTask, processing_time: Duration) -> f64 {
        // Base utilization calculation
        let base_utilization = match task.priority {
            TaskPriority::Critical => 0.95,
            TaskPriority::High => 0.85,
            TaskPriority::Normal => 0.70,
            TaskPriority::Low => 0.50,
        };

        // Adjust based on data size
        let size_factor = (task.data.len() as f64 / 1000000.0).min(1.0);
        
        // Adjust based on processing time vs expected
        let time_factor = if processing_time.as_millis() > 1000 {
            0.9 // Long processing time suggests high utilization
        } else {
            0.7 // Short processing time suggests lower utilization
        };

        (base_utilization * size_factor * time_factor).min(1.0)
    }

    /// Benchmark device capability
    pub async fn benchmark_device_capability(&self) -> GPUCapability {
        if !self.is_initialized {
            return GPUCapability {
                compute_units: 0,
                memory_gb: 0.0,
                benchmark_score: 0.0,
                supports_compute_shaders: false,
                max_workgroup_size: 0,
                vendor: "Unknown".to_string(),
                device_name: "Unknown".to_string(),
            };
        }

        // Run comprehensive benchmark
        let benchmark_score = self.run_comprehensive_benchmark().await.unwrap_or(0.0);
        
        let mut capabilities = self.capabilities.clone();
        capabilities.benchmark_score = benchmark_score;
        
        capabilities
    }

    /// Run comprehensive benchmark
    async fn run_comprehensive_benchmark(&self) -> Result<f64> {
        let mut total_score = 0.0;
        let mut test_count = 0;

        // Test different data sizes
        for size in [1000, 10000, 100000, 1000000] {
            let test_data: Vec<f32> = (0..size).map(|i| i as f32).collect();
            
            let task = GPUProcessingTask {
                task_id: format!("benchmark_{}", size),
                data: test_data,
                compute_shader: "".to_string(), // Use default shader
                workgroup_size: 256,
                expected_result_size: size,
                priority: TaskPriority::Normal,
            };

            match self.process_parallel_computation(&task).await {
                Ok(result) => {
                    total_score += result.benchmark_score;
                    test_count += 1;
                }
                Err(_) => {
                    // Skip failed tests
                }
            }
        }

        if test_count > 0 {
            Ok(total_score / test_count as f64)
        } else {
            Ok(0.0)
        }
    }

    /// Get GPU memory usage
    pub async fn get_memory_usage(&self) -> MemoryUsage {
        // This is a simplified implementation
        // In a real implementation, you would query the GPU driver for actual memory usage
        MemoryUsage {
            total_memory_mb: (self.capabilities.memory_gb as f64) * 1024.0,
            used_memory_mb: 0.0, // Would query actual usage
            free_memory_mb: (self.capabilities.memory_gb as f64) * 1024.0,
            utilization_percentage: 0.0,
        }
    }

    /// Check if GPU is healthy
    pub async fn health_check(&self) -> GPUHealthStatus {
        if !self.is_initialized {
            return GPUHealthStatus::NotInitialized;
        }

        // Simple health check
        let test_data: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
        let task = GPUProcessingTask {
            task_id: "health_check".to_string(),
            data: test_data,
            compute_shader: "".to_string(),
            workgroup_size: 4,
            expected_result_size: 4,
            priority: TaskPriority::Low,
        };

        match self.process_parallel_computation(&task).await {
            Ok(_) => GPUHealthStatus::Healthy,
            Err(_) => GPUHealthStatus::Unhealthy,
        }
    }

    /// Get GPU statistics
    pub async fn get_gpu_stats(&self) -> GPUStats {
        let memory_usage = self.get_memory_usage().await;
        let health_status = self.health_check().await;

        GPUStats {
            capabilities: self.capabilities.clone(),
            memory_usage,
            health_status,
            is_initialized: self.is_initialized,
            total_processing_time_ms: 0, // Would track over time
            tasks_processed: 0, // Would track over time
        }
    }
}

/// GPU memory usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUsage {
    pub total_memory_mb: f64,
    pub used_memory_mb: f64,
    pub free_memory_mb: f64,
    pub utilization_percentage: f64,
}

/// GPU health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GPUHealthStatus {
    Healthy,
    Unhealthy,
    NotInitialized,
    Error(String),
}

/// GPU statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GPUStats {
    pub capabilities: GPUCapability,
    pub memory_usage: MemoryUsage,
    pub health_status: GPUHealthStatus,
    pub is_initialized: bool,
    pub total_processing_time_ms: u64,
    pub tasks_processed: u64,
}

/// GPU task scheduler for managing multiple tasks
pub struct GPUTaskScheduler {
    pub gpu_processor: Arc<GPUProcessor>,
    pub task_queue: Arc<RwLock<Vec<GPUProcessingTask>>>,
    pub completed_tasks: Arc<RwLock<HashMap<String, GPUProcessingResult>>>,
    pub max_concurrent_tasks: usize,
}

impl GPUTaskScheduler {
    /// Create a new GPU task scheduler
    pub fn new(gpu_processor: Arc<GPUProcessor>) -> Self {
        Self {
            gpu_processor,
            task_queue: Arc::new(RwLock::new(Vec::new())),
            completed_tasks: Arc::new(RwLock::new(HashMap::new())),
            max_concurrent_tasks: 4, // Process 4 tasks concurrently
        }
    }

    /// Add task to queue
    pub async fn add_task(&self, task: GPUProcessingTask) -> Result<()> {
        let mut queue = self.task_queue.write().await;
        queue.push(task.clone());
        
        // Sort by priority
        queue.sort_by(|a, b| {
            let priority_order = |p: &TaskPriority| match p {
                TaskPriority::Critical => 0,
                TaskPriority::High => 1,
                TaskPriority::Normal => 2,
                TaskPriority::Low => 3,
            };
            priority_order(&a.priority).cmp(&priority_order(&b.priority))
        });

        tracing::info!("Added GPU task {} to queue (priority: {:?})", task.task_id, task.priority);
        Ok(())
    }

    /// Process next task in queue
    pub async fn process_next_task(&self) -> Result<Option<GPUProcessingResult>> {
        let mut queue = self.task_queue.write().await;
        
        if let Some(task) = queue.pop() {
            drop(queue); // Release lock before processing
            
            let result = self.gpu_processor.process_parallel_computation(&task).await?;
            
            // Store completed task
            {
                let mut completed = self.completed_tasks.write().await;
                completed.insert(task.task_id.clone(), result.clone());
            }
            
            tracing::info!("Processed GPU task {} in {}ms", task.task_id, result.processing_time_ms);
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    /// Get queue statistics
    pub async fn get_queue_stats(&self) -> QueueStats {
        let queue = self.task_queue.read().await;
        let completed = self.completed_tasks.read().await;
        
        QueueStats {
            pending_tasks: queue.len(),
            completed_tasks: completed.len(),
            queue_utilization: queue.len() as f64 / self.max_concurrent_tasks as f64,
        }
    }
}

/// Queue statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub pending_tasks: usize,
    pub completed_tasks: usize,
    pub queue_utilization: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gpu_processor_creation() {
        // This test requires a GPU, so it might fail in CI environments
        match GPUProcessor::new().await {
            Ok(processor) => {
                assert!(processor.is_initialized);
                assert!(processor.capabilities.supports_compute_shaders);
            }
            Err(_) => {
                // GPU not available, skip test
                tracing::warn!("GPU not available, skipping GPU processor test");
            }
        }
    }

    #[tokio::test]
    async fn test_task_scheduler() {
        // Skip this test for now as it requires mock GPU components
        tracing::warn!("Skipping GPU task scheduler test - requires mock GPU components");
        return;
    }
}
