use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Linker {
    vcvars_path: Option<PathBuf>,
}

impl Linker {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let paths = [
            "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community\\VC\\Auxiliary\\Build\\vcvarsall.bat",
            "C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\BuildTools\\VC\\Auxiliary\\Build\\vcvarsall.bat",
            "C:\\Program Files\\Microsoft Visual Studio\\2022\\Enterprise\\VC\\Auxiliary\\Build\\vcvarsall.bat",
            "C:\\Program Files\\Microsoft Visual Studio\\2022\\Professional\\VC\\Auxiliary\\Build\\vcvarsall.bat",
        ];
        let mut vcvars_path = None;
        for p in paths {
            let path = Path::new(p);
            if path.exists() {
                vcvars_path = Some(path.to_path_buf());
                break;
            }
        }
        Self { vcvars_path }
    }

    pub fn compile_and_link(&self, c_file: &Path, out_exe: &Path, lib_path: &Path) -> Result<()> {
        let c_file_str = c_file.to_string_lossy();
        let out_exe_str = out_exe.to_string_lossy();
        let lib_path_str = lib_path.to_string_lossy();

        let build_dir = out_exe.parent().unwrap_or_else(|| Path::new("build"));
        std::fs::create_dir_all(build_dir).ok();
        let bat_path = build_dir.join("compile.bat");

        let mut bat_content = String::new();
        if let Some(vcvars) = &self.vcvars_path {
            bat_content.push_str(&format!(
                "call \"{}\" amd64\r\n",
                vcvars.to_string_lossy()
            ));
        }
        bat_content.push_str(&format!(
            "cl.exe /O2 /Fe:\"{}\" \"{}\" \"{}\" ws2_32.lib userenv.lib ntdll.lib bcrypt.lib advapi32.lib\r\n",
            out_exe_str,
            c_file_str,
            lib_path_str
        ));
        std::fs::write(&bat_path, &bat_content)?;

        let mut cmd = Command::new("cmd.exe");
        cmd.arg("/c");
        cmd.arg(&bat_path);

        let output = cmd
            .output()
            .context("failed to execute C compiler process")?;

        std::fs::remove_file(&bat_path).ok();

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "C compilation failed.\nScript Content:\n{}\nStdout: {}\nStderr: {}",
                bat_content,
                stdout,
                stderr
            );
        }

        // Clean up temporary compilation artifacts (.obj file generated in same folder as out_exe)
        if let Some(parent) = out_exe.parent() {
            if let Some(stem) = out_exe.file_stem() {
                let obj_file = parent.join(format!("{}.obj", stem.to_string_lossy()));
                if obj_file.exists() {
                    std::fs::remove_file(obj_file).ok();
                }
            }
        }

        Ok(())
    }
}
