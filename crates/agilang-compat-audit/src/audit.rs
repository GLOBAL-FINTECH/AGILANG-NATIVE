use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct AuditReport {
    pub passed: bool,
    pub has_python_dependency: bool,
    pub has_agi_support: bool,
    pub has_ags_support: bool,
    pub has_assets: bool,
    pub has_policy: bool,
    pub has_ai_instructions: bool,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
}

pub struct NativeAuditChecker {
    root_dir: PathBuf,
}

impl NativeAuditChecker {
    pub fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }

    pub fn audit(&mut self) -> Result<AuditReport> {
        let mut report = AuditReport {
            passed: true,
            has_python_dependency: false,
            has_agi_support: false,
            has_ags_support: false,
            has_assets: false,
            has_policy: false,
            has_ai_instructions: false,
            issues: Vec::new(),
            warnings: Vec::new(),
        };

        // Check for Python dependencies
        self.check_python_dependencies(&mut report)?;

        // Check for AGI/AGS support
        self.check_language_support(&mut report)?;

        // Check for branding assets
        self.check_assets(&mut report)?;

        // Check for native-only policy
        self.check_policy(&mut report)?;

        // Check for AI instructions
        self.check_ai_instructions(&mut report)?;

        // Determine overall pass/fail
        report.passed = !report.has_python_dependency
            && report.has_agi_support
            && report.has_ags_support
            && report.has_policy
            && report.has_ai_instructions
            && report.issues.is_empty();

        Ok(report)
    }

    fn check_python_dependencies(&mut self, report: &mut AuditReport) -> Result<()> {
        let cargo_files = self.find_files("Cargo.toml")?;

        for cargo_file in cargo_files {
            if let Ok(content) = std::fs::read_to_string(&cargo_file) {
                // Check for Python-related dependencies
                let python_patterns = [
                    "python",
                    "pyo3",
                    "cpython",
                    "pypython",
                ];

                for pattern in &python_patterns {
                    if content.to_lowercase().contains(pattern) {
                        report.has_python_dependency = true;
                        report.issues.push(format!(
                            "Python dependency '{}' found in {}",
                            pattern,
                            cargo_file.display()
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    fn check_language_support(&mut self, report: &mut AuditReport) -> Result<()> {
        // Check for AGI syntax definitions
        let agi_files = self.find_files("*.agi")?;
        if !agi_files.is_empty() {
            report.has_agi_support = true;
        } else {
            report.warnings.push("No .agi files found in project".to_string());
        }

        // Check for AGS syntax definitions
        let ags_files = self.find_files("*.ags")?;
        if !ags_files.is_empty() {
            report.has_ags_support = true;
        } else {
            report.warnings.push("No .ags files found in project".to_string());
        }

        // Check for language configuration files
        let syntax_files = self.find_files("*tmLanguage.json")?;
        if !syntax_files.is_empty() {
            report.has_agi_support = true;
            report.has_ags_support = true;
        }

        Ok(())
    }

    fn check_assets(&mut self, report: &mut AuditReport) -> Result<()> {
        let logo_path = self.root_dir.join("assets/branding/agilang-logo.png");
        if logo_path.exists() {
            report.has_assets = true;
        } else {
            report.warnings.push(format!(
                "Official logo not found at {}",
                logo_path.display()
            ));
        }

        Ok(())
    }

    fn check_policy(&mut self, report: &mut AuditReport) -> Result<()> {
        let policy_path = self.root_dir.join("docs/NATIVE_ONLY_EXECUTION_POLICY.md");
        if policy_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&policy_path) {
                if content.to_lowercase().contains("native-only") {
                    report.has_policy = true;
                }
            }
        } else {
            report.issues.push("NATIVE_ONLY_EXECUTION_POLICY.md not found in docs/".to_string());
        }

        Ok(())
    }

    fn check_ai_instructions(&mut self, report: &mut AuditReport) -> Result<()> {
        let instruction_files = [
            "copilot-instructions.md",
            ".instructions.md",
            "AI_CONTINUATION_RULES.md",
            ".ai-instructions.md",
        ];

        for filename in &instruction_files {
            let path = self.root_dir.join(filename);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.to_lowercase().contains("python")
                        && content.to_lowercase().contains("must not")
                    {
                        report.has_ai_instructions = true;
                        break;
                    }
                }
            }
        }

        if !report.has_ai_instructions {
            report.warnings.push("AI continuation instructions not found. Create copilot-instructions.md with Python-exclusion rules.".to_string());
        }

        Ok(())
    }

    fn find_files(&self, pattern: &str) -> Result<Vec<PathBuf>> {
        let mut results = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.root_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && self.matches_pattern(&path, pattern) {
                    results.push(path);
                }
                if path.is_dir() && !self.is_ignored_dir(&path) {
                    results.extend(self.find_files_recursive(&path, pattern)?);
                }
            }
        }

        Ok(results)
    }

    fn find_files_recursive(&self, dir: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
        let mut results = Vec::new();

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && self.matches_pattern(&path, pattern) {
                    results.push(path);
                }
                if path.is_dir() && !self.is_ignored_dir(&path) {
                    results.extend(self.find_files_recursive(&path, pattern)?);
                }
            }
        }

        Ok(results)
    }

    fn matches_pattern(&self, path: &Path, pattern: &str) -> bool {
        if let Some(filename) = path.file_name() {
            if let Some(name_str) = filename.to_str() {
                if pattern.contains('*') {
                    let parts: Vec<&str> = pattern.split('*').collect();
                    if parts.len() == 2 {
                        return name_str.starts_with(parts[0]) && name_str.ends_with(parts[1]);
                    }
                } else {
                    return name_str == pattern;
                }
            }
        }
        false
    }

    fn is_ignored_dir(&self, path: &Path) -> bool {
        if let Some(name) = path.file_name() {
            if let Some(name_str) = name.to_str() {
                matches!(
                    name_str,
                    "target" | ".git" | "node_modules" | ".venv" | "__pycache__"
                )
            } else {
                false
            }
        } else {
            false
        }
    }
}
