//! Vulkan-only backend implemented with wgpu's native Vulkan HAL.
//! Deliberately rejects software (CPU) Vulkan adapters.
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use wgpu::util::DeviceExt;

use super::{DeviceInfo, DeviceKind};

const SHADER: &str = r#"
struct Dimensions {
    m: u32,
    k: u32,
    n: u32,
    padding: u32,
};
@group(0) @binding(0) var<storage, read> lhs: array<f32>;
@group(0) @binding(1) var<storage, read> rhs: array<f32>;
@group(0) @binding(2) var<storage, read_write> output: array<f32>;
@group(0) @binding(3) var<uniform> dims: Dimensions;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let col = id.x;
    let row = id.y;
    if (row >= dims.m || col >= dims.n) {
        return;
    }
    var total: f32 = 0.0;
    for (var p: u32 = 0u; p < dims.k; p = p + 1u) {
        total = fma(lhs[row * dims.k + p], rhs[p * dims.n + col], total);
    }
    output[row * dims.n + col] = total;
}
"#;

fn unavailable(detail: impl std::fmt::Display) -> AgilangError {
    AgilangError::new(ErrorCode::Unsupported, format!("Vulkan GPU unavailable: {detail}"))
}
fn failed(detail: impl std::fmt::Display) -> AgilangError {
    AgilangError::new(ErrorCode::Internal, format!("Vulkan compute failed: {detail}"))
}

fn adapters() -> Vec<wgpu::Adapter> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let mut found = instance.enumerate_adapters(wgpu::Backends::VULKAN);
    found.retain(|adapter| {
        let info = adapter.get_info();
        info.backend == wgpu::Backend::Vulkan
            && matches!(info.device_type, wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu)
    });
    // Prefer the RX 6800 XT/discrete GPUs over integrated hardware.
    found.sort_by_key(|adapter| adapter.get_info().device_type != wgpu::DeviceType::DiscreteGpu);
    found
}

fn open_device(adapter: &wgpu::Adapter) -> RuntimeResult<(wgpu::Device, wgpu::Queue)> {
    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("AGILANG Vulkan compute"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    )).map_err(unavailable)
}

pub(super) fn discover() -> Vec<DeviceInfo> {
    adapters().into_iter().map(|adapter| {
        let info = adapter.get_info();
        let result = open_device(&adapter);
        DeviceInfo {
            id: format!("vulkan:{:04x}:{:04x}", info.vendor, info.device),
            kind: DeviceKind::Vulkan,
            name: info.name,
            available: result.is_ok(),
            native: true,
            // wgpu's adapter limits do not reliably expose physical VRAM.
            memory_bytes: None,
            features: vec!["compute-f32".into(), "matmul-f32".into()],
            reason_unavailable: result.err().map(|e| e.message),
        }
    }).collect()
}

fn as_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

pub(super) fn matmul(
    lhs: &[f32], rhs: &[f32], m: usize, k: usize, n: usize,
) -> RuntimeResult<Vec<f32>> {
    let (device, queue) = adapters()
        .into_iter()
        .find_map(|adapter| open_device(&adapter).ok())
        .ok_or_else(|| unavailable("no hardware Vulkan adapter with a working compute device"))?;
    let limits = device.limits();

    let m32 = u32::try_from(m).map_err(|_| unavailable("matrix dimension exceeds u32"))?;
    let k32 = u32::try_from(k).map_err(|_| unavailable("matrix dimension exceeds u32"))?;
    let n32 = u32::try_from(n).map_err(|_| unavailable("matrix dimension exceeds u32"))?;
    let input_a = as_bytes(lhs);
    let input_b = as_bytes(rhs);
    let output_len = m.checked_mul(n)
        .and_then(|len| len.checked_mul(4))
        .ok_or_else(|| unavailable("output size overflow"))?;
    let bytes_a = u64::try_from(input_a.len()).map_err(|_| unavailable("input too large"))?;
    let bytes_b = u64::try_from(input_b.len()).map_err(|_| unavailable("input too large"))?;
    let bytes_out = u64::try_from(output_len).map_err(|_| unavailable("output too large"))?;
    if [bytes_a, bytes_b, bytes_out].iter().any(|&len| {
        len > u64::from(limits.max_storage_buffer_binding_size)
            || len > limits.max_buffer_size
    }) {
        return Err(unavailable("matrix exceeds one storage-buffer allocation limit; tiling is not yet implemented"));
    }
    let grid_x = n32.checked_add(7).ok_or_else(|| unavailable("column dimension overflow"))? / 8;
    let grid_y = m32.checked_add(7).ok_or_else(|| unavailable("row dimension overflow"))? / 8;
    if grid_x > limits.max_compute_workgroups_per_dimension
        || grid_y > limits.max_compute_workgroups_per_dimension {
        return Err(unavailable("compute workgroup dimensions exceed adapter limits"));
    }

    let a = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("AGILANG matmul A"),
        contents: &input_a,
        usage: wgpu::BufferUsages::STORAGE,
    });
    let b = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("AGILANG matmul B"),
        contents: &input_b,
        usage: wgpu::BufferUsages::STORAGE,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("AGILANG matmul output"),
        size: bytes_out,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("AGILANG matmul readback"),
        size: bytes_out,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let dims = [m32, k32, n32, 0u32];
    let dimension_bytes: Vec<u8> = dims.iter().flat_map(|x| x.to_le_bytes()).collect();
    let parameters = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("AGILANG matrix dimensions"),
        contents: &dimension_bytes,
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let bindings = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("AGILANG Vulkan matmul bindings"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false, min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false, min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false, min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false, min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("AGILANG Vulkan matmul resources"),
        layout: &bindings,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: a.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: b.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: output.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: parameters.as_entire_binding() },
        ],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("AGILANG Vulkan f32 matmul WGSL"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("AGILANG Vulkan pipeline layout"),
        bind_group_layouts: &[&bindings],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("AGILANG Vulkan matmul"),
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("AGILANG Vulkan matmul encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("AGILANG matmul dispatch"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(grid_x, grid_y, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes_out);
    queue.submit(Some(encoder.finish()));

    let slice = readback.slice(..);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    slice.map_async(wgpu::MapMode::Read, move |status| {
        let _ = sender.send(status);
    });
    device.poll(wgpu::Maintain::Wait);
    receiver.recv().map_err(failed)?.map_err(failed)?;
    let mapped = slice.get_mapped_range();
    let result = mapped.chunks_exact(4)
        .map(|part| f32::from_le_bytes([part[0], part[1], part[2], part[3]]))
        .collect::<Vec<_>>();
    drop(mapped);
    readback.unmap();
    if result.iter().any(|v| !v.is_finite()) {
        return Err(failed("matrix output contains non-finite values"));
    }
    Ok(result)
}
