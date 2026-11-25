//! Hardware discovery, explicit dispatch policy, and native accelerator execution.
//! GPU execution is opt-in through the vulkan Cargo feature and is never simulated.

use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};

#[cfg(feature = "vulkan")]
mod vulkan;

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

    let mut devices = vec![DeviceInfo {
        id: "cpu:0".into(),
        kind: DeviceKind::Cpu,
        name: format!("{} {}", std::env::consts::ARCH, std::env::consts::OS),
        available: true,
        native: true,
        memory_bytes: None,
        features,
        reason_unavailable: None,
    }];
    #[cfg(feature = "vulkan")]
    devices.extend(vulkan::discover());
    devices
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
        DispatchPolicy::CpuOnly => Ok(cpu),
        DispatchPolicy::PreferAccelerator => Ok(devices
            .iter()
            .find(|d| d.kind == DeviceKind::Vulkan && d.available)
            .cloned()
            .unwrap_or(cpu)),
        DispatchPolicy::RequireAccelerator => devices
            .into_iter()
            .find(|d| d.kind == DeviceKind::Vulkan && d.available)
            .ok_or_else(|| {
                AgilangError::new(
                ErrorCode::Unsupported,
                "no usable Vulkan GPU; enable feature vulkan and install a working Vulkan driver",
            )
            }),
    }
}

/// Multiplies row-major f32 matrices A[m,k] and B[k,n].
/// CpuOnly is deterministic CPU execution. PreferAccelerator may fall back to
/// CPU if GPU initialization or dispatch fails. RequireAccelerator never falls back.
pub fn matmul_f32(
    lhs: &[f32],
    rhs: &[f32],
    m: usize,
    k: usize,
    n: usize,
    policy: DispatchPolicy,
) -> RuntimeResult<Vec<f32>> {
    if m == 0 || k == 0 || n == 0 {
        return Err(AgilangError::invalid_argument(
            "matrix dimensions must be nonzero",
        ));
    }
    let left_len = m
        .checked_mul(k)
        .ok_or_else(|| AgilangError::new(ErrorCode::Overflow, "matrix size overflow"))?;
    let right_len = k
        .checked_mul(n)
        .ok_or_else(|| AgilangError::new(ErrorCode::Overflow, "matrix size overflow"))?;
    let output_len = m
        .checked_mul(n)
        .ok_or_else(|| AgilangError::new(ErrorCode::Overflow, "matrix size overflow"))?;
    if lhs.len() != left_len || rhs.len() != right_len {
        return Err(AgilangError::invalid_argument(
            "matrix data length does not match dimensions",
        ));
    }
    if lhs.iter().chain(rhs.iter()).any(|v| !v.is_finite()) {
        return Err(AgilangError::invalid_argument(
            "matrix entries must be finite",
        ));
    }
    if policy != DispatchPolicy::CpuOnly {
        #[cfg(feature = "vulkan")]
        {
            match vulkan::matmul(lhs, rhs, m, k, n) {
                Ok(values) => return Ok(values),
                Err(e) if policy == DispatchPolicy::RequireAccelerator => return Err(e),
                Err(_) => {} // Explicit prefer policy permits CPU fallback.
            }
        }
        #[cfg(not(feature = "vulkan"))]
        if policy == DispatchPolicy::RequireAccelerator {
            return Err(AgilangError::new(
                ErrorCode::Unsupported,
                "Vulkan backend was not compiled; enable --features vulkan",
            ));
        }
    }
    let mut out = vec![0.0f32; output_len];
    for row in 0..m {
        for p in 0..k {
            for col in 0..n {
                out[row * n + col] += lhs[row * k + p] * rhs[p * n + col];
            }
        }
    }
    Ok(out)
}

pub fn capabilities_json() -> RuntimeResult<Vec<u8>> {
    serde_json::to_vec(&discover_devices())
        .map_err(|e| AgilangError::new(ErrorCode::Serialization, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_matmul_matches_expected() {
        let out = matmul_f32(
            &[1., 2., 3., 4., 5., 6.],
            &[7., 8., 9., 10., 11., 12.],
            2,
            3,
            2,
            DispatchPolicy::CpuOnly,
        )
        .unwrap();
        assert_eq!(out, vec![58., 64., 139., 154.]);
    }

    #[test]
    fn rejects_bad_dimensions() {
        assert!(matmul_f32(&[1.], &[1.], 2, 1, 1, DispatchPolicy::CpuOnly).is_err());
        assert!(matmul_f32(&[1.], &[1.], 0, 1, 1, DispatchPolicy::CpuOnly).is_err());
        assert!(matmul_f32(&[1.], &[1.], usize::MAX, 2, 1, DispatchPolicy::CpuOnly).is_err());
    }

    #[test]
    fn cpu_always_discoverable() {
        assert_eq!(
            select_device(DispatchPolicy::CpuOnly).unwrap().kind,
            DeviceKind::Cpu
        );
    }

    #[cfg(not(feature = "vulkan"))]
    #[test]
    fn require_accelerator_is_not_silent_cpu_fallback() {
        assert!(select_device(DispatchPolicy::RequireAccelerator).is_err());
        assert!(matmul_f32(&[1.], &[2.], 1, 1, 1, DispatchPolicy::RequireAccelerator).is_err());
    }

    #[cfg(feature = "vulkan")]
    #[test]
    fn hardware_gpu_matmul_opt_in() {
        if std::env::var("AGILANG_TEST_VULKAN").ok().as_deref() != Some("1") {
            return;
        }
        assert_eq!(
            select_device(DispatchPolicy::RequireAccelerator)
                .unwrap()
                .kind,
            DeviceKind::Vulkan
        );
        let lhs = [1., 2., 3., 4., 5., 6.];
        let rhs = [7., 8., 9., 10., 11., 12.];
        let out = matmul_f32(&lhs, &rhs, 2, 3, 2, DispatchPolicy::RequireAccelerator).unwrap();
        for (got, expected) in out.iter().zip([58., 64., 139., 154.]) {
            assert!(
                (got - expected).abs() < 0.001,
                "GPU result {got} != {expected}"
            );
        }
    }
}
