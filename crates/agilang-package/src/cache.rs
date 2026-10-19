use anyhow::Result;
use sha2::{Digest,Sha256};
use std::{fs,path::{Path,PathBuf}};
pub fn cache_path(root:&Path,source:&str)->PathBuf{
 let mut h=Sha256::new();h.update(source.as_bytes());let key=format!("{:x}",h.finalize());root.join(".agilang/cache").join(key)
}
pub fn store(root:&Path,source:&str,bytes:&[u8])->Result<PathBuf>{let p=cache_path(root,source);fs::create_dir_all(p.parent().unwrap())?;fs::write(&p,bytes)?;Ok(p)}
pub fn load(root:&Path,source:&str)->Result<Option<Vec<u8>>>{let p=cache_path(root,source);if !p.exists(){return Ok(None)} Ok(Some(fs::read(p)?))}
