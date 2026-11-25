use anyhow::{Context, Result};
use std::{env, fs};

fn format_source(input: &str) -> String {
    let mut out = input.lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
    if !out.ends_with('\n') { out.push('\n'); }
    out
}
fn main() -> Result<()> {
    let mut args=env::args().skip(1);
    let path=args.next().context("usage: agilang-fmt <file.agi> [--check]")?;
    let check=args.any(|a| a=="--check");
    let text=fs::read_to_string(&path)?;
    let formatted=format_source(&text);
    if check { anyhow::ensure!(text==formatted, "{} is not formatted", path); println!("formatted: {}", path); }
    else { fs::write(&path, formatted)?; println!("formatted: {}", path); }
    Ok(())
}
