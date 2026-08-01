//! Hardware discovery and execution-policy layer for AGILANG.
//!
//! This crate never claims an accelerator is usable unless a concrete backend
//! has been compiled and initialized. CPU execution is always available.

use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeviceKind {
    Cpu,
    Cuda,
    DirectMl,
    Vulkan,
    Metal,
    OpenCl,
    Npu,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub kind: DeviceKind,
    pub name: String,
    pub available: bool,
    pub native: bool,
    pub memory_bytes: Option<u64>,
    pub features: Vec<String>,
    pub reason_unavailable: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DispatchPolicy {
    CpuOnly,
    PreferAccelerator,
    RequireAccelerator,
}

pub fn discover_devices() -> Vec<DeviceInfo> {
    let mut features = Vec::new();
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::is_x86_feature_detected!("sse2") {
            features.push("sse2".into());
        }
        if std::is_x86_feature_detected!("avx") {
            features.push("avx".into());
        }
        if std::is_x86_feature_detected!("avx2") {
            features.push("avx2".into());
        }
        if std::is_x86_feature_detected!("fma") {
            features.push("fma".into());
        }
        if std::is_x86_feature_detected!("aes") {
            features.push("aes-ni".into());
        }
        if std::is_x86_feature_detected!("sha") {
            features.push("sha-ni".into());
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        features.push("neon".into());
    }
    vec![DeviceInfo {
        id: "cpu:0".into(),
        kind: DeviceKind::Cpu,
        name: format!("{} {}", std::env::consts::ARCH, std::env::consts::OS),
        available: true,
        native: true,
        memory_bytes: None,
        features,
        reason_unavailable: None,
    }]
}

pub fn select_device(policy: DispatchPolicy) -> RuntimeResult<DeviceInfo> {
    let devices = discover_devices();
    let cpu = devices
        .iter()
        .find(|d| d.kind == DeviceKind::Cpu)
        .cloned()
        .ok_or_else(|| {
            AgilangError::new(ErrorCode::Unsupported, "native CPU backend unavailable")
        })?;
    match policy {
        DispatchPolicy::CpuOnly | DispatchPolicy::PreferAccelerator => Ok(cpu),
        DispatchPolicy::RequireAccelerator => Err(AgilangError::new(
            ErrorCode::Unsupported,
            "no compiled GPU/NPU backend is available; enable a concrete CUDA, DirectML, Vulkan, Metal, OpenCL, or NPU provider",
        )),
    }
}

pub fn capabilities_json() -> RuntimeResult<Vec<u8>> {
    serde_json::to_vec(&discover_devices())
        .map_err(|e| AgilangError::new(ErrorCode::Serialization, e.to_string()))
}
