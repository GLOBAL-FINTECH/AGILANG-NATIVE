//! Cross-platform native operating-system services with explicit limits.

use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub os: String,
    pub architecture: String,
    pub family: String,
    pub executable: String,
    pub current_directory: String,
    pub process_id: u32,
    pub logical_cpus: usize,
}

#[derive(Debug, Clone)]
pub struct FileSystemPolicy {
    roots: Vec<PathBuf>,
    max_read_bytes: u64,
    allow_write: bool,
}

impl FileSystemPolicy {
    pub fn unrestricted() -> Self {
        Self {
            roots: Vec::new(),
            max_read_bytes: 256 * 1024 * 1024,
            allow_write: true,
        }
    }
    pub fn sandboxed(roots: Vec<PathBuf>, max_read_bytes: u64, allow_write: bool) -> Self {
        Self {
            roots,
            max_read_bytes,
            allow_write,
        }
    }

    fn authorize(&self, path: &Path, write: bool) -> RuntimeResult<PathBuf> {
        if write && !self.allow_write {
            return Err(AgilangError::new(
                ErrorCode::PermissionDenied,
                "filesystem writes are disabled",
            ));
        }
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            env::current_dir()?.join(path)
        };
        let normalized = normalize_path(&absolute);
        if !self.roots.is_empty() {
            let allowed = self
                .roots
                .iter()
                .map(|r| normalize_path(r))
                .any(|root| normalized.starts_with(root));
            if !allowed {
                return Err(AgilangError::new(
                    ErrorCode::PermissionDenied,
                    "path is outside configured filesystem roots",
                ));
            }
        }
        Ok(normalized)
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        use std::path::Component;
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn platform_info() -> RuntimeResult<PlatformInfo> {
    Ok(PlatformInfo {
        os: env::consts::OS.to_owned(),
        architecture: env::consts::ARCH.to_owned(),
        family: env::consts::FAMILY.to_owned(),
        executable: env::current_exe()?.to_string_lossy().into_owned(),
        current_directory: env::current_dir()?.to_string_lossy().into_owned(),
        process_id: std::process::id(),
        logical_cpus: std::thread::available_parallelism()
            .map(|v| v.get())
            .unwrap_or(1),
    })
}

pub fn unix_time_millis() -> RuntimeResult<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .map_err(|e| AgilangError::new(ErrorCode::Internal, e.to_string()))
}

pub fn environment(name: &str) -> Option<String> {
    env::var(name).ok()
}

pub fn read_file(policy: &FileSystemPolicy, path: impl AsRef<Path>) -> RuntimeResult<Vec<u8>> {
    let path = policy.authorize(path.as_ref(), false)?;
    let metadata = fs::metadata(&path)?;
    if metadata.len() > policy.max_read_bytes {
        return Err(
            AgilangError::new(ErrorCode::Overflow, "file exceeds configured read limit")
                .with_context("bytes", metadata.len().to_string()),
        );
    }
    Ok(fs::read(path)?)
}

pub fn write_file(
    policy: &FileSystemPolicy,
    path: impl AsRef<Path>,
    data: &[u8],
) -> RuntimeResult<()> {
    let path = policy.authorize(path.as_ref(), true)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, data)?;
    Ok(())
}

pub fn create_directory(policy: &FileSystemPolicy, path: impl AsRef<Path>) -> RuntimeResult<()> {
    fs::create_dir_all(policy.authorize(path.as_ref(), true)?)?;
    Ok(())
}

pub fn remove_file(policy: &FileSystemPolicy, path: impl AsRef<Path>) -> RuntimeResult<()> {
    fs::remove_file(policy.authorize(path.as_ref(), true)?)?;
    Ok(())
}
