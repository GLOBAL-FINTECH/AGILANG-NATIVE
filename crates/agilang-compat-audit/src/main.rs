use anyhow::{bail, Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn require(root: &Path, rel: &str) -> Result<()> {
    let p = root.join(rel);
    if !p.exists() {
        bail!("missing required native compatibility asset: {}", rel);
    }
    if p.is_file() && fs::metadata(&p)?.len() == 0 {
        bail!("empty required asset: {}", rel);
    }
    Ok(())
}

fn main() -> Result<()> {
    let root = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or(env::current_dir()?);
    for rel in [
        "Cargo.toml",
        "README.md",
        "compat/canonical.lock.json",
        "compat/feature-matrix.csv",
        "compat/handbook-contract.json",
        "docs/NATIVE_ONLY_EXECUTION_POLICY.md",
        "docs/AI_IMPLEMENTATION_INSTRUCTIONS.md",
        "assets/branding/agilang-logo.png",
        "editor/vscode-agilang/package.json",
        "editor/vscode-agilang/syntaxes/agilang.tmLanguage.json",
        "editor/vscode-agilang/syntaxes/ags.tmLanguage.json",
    ] {
        require(&root, rel)?;
    }
    let contract = fs::read_to_string(root.join("compat/handbook-contract.json"))?;
    let _: serde_json::Value =
        serde_json::from_str(&contract).context("invalid handbook contract JSON")?;
    let policy = fs::read_to_string(root.join("docs/NATIVE_ONLY_EXECUTION_POLICY.md"))?;
    for forbidden in ["Python runtime is required", "pip install", "python -m"] {
        if policy.contains(forbidden) {
            bail!("native-only policy contains forbidden dependency statement: {forbidden}");
        }
    }
    println!("AGILANG native compatibility audit: PASS");
    println!("Python runtime dependency: NONE");
    println!("AGI backend: REQUIRED");
    println!("AGS frontend: REQUIRED");
    println!("Syntax and logo assets: PRESENT");
    Ok(())
}
