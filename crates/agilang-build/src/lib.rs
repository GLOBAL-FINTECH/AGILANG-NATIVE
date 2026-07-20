use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn build_project(entry_file: &Path, out_exe: &Path, emit_c: bool) -> Result<()> {
    println!("Compiling {}", entry_file.display());

    // 1. Load source and compile to AST
    let source = agilang_source::SourceFile::load(entry_file)
        .with_context(|| format!("failed to load entry file: {}", entry_file.display()))?;

    println!("Generating typed HIR");
    let hir = agilang_compiler::hir(&source).map_err(|errs| {
        for e in &errs {
            eprintln!("{}", e.render(&source));
        }
        anyhow::anyhow!("HIR compilation failed with {} errors", errs.len())
    })?;

    // 2. Generate C code
    println!("Generating native C");
    let c_code = agilang_codegen_c::generate(&hir);

    let build_dir = out_exe.parent().unwrap_or(Path::new("build"));
    let gen_dir = build_dir.join("generated");
    fs::create_dir_all(&gen_dir)?;

    let stem = entry_file
        .file_stem()
        .unwrap_or_else(|| std::ffi::OsStr::new("app"));
    let c_file_path = gen_dir.join(format!("{}.c", stem.to_string_lossy()));
    fs::write(&c_file_path, &c_code)?;

    // 3. Find runtime library
    let runtime_lib = find_runtime_lib()?;

    // 4. Compile and link C file
    println!("Compiling native target");
    println!("Linking AGILANG runtime");
    let linker = agilang_linker::Linker::new();
    linker.compile_and_link(&c_file_path, out_exe, &runtime_lib)?;

    // Clean up C files if emit_c is false
    if !emit_c {
        fs::remove_dir_all(gen_dir).ok();
    }

    // Write manifest
    let manifest_path = build_dir.join("build-manifest.json");
    let manifest_json = format!(
        "{{\n  \"project\": \"{}\",\n  \"target\": \"native\",\n  \"executable\": \"{}\"\n}}\n",
        stem.to_string_lossy(),
        out_exe.to_string_lossy().replace('\\', "/")
    );
    fs::write(manifest_path, manifest_json)?;

    println!("Finished release build");
    println!("Output: {}", out_exe.display());

    Ok(())
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
