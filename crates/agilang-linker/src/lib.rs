use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "windows")]
const WINDOWS_SYSTEM_LIBRARIES: &str = "ws2_32.lib userenv.lib ntdll.lib bcrypt.lib advapi32.lib pdh.lib powrprof.lib iphlpapi.lib netapi32.lib secur32.lib ole32.lib oleaut32.lib propsys.lib psapi.lib shell32.lib wbemuuid.lib";

#[derive(Debug, Clone)]
pub struct Linker {
    #[cfg(target_os = "windows")]
    vcvars_path: Option<PathBuf>,
    #[cfg(not(target_os = "windows"))]
    c_compiler: PathBuf,
}

impl Linker {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        {
            let paths = [
                "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community\\VC\\Auxiliary\\Build\\vcvarsall.bat",
                "C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\BuildTools\\VC\\Auxiliary\\Build\\vcvarsall.bat",
                "C:\\Program Files\\Microsoft Visual Studio\\2022\\Enterprise\\VC\\Auxiliary\\Build\\vcvarsall.bat",
                "C:\\Program Files\\Microsoft Visual Studio\\2022\\Professional\\VC\\Auxiliary\\Build\\vcvarsall.bat",
            ];
            let vcvars_path = paths
                .iter()
                .map(Path::new)
                .find(|path| path.exists())
                .map(Path::to_path_buf);
            Self { vcvars_path }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let c_compiler = std::env::var_os("AGILANG_CC")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(detect_unix_c_compiler()));
            Self { c_compiler }
        }
    }

    pub fn compile_and_link(&self, c_file: &Path, out_exe: &Path, lib_path: &Path) -> Result<()> {
        let build_dir = out_exe.parent().unwrap_or_else(|| Path::new("build"));
        let obj_dir = build_dir.join("objects");
        std::fs::create_dir_all(&obj_dir)?;
        if let Some(parent) = out_exe.parent() {
            std::fs::create_dir_all(parent)?;
        }

        #[cfg(target_os = "windows")]
        {
            self.compile_and_link_windows(c_file, out_exe, lib_path, build_dir, &obj_dir)
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.compile_and_link_unix(c_file, out_exe, lib_path, &obj_dir)
        }
    }

    #[cfg(target_os = "windows")]
    fn compile_and_link_windows(
        &self,
        c_file: &Path,
        out_exe: &Path,
        lib_path: &Path,
        build_dir: &Path,
        obj_dir: &Path,
    ) -> Result<()> {
        let stem = out_exe
            .file_stem()
            .unwrap_or_else(|| std::ffi::OsStr::new("app"));
        let obj_file_path = obj_dir.join(format!("{}.obj", stem.to_string_lossy()));
        let bat_path = build_dir.join("compile.bat");

        let mut bat_content = String::new();
        if let Some(vcvars) = &self.vcvars_path {
            bat_content.push_str(&format!("call \"{}\" amd64\r\n", vcvars.display()));
        }
        bat_content.push_str(&format!(
            "cl.exe /nologo /O2 /Fe:\"{}\" /Fo:\"{}\" \"{}\" \"{}\" {}\r\n",
            out_exe.display(),
            obj_file_path.display(),
            c_file.display(),
            lib_path.display(),
            WINDOWS_SYSTEM_LIBRARIES
        ));
        std::fs::write(&bat_path, &bat_content)?;

        let output = Command::new("cmd.exe")
            .arg("/c")
            .arg(&bat_path)
            .output()
            .context("failed to execute MSVC C compiler process")?;
        std::fs::remove_file(&bat_path).ok();

        if !output.status.success() {
            bail!(
                "C compilation failed.\nScript Content:\n{}\nStdout: {}\nStderr: {}",
                bat_content,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    fn compile_and_link_unix(
        &self,
        c_file: &Path,
        out_exe: &Path,
        lib_path: &Path,
        obj_dir: &Path,
    ) -> Result<()> {
        let stem = out_exe
            .file_stem()
            .unwrap_or_else(|| std::ffi::OsStr::new("app"));
        let obj_file = obj_dir.join(format!("{}.o", stem.to_string_lossy()));

        let compile = Command::new(&self.c_compiler)
            .args(["-std=c11", "-O2", "-fPIC", "-c"])
            .arg(c_file)
            .arg("-o")
            .arg(&obj_file)
            .output()
            .with_context(|| {
                format!(
                    "failed to execute Linux C compiler `{}`; install clang or gcc, or set AGILANG_CC",
                    self.c_compiler.display()
                )
            })?;
        if !compile.status.success() {
            bail!(
                "C compilation failed with `{}`.\nStdout: {}\nStderr: {}",
                self.c_compiler.display(),
                String::from_utf8_lossy(&compile.stdout),
                String::from_utf8_lossy(&compile.stderr)
            );
        }

        let link = Command::new(&self.c_compiler)
            .arg(&obj_file)
            .arg(lib_path)
            .args(["-o"])
            .arg(out_exe)
            .args(["-lpthread", "-ldl", "-lm", "-lrt", "-lutil"])
            .output()
            .with_context(|| {
                format!(
                    "failed to execute Linux linker through `{}`",
                    self.c_compiler.display()
                )
            })?;
        if !link.status.success() {
            bail!(
                "native link failed with `{}`.\nStdout: {}\nStderr: {}",
                self.c_compiler.display(),
                String::from_utf8_lossy(&link.stdout),
                String::from_utf8_lossy(&link.stderr)
            );
        }
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
fn detect_unix_c_compiler() -> &'static str {
    for compiler in ["clang", "cc", "gcc"] {
        if Command::new(compiler).arg("--version").output().is_ok() {
            return compiler;
        }
    }
    "cc"
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "windows")]
    use super::WINDOWS_SYSTEM_LIBRARIES;

    #[cfg(target_os = "windows")]
    #[test]
    fn links_resource_discovery_windows_dependencies() {
        for library in [
            "pdh.lib",
            "powrprof.lib",
            "iphlpapi.lib",
            "netapi32.lib",
            "secur32.lib",
            "ole32.lib",
            "oleaut32.lib",
            "propsys.lib",
            "psapi.lib",
            "shell32.lib",
            "wbemuuid.lib",
        ] {
            assert!(WINDOWS_SYSTEM_LIBRARIES.contains(library));
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn selects_a_unix_c_compiler_name() {
        let compiler = super::detect_unix_c_compiler();
        assert!(["clang", "cc", "gcc"].contains(&compiler));
    }
}
