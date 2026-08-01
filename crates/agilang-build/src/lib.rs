use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn build_project(
    entry_file: &Path,
    out_exe: &Path,
    emit_c: bool,
    keep_generated: bool,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Compiling entry file: {}", entry_file.display());
    }

    // 1. Load source and compile to AST
    let source = agilang_source::SourceFile::load(entry_file)
        .with_context(|| format!("failed to load entry file: {}", entry_file.display()))?;

    if verbose {
        println!("Generating typed HIR");
    }
    let hir = agilang_compiler::hir(&source).map_err(|errs| {
        for e in &errs {
            eprintln!("{}", e.render(&source));
        }
        anyhow::anyhow!("HIR compilation failed with {} errors", errs.len())
    })?;

    // 2. Generate C code
    if verbose {
        println!("Generating native C");
    }
    let c_code = agilang_codegen_c::generate(&hir);

    let build_dir = out_exe.parent().unwrap_or_else(|| Path::new("build"));
    let gen_dir = build_dir.join("generated");
    fs::create_dir_all(&gen_dir)?;

    let stem = entry_file
        .file_stem()
        .unwrap_or_else(|| std::ffi::OsStr::new("app"));
    let c_file_path = gen_dir.join(format!("{}.c", stem.to_string_lossy()));
    fs::write(&c_file_path, &c_code)?;

    if emit_c {
        println!("Emitted C source: {}", c_file_path.display());
        return Ok(());
    }

    // 3. Find runtime library
    let runtime_lib = find_runtime_lib()?;

    // 4. Compile and link C file
    if verbose {
        println!("Compiling native target and linking AGILANG runtime");
    }
    let linker = agilang_linker::Linker::new();
    linker.compile_and_link(&c_file_path, out_exe, &runtime_lib)?;

    // Clean up if keep_generated is false
    if !keep_generated {
        fs::remove_dir_all(&gen_dir).ok();
        fs::remove_dir_all(build_dir.join("objects")).ok();
    }

    // Write manifest under manifests/build-manifest.json
    let manifests_dir = build_dir.join("manifests");
    fs::create_dir_all(&manifests_dir)?;
    let manifest_path = manifests_dir.join("build-manifest.json");
    let manifest_json = format!(
        "{{\n  \"project\": \"{}\",\n  \"target\": \"native\",\n  \"executable\": \"{}\"\n}}\n",
        stem.to_string_lossy(),
        out_exe.to_string_lossy().replace('\\', "/")
    );
    fs::write(manifest_path, manifest_json)?;

    if let Some(project_root) = find_project_root(entry_file) {
        agilang_framework_server::write_framework_manifest(&project_root)?;
    }

    println!("Finished release build");
    println!("Output: {}", out_exe.display());

    Ok(())
}

fn find_project_root(entry_file: &Path) -> Option<PathBuf> {
    let mut dir = entry_file.parent()?.to_path_buf();
    loop {
        if dir.join("agilang.toml").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn find_runtime_lib() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        let release_lib = dir.join("target/release/agilang_runtime_abi.lib");
        if release_lib.exists() {
            return Ok(release_lib);
        }
        let debug_lib = dir.join("target/debug/agilang_runtime_abi.lib");
        if debug_lib.exists() {
            return Ok(debug_lib);
        }
        if !dir.pop() {
            break;
        }
    }
    // Fallback search in parents
    bail!("agilang_runtime_abi.lib not found in target directories")
}
