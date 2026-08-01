//! Resource-aware computation planning for AGILANG.
use agilang_runtime_core::{AgilangError, ErrorCode, RuntimeResult};
use memmap2::MmapMut;
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
};
use sysinfo::{Disks, System};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceSnapshot {
    pub total_ram_bytes: u64,
    pub available_ram_bytes: u64,
    pub logical_cpus: usize,
    pub disks: Vec<DiskSnapshot>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiskSnapshot {
    pub mount_point: PathBuf,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub removable: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum StorageTier {
    Ram,
    MemoryMappedSsd,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComputeBudget {
    pub max_ram_bytes: u64,
    pub max_spill_bytes: u64,
    pub max_threads: usize,
    pub spill_directory: Option<PathBuf>,
}
impl ComputeBudget {
    pub fn conservative(snapshot: &ResourceSnapshot) -> Self {
        Self {
            max_ram_bytes: snapshot.available_ram_bytes.saturating_mul(60) / 100,
            max_spill_bytes: snapshot
                .disks
                .iter()
                .map(|d| d.available_bytes)
                .max()
                .unwrap_or(0)
                .saturating_mul(50)
                / 100,
            max_threads: snapshot.logical_cpus.max(1),
            spill_directory: snapshot
                .disks
                .iter()
                .max_by_key(|d| d.available_bytes)
                .map(|d| d.mount_point.clone()),
        }
    }
    pub fn choose_tier(&self, bytes: u64, streaming_supported: bool) -> RuntimeResult<StorageTier> {
        if bytes <= self.max_ram_bytes {
            return Ok(StorageTier::Ram);
        }
        if streaming_supported && bytes <= self.max_spill_bytes && self.spill_directory.is_some() {
            return Ok(StorageTier::MemoryMappedSsd);
        }
        Err(AgilangError::new(
            ErrorCode::Overflow,
            format!(
                "computation requires {bytes} bytes, exceeding RAM budget {} and spill budget {}",
                self.max_ram_bytes, self.max_spill_bytes
            ),
        ))
    }
}
pub fn snapshot() -> ResourceSnapshot {
    let mut system = System::new();
    system.refresh_memory();
    let disks = Disks::new_with_refreshed_list()
        .iter()
        .map(|d| DiskSnapshot {
            mount_point: d.mount_point().to_path_buf(),
            total_bytes: d.total_space(),
            available_bytes: d.available_space(),
            removable: d.is_removable(),
        })
        .collect();
    ResourceSnapshot {
        total_ram_bytes: system.total_memory(),
        available_ram_bytes: system.available_memory(),
        logical_cpus: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
        disks,
    }
}
pub struct SpillBuffer {
    path: PathBuf,
    map: MmapMut,
}
impl SpillBuffer {
    pub fn create(directory: &Path, bytes: usize) -> RuntimeResult<Self> {
        std::fs::create_dir_all(directory).map_err(io_error)?;
        let file = tempfile::Builder::new()
            .prefix("agilang-spill-")
            .tempfile_in(directory)
            .map_err(io_error)?;
        file.as_file().set_len(bytes as u64).map_err(io_error)?;
        let (_, path) = file.keep().map_err(|e| io_error(e.error))?;
        let writable = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(io_error)?;
        let map = unsafe { MmapMut::map_mut(&writable).map_err(io_error)? };
        Ok(Self { path, map })
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.map
    }
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.map
    }
    pub fn flush(&self) -> RuntimeResult<()> {
        self.map.flush().map_err(io_error)
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for SpillBuffer {
    fn drop(&mut self) {
        let _ = self.map.flush();
        let _ = std::fs::remove_file(&self.path);
    }
}
fn io_error(error: impl std::fmt::Display) -> AgilangError {
    AgilangError::new(ErrorCode::Io, error.to_string())
}
pub fn capabilities_json() -> RuntimeResult<Vec<u8>> {
    serde_json::to_vec(&snapshot())
        .map_err(|e| AgilangError::new(ErrorCode::Serialization, e.to_string()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chooses_ram_then_spill() {
        let b = ComputeBudget {
            max_ram_bytes: 100,
            max_spill_bytes: 1000,
            max_threads: 2,
            spill_directory: Some(std::env::temp_dir()),
        };
        assert_eq!(b.choose_tier(80, true).unwrap(), StorageTier::Ram);
        assert_eq!(
            b.choose_tier(500, true).unwrap(),
            StorageTier::MemoryMappedSsd
        );
        assert!(b.choose_tier(500, false).is_err());
    }
}
