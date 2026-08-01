use anyhow::{bail, Context, Result};
use serde_json::Value;
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

fn validate_canonical_lock(root: &Path) -> Result<()> {
    let lock_path = root.join("compat/canonical.lock.json");
    let lock_text = fs::read_to_string(&lock_path)?;
    let lock: Value =
        serde_json::from_str(&lock_text).context("invalid canonical lock JSON")?;

    let schema = lock
        .get("schema")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if schema != "AGI-COMPAT-1" {
        bail!("canonical lock must declare schema AGI-COMPAT-1");
    }

    let commit = lock
        .get("commit")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if commit.contains("REPLACE_WITH")
        || commit.len() != 40
        || !commit.chars().all(|c| c.is_ascii_hexdigit())
    {
        bail!("canonical lock commit must be a real 40-character Git SHA");
    }

    let captured_at = lock
        .get("captured_at_utc")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if captured_at.contains("REPLACE_WITH") || captured_at.is_empty() {
        bail!("canonical lock captured_at_utc must be a real ISO-8601 timestamp");
    }

    Ok(())
}

fn validate_feature_matrix(root: &Path) -> Result<()> {
    let matrix_path = root.join("compat/feature-matrix.csv");
    let matrix = fs::read_to_string(&matrix_path)?;
    let mut rows = 0usize;
    for (index, line) in matrix.lines().enumerate() {
        if index == 0 || line.trim().is_empty() {
            continue;
        }
        rows += 1;
        if line.contains("<inventory-required>") {
            bail!(
                "feature matrix contains placeholder canonical location on row {}",
                index + 1
            );
        }
    }
    if rows == 0 {
        bail!("feature matrix must contain at least one data row");
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
        "compat/harness/README.md",
        "compat/results/.gitkeep",
        "docs/NATIVE_ONLY_EXECUTION_POLICY.md",
        "docs/AI_IMPLEMENTATION_INSTRUCTIONS.md",
        "docs/AGILANG_NATIVE_CATCHUP_REQUIREMENTS.md",
        "assets/branding/agilang-logo.png",
        "editor/vscode-agilang/package.json",
        "editor/vscode-agilang/syntaxes/agilang.tmLanguage.json",
        "editor/vscode-agilang/syntaxes/ags.tmLanguage.json",
    ] {
        require(&root, rel)?;
    }
    validate_canonical_lock(&root)?;
    validate_feature_matrix(&root)?;
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
    println!(
        "Canonical lock: {}",
        root.join("compat/canonical.lock.json").display()
    );
    println!("Python runtime dependency: NONE");
    println!("AGI backend: REQUIRED");
    println!("AGS frontend: REQUIRED");
    println!("Syntax and logo assets: PRESENT");
    Ok(())
}
