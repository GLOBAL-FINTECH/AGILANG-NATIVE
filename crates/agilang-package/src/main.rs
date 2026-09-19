use agilang_package::{load_manifest, resolve_lockfile, verify_lockfile, write_lockfile};
use anyhow::{Context, Result};
use std::{env, path::PathBuf};

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".into());
    let root = PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    match command.as_str() {
        "init" => {
            let path = root.join("agilang.toml");
            if path.exists() { anyhow::bail!("{} already exists", path.display()); }
            let name = root.file_name().context("project path has no name")?.to_string_lossy();
            std::fs::write(&path, format!("[project]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\ntoolchain = \"0.8.0\"\n\n[dependencies]\n", name))?;
            println!("created {}", path.display());
        }
        "lock" => {
            let lock = resolve_lockfile(&root)?;
            let path = write_lockfile(&root, &lock)?;
            println!("locked {} dependencies -> {}", lock.dependencies.len(), path.display());
        }
        "check" => {
            let (manifest, hash) = load_manifest(&root)?;
            println!("{} {} (manifest {})", manifest.project.name, manifest.project.version, &hash[..16]);
            verify_lockfile(&root)?;
            println!("lockfile is reproducible and up to date");
        }
        _ => {
            println!("AGILANG package manager");
            println!("usage: agilang-pkg <init|lock|check> [project-dir]");
        }
    }
    Ok(())
}
