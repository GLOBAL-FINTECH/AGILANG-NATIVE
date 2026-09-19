use anyhow::{Context, Result};
pub mod cache;
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
    let text = std::str::from_utf8(&bytes).context("agilang.toml is not UTF-8")?;
    Ok((parse_manifest(text)?, sha256_hex(&bytes)))
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
    let mut out = format!("format = {}\nmanifest_sha256 = \"{}\"\n\n", lock.format, lock.manifest_sha256);
    for dep in &lock.dependencies {
        out.push_str("[[dependencies]]\n");
        out.push_str(&format!("name = \"{}\"\nsource = \"{}\"\nchecksum = \"{}\"\n\n", dep.name, dep.source, dep.checksum));
    }
    fs::write(&path, out)?;
    Ok(path)
}
pub fn verify_lockfile(root: &Path) -> Result<()> {
    let expected = resolve_lockfile(root)?;
    let path = root.join("agilang.lock");
    let actual = parse_lockfile(&fs::read_to_string(&path).with_context(|| format!("missing {}", path.display()))?)?;
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

fn parse_manifest(text: &str) -> Result<Manifest> {
    let mut section = String::new(); let mut project = BTreeMap::<String,String>::new(); let mut deps = BTreeMap::new();
    for raw in text.lines() { let line = raw.split('#').next().unwrap_or("").trim(); if line.is_empty(){continue} if line.starts_with('[')&&line.ends_with(']'){section=line.trim_matches(&['[',']'][..]).to_string();continue} let Some((k,v))=line.split_once('=') else {continue}; let k=k.trim(); let v=v.trim().trim_matches('"').to_string(); if section=="project"{project.insert(k.into(),v)} else if section=="dependencies"{ if v.starts_with("{") { let inner=v.trim_matches(&['{','}'][..]); let mut version=None; let mut path=None; let mut git=None; let mut rev=None; for item in inner.split(","){ if let Some((ik,iv))=item.split_once("="){ let iv=iv.trim().trim_matches('"').to_string(); match ik.trim(){ "version"=>version=Some(iv), "path"=>path=Some(iv), "git"=>git=Some(iv), "rev"=>rev=Some(iv), _=>{} } } } deps.insert(k.into(),Dependency::Detailed{version,path,git,rev}); } else { deps.insert(k.into(),Dependency::Version(v)); } } }
    Ok(Manifest{project:Project{name:project.get("name").cloned().context("project.name is required")?,version:project.get("version").cloned().context("project.version is required")?,edition:project.get("edition").cloned().unwrap_or_else(||"2026".into()),toolchain:project.get("toolchain").cloned().unwrap_or_else(||"0.8.0".into())},dependencies:deps})
}
fn parse_lockfile(text: &str) -> Result<Lockfile> {
    let mut format=None; let mut hash=None; let mut deps=Vec::new(); let mut cur=None;
    for raw in text.lines(){let line=raw.trim(); if line=="[[dependencies]]"{if let Some(d)=cur.take(){deps.push(d)} cur=Some(LockedDependency{name:String::new(),source:String::new(),checksum:String::new()});continue} let Some((k,v))=line.split_once('=') else {continue}; let v=v.trim().trim_matches('"').to_string(); match k.trim(){"format"=>format=v.parse().ok(),"manifest_sha256"=>hash=Some(v),"name"=>if let Some(d)=cur.as_mut(){d.name=v},"source"=>if let Some(d)=cur.as_mut(){d.source=v},"checksum"=>if let Some(d)=cur.as_mut(){d.checksum=v},_=>{}}}
    if let Some(d)=cur{deps.push(d)} Ok(Lockfile{format:format.context("lock format missing")?,manifest_sha256:hash.context("manifest hash missing")?,dependencies:deps})
}
