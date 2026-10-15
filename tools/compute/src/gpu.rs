use crate::cpu::*;
use std::{
    future::Future,
    pin::pin,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};
struct ThreadWake(std::thread::Thread);
impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}
fn block_on<F: Future>(f: F) -> F::Output {
    let mut future = pin!(f);
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park_timeout(Duration::from_secs(1)),
        }
    }
}
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    limit: u64,
}
pub struct Prepared<'a> {
    gpu: &'a Gpu,
    input: wgpu::Buffer,
    output: wgpu::Buffer,
    readback: wgpu::Buffer,
    group: wgpu::BindGroup,
    allocation_ms: f64,
}
impl Gpu {
    pub fn open() -> Option<Self> {
        let start = Instant::now();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let adapters = instance.enumerate_adapters(wgpu::Backends::VULKAN);
        for adapter in &adapters {
            println!(
                "adapter={:?} shader_int64={} storage_limit={}",
                adapter.get_info(),
                adapter.features().contains(wgpu::Features::SHADER_INT64),
                adapter.limits().max_storage_buffer_binding_size
            );
        }
        let adapter = adapters.into_iter().find(|a| {
            matches!(
                a.get_info().device_type,
                wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::DiscreteGpu
            ) && a.features().contains(wgpu::Features::SHADER_INT64)
        });
        let Some(adapter) = adapter else {
            println!("gpu_skipped=no_hardware_adapter_with_native_int64");
            return None;
        };
        let caps = adapter.limits();
        let capacity = u64::from(caps.max_storage_buffer_binding_size)
            .min(caps.max_buffer_size)
            .min(512 * 1024 * 1024);
        let limits = wgpu::Limits {
            max_storage_buffer_binding_size: capacity as u32,
            max_buffer_size: capacity,
            ..Default::default()
        };
        let (device, queue) = match block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Legacy compute experiment"),
            required_features: wgpu::Features::SHADER_INT64,
            required_limits: limits,
            ..Default::default()
        })) {
            Ok(pair) => pair,
            Err(e) => {
                println!("gpu_skipped=device_request_failed error={e}");
                return None;
            }
        };
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Exact utility selection"),
            source: wgpu::ShaderSource::Wgsl(include_str!("decision.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        if let Some(error) = block_on(device.pop_error_scope()) {
            println!("gpu_skipped=invalid_kernel error={error}");
            return None;
        }
        println!(
            "gpu_initialization_ms={:.3}",
            start.elapsed().as_secs_f64() * 1000.
        );
        let limit = u64::from(device.limits().max_storage_buffer_binding_size);
        Some(Self {
            device,
            queue,
            pipeline,
            limit,
        })
    }
    pub fn prepare(&self, input: &Input) -> Option<Prepared<'_>> {
        let size = (2 + 7 * input.rows.len() + 2 * input.people) as u64 * 8;
        if size > self.limit {
            println!(
                "gpu_skipped=binding_limit bytes={size} limit={}",
                self.limit
            );
            return None;
        }
        let start = Instant::now();
        let gpu_input = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: input.people as u64 * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: output.size(),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: gpu_input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        Some(Prepared {
            gpu: self,
            input: gpu_input,
            output,
            readback,
            group,
            allocation_ms: start.elapsed().as_secs_f64() * 1000.,
        })
    }
}
impl Prepared<'_> {
    fn submit(&self, people: usize, repeats: usize) {
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        for _ in 0..repeats {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.gpu.pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            pass.dispatch_workgroups((people as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&self.output, 0, &self.readback, 0, self.output.size());
        self.gpu.queue.submit([encoder.finish()]);
    }
    fn read(&self, expected: &[Answer]) -> Vec<Answer> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.gpu
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .expect("GPU completion");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("GPU readback callback")
            .expect("GPU map");
        let mapped = self.readback.slice(..).get_mapped_range();
        let words: &[u64] = bytemuck::cast_slice(&mapped);
        let answers: Vec<_> = words
            .as_chunks::<2>()
            .0
            .iter()
            .map(|w| Answer {
                id: w[0],
                score: w[1] as i64,
            })
            .collect();
        assert_eq!(
            answers, expected,
            "GPU must reproduce exact saturating selection"
        );
        drop(mapped);
        self.readback.unmap();
        answers
    }
    pub fn evaluate(&mut self, input: &Input, lanes: &Lanes, expected: &[Answer], repeat: usize) {
        let total = Instant::now();
        let start = Instant::now();
        let mut upload = self
            .gpu
            .queue
            .write_buffer_with(
                &self.input,
                0,
                wgpu::BufferSize::new(self.input.size()).unwrap(),
            )
            .expect("GPU staging allocation");
        let upload_enqueue = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        let words: &mut [u64] = bytemuck::cast_slice_mut(&mut upload);
        words[0] = input.people as u64;
        words[1] = input.choices as u64;
        let n = input.rows.len();
        words[2..2 + n].copy_from_slice(&lanes.ids);
        for column in 0..5 {
            words[2 + (column + 1) * n..2 + (column + 2) * n]
                .copy_from_slice(bytemuck::cast_slice(&lanes.fields[column]));
        }
        for (i, flag) in lanes.survival.iter().enumerate() {
            words[2 + 6 * n + i] = u64::from(*flag);
        }
        words[2 + 7 * n..2 + 7 * n + input.people].copy_from_slice(&input.current);
        for i in 0..input.people {
            words[2 + 7 * n + input.people + i] = u64::from(input.urgent[i]);
        }
        drop(upload);
        let packing = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        self.submit(input.people, 1);
        self.read(expected);
        let submit_readback = start.elapsed().as_secs_f64() * 1000.;
        let elapsed = total.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        self.submit(input.people, 32);
        self.read(expected);
        let resident = start.elapsed().as_secs_f64() * 1000. / 32.;
        println!(
            "population={} choices={} repeat={repeat} backend=gpu elapsed_ms={elapsed:.3} packing_ms={packing:.3} upload_enqueue_ms={upload_enqueue:.3} submit_readback_ms={submit_readback:.3} resident_dispatch_ms={resident:.3} setup_ms={:.3} device_buffer_bytes={}",
            input.people,
            input.choices,
            self.allocation_ms,
            self.input.size() + self.output.size() + self.readback.size()
        );
    }
}
