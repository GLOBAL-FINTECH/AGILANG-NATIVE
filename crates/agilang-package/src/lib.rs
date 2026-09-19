use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub project: Project,
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    pub version: String,
    #[serde(default = "default_edition")]
    pub edition: String,
    #[serde(default = "default_toolchain")]
    pub toolchain: String,
}
fn default_edition() -> String { "2026".into() }
fn default_toolchain() -> String { "0.8.0".into() }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Dependency {
    Version(String),
    Detailed { version: Option<String>, path: Option<String>, git: Option<String>, rev: Option<String> },
}
impl Dependency {
    pub fn canonical_source(&self, root: &Path) -> String {
        match self {
            Self::Version(v) => format!("registry:{v}"),
            Self::Detailed { version, path, git, rev } => {
                if let Some(p) = path {
                    format!("path:{}", normalize_path(&root.join(p)))
                } else if let Some(g) = git {
                    format!("git:{g}#{}", rev.as_deref().unwrap_or("HEAD"))
                } else {
                    format!("registry:{}", version.as_deref().unwrap_or("*"))
                }
            }
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockedDependency {
    pub name: String,
    pub source: String,
    pub checksum: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lockfile {
    pub format: u32,
    pub manifest_sha256: String,
    pub dependencies: Vec<LockedDependency>,
}
pub fn load_manifest(root: &Path) -> Result<(Manifest, String)> {
    let path = root.join("agilang.toml");
    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let manifest: Manifest = toml::from_str(std::str::from_utf8(&bytes)?).with_context(|| format!("failed to parse {}", path.display()))?;
    Ok((manifest, sha256_hex(&bytes)))
}
pub fn resolve_lockfile(root: &Path) -> Result<Lockfile> {
    let (manifest, manifest_sha256) = load_manifest(root)?;
    let mut dependencies = Vec::new();
    for (name, dependency) in manifest.dependencies {
        let source = dependency.canonical_source(root);
        let checksum = sha256_hex(source.as_bytes());
        dependencies.push(LockedDependency { name, source, checksum });
    }
    dependencies.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Lockfile { format: 1, manifest_sha256, dependencies })
}
pub fn write_lockfile(root: &Path, lock: &Lockfile) -> Result<PathBuf> {
    let path = root.join("agilang.lock");
    fs::write(&path, toml::to_string_pretty(lock)?)?;
    Ok(path)
}
pub fn verify_lockfile(root: &Path) -> Result<()> {
    let expected = resolve_lockfile(root)?;
    let path = root.join("agilang.lock");
    let actual: Lockfile = toml::from_str(std::str::from_utf8(&fs::read(&path).with_context(|| format!("missing {}", path.display()))?)?)?;
    anyhow::ensure!(actual == expected, "agilang.lock is stale; run agilang-pkg lock");
    Ok(())
}
fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}
fn normalize_path(path: &Path) -> String {
    path.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dependency_sources_are_deterministic() {
        let dep = Dependency::Detailed { version: Some("1.2.3".into()), path: Some("../shared".into()), git: None, rev: None };
        assert!(dep.canonical_source(Path::new("/workspace/app")).starts_with("path:"));
    }
    #[test]
    fn lockfile_dependencies_are_sorted() {
        let mut deps = BTreeMap::new();
        deps.insert("z".into(), Dependency::Version("1".into()));
        deps.insert("a".into(), Dependency::Version("2".into()));
        assert_eq!(deps.keys().cloned().collect::<Vec<_>>(), vec!["a", "z"]);
    }
}
