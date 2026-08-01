use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
const RUNTIME_LIB_NAME: &str = "agilang_runtime_abi.lib";
#[cfg(not(target_os = "windows"))]
const RUNTIME_LIB_NAME: &str = "libagilang_runtime_abi.a";

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

    let source = agilang_source::SourceFile::load(entry_file)
        .with_context(|| format!("failed to load entry file: {}", entry_file.display()))?;

    if verbose {
        println!("Generating typed HIR");
    }
    let hir = agilang_compiler::hir(&source).map_err(|errs| {
        for error in &errs {
            eprintln!("{}", error.render(&source));
        }
        anyhow::anyhow!("HIR compilation failed with {} errors", errs.len())
    })?;

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

    let runtime_lib = find_runtime_lib()?;

    if verbose {
        println!("Compiling native target and linking AGILANG runtime");
        println!("Runtime library: {}", runtime_lib.display());
    }
    let linker = agilang_linker::Linker::new();
    linker.compile_and_link(&c_file_path, out_exe, &runtime_lib)?;

    if !keep_generated {
        fs::remove_dir_all(&gen_dir).ok();
        fs::remove_dir_all(build_dir.join("objects")).ok();
    }

    let manifests_dir = build_dir.join("manifests");
    fs::create_dir_all(&manifests_dir)?;
    let manifest_path = manifests_dir.join("build-manifest.json");
    let manifest_json = format!(
        "{{\n  \"project\": \"{}\",\n  \"target\": \"{}\",\n  \"executable\": \"{}\"\n}}\n",
        stem.to_string_lossy(),
        native_target_name(),
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

pub fn native_target_name() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "x86_64-pc-windows-msvc"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "aarch64-pc-windows-msvc"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "aarch64-unknown-linux-gnu"
    }
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64")
    )))]
    {
        "unsupported-native-target"
    }
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
        "AGILANG runtime library `{RUNTIME_LIB_NAME}` not found. Set AGILANG_RUNTIME_LIB or install it beside the AGILANG toolchain under lib/, runtime/, or a workspace target directory."
    )
}

fn installed_runtime_candidates(exe_path: &Path) -> Vec<PathBuf> {
    let Some(bin_dir) = exe_path.parent() else {
        return Vec::new();
    };
    let mut candidates = vec![
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
    ];

    #[cfg(target_os = "linux")]
    {
        candidates.extend([
            PathBuf::from("/usr/local/lib/agilang").join(RUNTIME_LIB_NAME),
            PathBuf::from("/usr/local/lib/agilang/lib").join(RUNTIME_LIB_NAME),
            PathBuf::from("/usr/lib/agilang").join(RUNTIME_LIB_NAME),
        ]);
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(
                PathBuf::from(home)
                    .join(".local/share/agilang/lib")
                    .join(RUNTIME_LIB_NAME),
            );
        }
    }

    candidates
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
            Some(root.join("bin").join(executable_name())),
            root.clone(),
        )
        .unwrap();

        assert_eq!(resolved, env_lib);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn runtime_resolver_finds_library_in_installed_lib_directory() {
        let root = unique_temp_dir("exe");
        let exe_path = root.join("toolchain").join("bin").join(executable_name());
        let runtime_lib = root
            .join("toolchain")
            .join("lib")
            .join(RUNTIME_LIB_NAME);
        touch(&runtime_lib);

        let resolved = resolve_runtime_lib(None, Some(exe_path), root.clone()).unwrap();

        assert_eq!(
            resolved.canonicalize().unwrap(),
            runtime_lib.canonicalize().unwrap()
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn runtime_resolver_finds_workspace_target_parent() {
        let root = unique_temp_dir("workspace");
        let current_dir = root.join("examples").join("computation").join("src");
        let runtime_lib = root
            .join("target")
            .join("release")
            .join(RUNTIME_LIB_NAME);
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

        assert!(error.to_string().contains("AGILANG runtime library"));
        assert!(error.to_string().contains(RUNTIME_LIB_NAME));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn native_target_matches_host_platform() {
        assert_ne!(native_target_name(), "unsupported-native-target");
    }

    fn executable_name() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            "agilang.exe"
        }
        #[cfg(not(target_os = "windows"))]
        {
            "agilang"
        }
    }
}
