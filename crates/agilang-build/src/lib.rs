use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

const RUNTIME_LIB_NAME: &str = "agilang_runtime_abi.lib";

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

pub fn diagnose_runtime_lib() -> Result<PathBuf> {
    find_runtime_lib()
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
    resolve_runtime_lib(
        std::env::var_os("AGILANG_RUNTIME_LIB").map(PathBuf::from),
        std::env::current_exe().ok(),
        std::env::current_dir()?,
    )
}

fn resolve_runtime_lib(
    env_override: Option<PathBuf>,
    current_exe: Option<PathBuf>,
    current_dir: PathBuf,
) -> Result<PathBuf> {
    if let Some(path) = env_override {
        if path.is_file() {
            return Ok(path);
        }
    }

    if let Some(exe_path) = current_exe {
        for candidate in installed_runtime_candidates(&exe_path) {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    for candidate in workspace_runtime_candidates(&current_dir) {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    bail!(
        "AGILANG runtime library not found. Set AGILANG_RUNTIME_LIB or install {RUNTIME_LIB_NAME} beside agilang.exe, ../lib, ../runtime, or a workspace target directory."
    )
}

fn installed_runtime_candidates(exe_path: &Path) -> Vec<PathBuf> {
    let Some(bin_dir) = exe_path.parent() else {
        return Vec::new();
    };
    vec![
        bin_dir.join(RUNTIME_LIB_NAME),
        bin_dir.join("..").join("lib").join(RUNTIME_LIB_NAME),
        bin_dir.join("..").join("runtime").join(RUNTIME_LIB_NAME),
        bin_dir
            .join("..")
            .join("target")
            .join("release")
            .join(RUNTIME_LIB_NAME),
        bin_dir
            .join("..")
            .join("target")
            .join("debug")
            .join(RUNTIME_LIB_NAME),
    ]
}

fn workspace_runtime_candidates(current_dir: &Path) -> Vec<PathBuf> {
    let mut dir = current_dir.to_path_buf();
    let mut candidates = Vec::new();
    loop {
        candidates.push(dir.join("target").join("release").join(RUNTIME_LIB_NAME));
        candidates.push(dir.join("target").join("debug").join(RUNTIME_LIB_NAME));
        if !dir.pop() {
            break;
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("agilang-build-{name}-{stamp}"))
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"runtime").unwrap();
    }

    #[test]
    fn runtime_resolver_prefers_environment_override() {
        let root = unique_temp_dir("env");
        let env_lib = root.join("custom").join(RUNTIME_LIB_NAME);
        let exe_lib = root.join("bin").join(RUNTIME_LIB_NAME);
        touch(&env_lib);
        touch(&exe_lib);

        let resolved = resolve_runtime_lib(
            Some(env_lib.clone()),
            Some(root.join("bin").join("agilang.exe")),
            root.clone(),
        )
        .unwrap();

        assert_eq!(resolved, env_lib);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn runtime_resolver_finds_library_beside_executable_installation() {
        let root = unique_temp_dir("exe");
        let exe_path = root.join("toolchain").join("bin").join("agilang.exe");
        let runtime_lib = root.join("toolchain").join("lib").join(RUNTIME_LIB_NAME);
        touch(&runtime_lib);

        let resolved = resolve_runtime_lib(None, Some(exe_path), root.clone()).unwrap();

        assert_eq!(resolved.canonicalize().unwrap(), runtime_lib.canonicalize().unwrap());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn runtime_resolver_finds_workspace_target_parent() {
        let root = unique_temp_dir("workspace");
        let current_dir = root.join("examples").join("computation").join("src");
        let runtime_lib = root.join("target").join("release").join(RUNTIME_LIB_NAME);
        touch(&runtime_lib);
        fs::create_dir_all(&current_dir).unwrap();

        let resolved = resolve_runtime_lib(None, None, current_dir).unwrap();

        assert_eq!(resolved, runtime_lib);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn runtime_resolver_returns_clear_error_when_missing() {
        let root = unique_temp_dir("missing");
        fs::create_dir_all(&root).unwrap();

        let error = resolve_runtime_lib(None, None, root.clone()).unwrap_err();

        assert!(error.to_string().contains("AGILANG runtime library not found"));
        fs::remove_dir_all(root).ok();
    }
}
