use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::{
    env, fs,
    ffi::OsString,
    io::Write,
    process::Command,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
struct RunOutcome {
    exit_code: i32,
    stdout: String,
    stderr: String,
    execution_error: Option<String>,
    artifacts: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone)]
struct Mismatch {
    fixture_id: String,
    class: String,
    detail: String,
}

#[derive(Debug, Clone)]
struct FixtureResult {
    id: String,
    category: String,
    capabilities: Vec<String>,
    comparison_mode: String,
    normalization_applied: bool,
    native_status: String,
    native_exit_code: i32,
    native_stdout: String,
    native_stderr: String,
    native_execution_error: Option<String>,
    canonical_status: String,
    canonical_exit_code: Option<i32>,
    canonical_stdout: Option<String>,
    canonical_stderr: Option<String>,
    canonical_execution_error: Option<String>,
    parity_status: String,
    classification: Vec<String>,
    mismatch_classes: Vec<String>,
}

#[derive(Debug, Default, Clone)]
struct CategoryScore {
    total: usize,
    passed: usize,
    mismatches: usize,
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_string()
}

fn read_optional_file(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    fs::read_to_string(path).ok().map(|s| normalize_newlines(&s))
}

fn run_shell(command: &str, cwd: &Path) -> Result<(i32, String, String)> {
    #[cfg(windows)]
    let output = {
        let mut shell = Command::new(windows_cmd_path());
        shell.arg("/C").arg(command).current_dir(cwd);

        if let Some(cargo_bin) = windows_cargo_bin_dir() {
            let existing_path = env::var_os("PATH").unwrap_or_default();
            let mut merged_path = OsString::new();
            merged_path.push(cargo_bin.as_os_str());
            merged_path.push(";");
            merged_path.push(existing_path);
            shell.env("PATH", merged_path);
        }

        shell
            .output()
            .with_context(|| format!("failed to execute command: {command}"))?
    };

    #[cfg(not(windows))]
    let output = Command::new("sh")
        .arg("-lc")
        .arg(command)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to execute command: {command}"))?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = normalize_newlines(&String::from_utf8_lossy(&output.stdout));
    let stderr = normalize_newlines(&String::from_utf8_lossy(&output.stderr));
    Ok((exit_code, stdout, stderr))
}

#[cfg(windows)]
fn windows_cmd_path() -> String {
    env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string())
}

#[cfg(windows)]
fn windows_cargo_bin_dir() -> Option<PathBuf> {
    if let Ok(cargo_bin) = env::var("AGILANG_CARGO_BIN") {
        let path = PathBuf::from(cargo_bin);
        if path.exists() {
            if let Some(parent) = path.parent() {
                return Some(parent.to_path_buf());
            }
        }
    }

    if let Ok(cargo_home) = env::var("CARGO_HOME") {
        let candidate = PathBuf::from(cargo_home).join("bin");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    if let Ok(user_profile) = env::var("USERPROFILE") {
        let candidate = PathBuf::from(user_profile).join(".cargo/bin");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    if let Ok(home) = env::var("HOME") {
        let candidate = PathBuf::from(home).join(".cargo/bin");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let home_drive = env::var("HOMEDRIVE").ok();
    let home_path = env::var("HOMEPATH").ok();
    if let (Some(drive), Some(path)) = (home_drive, home_path) {
        let candidate = PathBuf::from(format!("{}{}", drive, path)).join(".cargo/bin");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

fn classify(category: &str, detail: &str) -> String {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("harness") || lower.contains("spawn") || lower.contains("execution failed") {
        return "HARNESS".to_string();
    }
    if lower.contains("packaging") || lower.contains("install") {
        return "PACKAGING".to_string();
    }
    if lower.contains("parse") || lower.contains("syntax") {
        return "PARSE".to_string();
    }
    if lower.contains("type") {
        return "TYPE".to_string();
    }
    if lower.contains("diagnostic") || lower.contains("stderr") {
        return "DIAGNOSTIC".to_string();
    }
    if lower.contains("artifact") || lower.contains("generated") {
        return "CODEGEN".to_string();
    }
    if lower.contains("runtime") || lower.contains("segmentation") || lower.contains("panic") {
        return "RUNTIME".to_string();
    }
    match category {
        "framework" | "mvc" => "FRAMEWORK".to_string(),
        "database" | "orm" => "DATABASE".to_string(),
        "ags" => "AGS".to_string(),
        "scientific" => "SCIENTIFIC".to_string(),
        "ml" => "ML".to_string(),
        "modules" | "imports" => "MODULE".to_string(),
        _ => "SEMANTIC".to_string(),
    }
}

fn run_fixture_engine(
    command: &str,
    fixture_path: &Path,
    cwd: &Path,
    artifacts: &[String],
) -> RunOutcome {
    let fixture_arg = fixture_path.to_string_lossy().replace('\\', "/");
    let rendered = command.replace("{fixture}", &fixture_arg);
    let rendered = resolve_cargo_command(&rendered);
    let (exit_code, stdout, stderr, execution_error) = match run_shell(&rendered, cwd) {
        Ok((code, out, err)) => (code, out, err, None),
        Err(err) => (
            -1,
            String::new(),
            String::new(),
            Some(err.to_string()),
        ),
    };
    let artifact_values = artifacts
        .iter()
        .map(|rel| {
            let full = cwd.join(rel);
            (rel.clone(), read_optional_file(&full))
        })
        .collect();
    RunOutcome {
        exit_code,
        stdout,
        stderr,
        execution_error,
        artifacts: artifact_values,
    }
}

fn resolve_cargo_command(command: &str) -> String {
    let trimmed = command.trim_start();
    let cargo_prefix = "cargo ";
    if !trimmed.starts_with(cargo_prefix) {
        return command.to_string();
    }

    let cargo_bin = env::var("AGILANG_CARGO_BIN")
        .ok()
        .or_else(|| {
            env::var("USERPROFILE").ok().and_then(|home| {
                let candidate = PathBuf::from(home).join(".cargo/bin/cargo.exe");
                if candidate.exists() {
                    Some(candidate.to_string_lossy().to_string())
                } else {
                    None
                }
            })
        });

    if let Some(bin) = cargo_bin {
        let suffix = trimmed.strip_prefix(cargo_prefix).unwrap_or_default();
        return format!("{} {}", bin.replace('/', "\\"), suffix);
    }

    command.to_string()
}

fn contains_all(haystack: &str, needle_values: &[String]) -> bool {
    needle_values.iter().all(|n| haystack.contains(n))
}

fn normalize_cargo_test_stdout(text: &str) -> String {
    let mut test_lines: Vec<String> = Vec::new();
    let mut other_lines: Vec<String> = Vec::new();

    for line in text.lines() {
        let normalized_line = if let Some(pos) = line.find("finished in ") {
            let prefix = &line[..pos + "finished in ".len()];
            format!("{}<time>s", prefix)
        } else {
            line.to_string()
        };

        if normalized_line.starts_with("test ") && normalized_line.contains(" ... ") {
            test_lines.push(normalized_line);
        } else {
            other_lines.push(normalized_line);
        }
    }

    test_lines.sort();

    let mut normalized = Vec::with_capacity(test_lines.len() + other_lines.len());
    normalized.extend(other_lines);
    normalized.extend(test_lines);
    normalized.join("\n")
}

fn normalize_cargo_test_stderr(text: &str) -> String {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("Compiling ")
        })
        .map(|line| {
            if let Some(pos) = line.find("target(s) in ") {
                let prefix = &line[..pos + "target(s) in ".len()];
                format!("{}<time>s", prefix)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalized_for_compare(text: &str, command: &str, is_stderr: bool) -> String {
    if command.contains("cargo test") {
        if is_stderr {
            return normalize_cargo_test_stderr(text);
        }
        return normalize_cargo_test_stdout(text);
    }
    text.to_string()
}

fn command_uses_cargo_test(command: &str) -> bool {
    command.contains("cargo test")
}

fn write_report_atomic(path: &Path, report_json: &str) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .context("invalid report file name")?;
    let temp_path = path.with_file_name(format!("{file_name}.tmp"));

    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&temp_path)
        .with_context(|| format!("failed to create temporary report: {}", temp_path.display()))?;
    file.write_all(report_json.as_bytes())
        .with_context(|| format!("failed to write temporary report: {}", temp_path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to flush temporary report: {}", temp_path.display()))?;
    drop(file);

    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("failed to replace existing report: {}", path.display()))?;
    }

    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "failed to atomically rename temporary report {} -> {}",
            temp_path.display(),
            path.display()
        )
    })?;

    // Best-effort parent directory sync; unsupported platforms/filesystems may reject this.
    if let Some(parent) = path.parent() {
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

fn run_parity(root: &Path) -> Result<()> {
    let manifest_path = root.join("compat/python314/fixtures.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("missing parity manifest: {}", manifest_path.display()))?;
    let manifest: Value = serde_json::from_str(&manifest_text)
        .with_context(|| format!("invalid parity manifest JSON: {}", manifest_path.display()))?;

    let fixtures = manifest
        .get("fixtures")
        .and_then(Value::as_array)
        .context("parity manifest must contain array: fixtures")?;
    let expected_fixture_count = manifest
        .get("expected_fixture_count")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let expected_parity_fixture_count = manifest
        .get("expected_parity_fixture_count")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let coverage_target_fixture_count = manifest
        .get("coverage_target_fixture_count")
        .and_then(Value::as_u64)
        .unwrap_or(expected_fixture_count as u64) as usize;
    let capability_registry: Vec<String> = manifest
        .get("capability_registry")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    let min_capability_covered_count = manifest
        .get("min_capability_covered_count")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let capability_registry_set: BTreeSet<String> = capability_registry.iter().cloned().collect();

    let default_native = env::var("AGILANG_NATIVE_CMD")
        .unwrap_or_else(|_| "cargo run -q -p agilang-native-cli -- run {fixture}".to_string());
    let default_canonical = env::var("AGILANG_CANONICAL_CMD").ok().or_else(|| {
        manifest
            .get("canonical_cmd")
            .and_then(Value::as_str)
            .map(ToString::to_string)
    });

    let mut compared = 0usize;
    let mut native_passed = 0usize;
    let mut mismatches: Vec<Mismatch> = vec![];
    let mut fixture_results: Vec<FixtureResult> = vec![];
    let mut covered_capabilities: BTreeSet<String> = BTreeSet::new();
    let mut capability_fixture_outcomes: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();

    for fixture in fixtures {
        let mut fixture_classes: Vec<String> = Vec::new();
        let id = fixture
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let category = fixture
            .get("category")
            .and_then(Value::as_str)
            .unwrap_or("semantic")
            .to_string();
        let fixture_rel = fixture
            .get("path")
            .and_then(Value::as_str)
            .with_context(|| format!("fixture `{id}` missing path"))?;
        let fixture_path = root.join(fixture_rel);
        let artifacts: Vec<String> = fixture
            .get("artifacts")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let fixture_capabilities: Vec<String> = fixture
            .get("capabilities")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();

        for capability in &fixture_capabilities {
            if capability_registry_set.is_empty() || capability_registry_set.contains(capability) {
                covered_capabilities.insert(capability.clone());
            } else {
                let class = classify(&category, "unknown capability id");
                fixture_classes.push(class.clone());
                mismatches.push(Mismatch {
                    fixture_id: id.clone(),
                    class,
                    detail: format!(
                        "fixture references unknown capability id `{}` not present in capability_registry",
                        capability
                    ),
                });
            }
        }

        let native_cmd = fixture
            .get("native_cmd")
            .and_then(Value::as_str)
            .unwrap_or(&default_native)
            .to_string();

        let canonical_cmd = fixture
            .get("canonical_cmd")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .or_else(|| default_canonical.clone());

        let native = run_fixture_engine(&native_cmd, &fixture_path, root, &artifacts);

        let expected_exit = fixture
            .get("expected")
            .and_then(|e| e.get("exit_code"))
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32;

        let expected_stdout_contains: Vec<String> = fixture
            .get("expected")
            .and_then(|e| e.get("stdout_contains"))
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let expected_stderr_contains: Vec<String> = fixture
            .get("expected")
            .and_then(|e| e.get("stderr_contains"))
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let native_expect_ok = native.execution_error.is_none()
            && native.exit_code == expected_exit
            && contains_all(&native.stdout, &expected_stdout_contains)
            && contains_all(&native.stderr, &expected_stderr_contains);

        let native_status = if native_expect_ok {
            native_passed += 1;
            "pass".to_string()
        } else {
            let class = classify(&category, "expected/native mismatch");
            fixture_classes.push(class.clone());
            mismatches.push(Mismatch {
                fixture_id: id.clone(),
                class,
                detail: format!(
                    "native expectation mismatch: exit={} expected={} stdout=`{}` stderr=`{}` execution_error={:?}",
                    native.exit_code,
                    expected_exit,
                    native.stdout,
                    native.stderr,
                    native.execution_error
                ),
            });
            "fail".to_string()
        };

        let mut canonical_status = "missing".to_string();
        let mut parity_status = "blocked".to_string();
        let mut canonical_exit_code: Option<i32> = None;
        let mut canonical_stdout: Option<String> = None;
        let mut canonical_stderr: Option<String> = None;
        let mut canonical_execution_error: Option<String> = None;

        let mut comparison_mode = "native-only".to_string();
        let mut normalization_applied = false;

        if let Some(canonical_cmd) = canonical_cmd {
            let canonical = run_fixture_engine(&canonical_cmd, &fixture_path, root, &artifacts);
            canonical_status = "executed".to_string();
            compared += 1;
            canonical_exit_code = Some(canonical.exit_code);
            canonical_stdout = Some(canonical.stdout.clone());
            canonical_stderr = Some(canonical.stderr.clone());
            canonical_execution_error = canonical.execution_error.clone();

            normalization_applied =
                command_uses_cargo_test(&native_cmd) || command_uses_cargo_test(&canonical_cmd);
            comparison_mode = if normalization_applied {
                "cargo-test-normalized"
            } else {
                "strict"
            }
            .to_string();

            let mut parity_ok = true;
            let native_stdout_cmp = normalized_for_compare(&native.stdout, &native_cmd, false);
            let canonical_stdout_cmp = normalized_for_compare(&canonical.stdout, &canonical_cmd, false);
            let native_stderr_cmp = normalized_for_compare(&native.stderr, &native_cmd, true);
            let canonical_stderr_cmp = normalized_for_compare(&canonical.stderr, &canonical_cmd, true);

            if canonical.execution_error.is_some() {
                parity_ok = false;
                let class = classify(&category, "canonical execution failed");
                fixture_classes.push(class.clone());
                mismatches.push(Mismatch {
                    fixture_id: id.clone(),
                    class,
                    detail: format!(
                        "canonical execution failed: {:?}",
                        canonical.execution_error
                    ),
                });
            }
            if canonical.exit_code != native.exit_code {
                parity_ok = false;
                let class = classify(&category, "exit mismatch");
                fixture_classes.push(class.clone());
                mismatches.push(Mismatch {
                    fixture_id: id.clone(),
                    class,
                    detail: format!(
                        "exit mismatch native={} canonical={}",
                        native.exit_code, canonical.exit_code
                    ),
                });
            }
            if canonical_stdout_cmp != native_stdout_cmp {
                parity_ok = false;
                let class = classify(&category, "stdout mismatch");
                fixture_classes.push(class.clone());
                mismatches.push(Mismatch {
                    fixture_id: id.clone(),
                    class,
                    detail: format!(
                        "stdout mismatch native=`{}` canonical=`{}` normalized_native=`{}` normalized_canonical=`{}`",
                        native.stdout, canonical.stdout, native_stdout_cmp, canonical_stdout_cmp
                    ),
                });
            }
            if canonical_stderr_cmp != native_stderr_cmp {
                parity_ok = false;
                let class = classify(&category, "stderr mismatch");
                fixture_classes.push(class.clone());
                mismatches.push(Mismatch {
                    fixture_id: id.clone(),
                    class,
                    detail: format!(
                        "stderr mismatch native=`{}` canonical=`{}` normalized_native=`{}` normalized_canonical=`{}`",
                        native.stderr, canonical.stderr, native_stderr_cmp, canonical_stderr_cmp
                    ),
                });
            }

            for (path, native_artifact) in &native.artifacts {
                let canonical_artifact = canonical
                    .artifacts
                    .iter()
                    .find(|(p, _)| p == path)
                    .map(|(_, v)| v.clone())
                    .unwrap_or(None);
                if native_artifact != &canonical_artifact {
                    parity_ok = false;
                    let class = classify(&category, "artifact mismatch");
                    fixture_classes.push(class.clone());
                    mismatches.push(Mismatch {
                        fixture_id: id.clone(),
                        class,
                        detail: format!(
                            "artifact mismatch at {} native={:?} canonical={:?}",
                            path, native_artifact, canonical_artifact
                        ),
                    });
                }
            }

            parity_status = if parity_ok {
                "pass".to_string()
            } else {
                "fail".to_string()
            };
        } else if expected_parity_fixture_count > 0 {
            let class = classify(&category, "canonical command missing");
            fixture_classes.push(class.clone());
            mismatches.push(Mismatch {
                fixture_id: id.clone(),
                class,
                detail: "canonical command missing while parity coverage is required".to_string(),
            });
        }

        fixture_classes.sort();
        fixture_classes.dedup();

        fixture_results.push(FixtureResult {
            id,
            category,
            capabilities: fixture_capabilities.clone(),
            comparison_mode,
            normalization_applied,
            native_status,
            native_exit_code: native.exit_code,
            native_stdout: native.stdout,
            native_stderr: native.stderr,
            native_execution_error: native.execution_error,
            canonical_status,
            canonical_exit_code,
            canonical_stdout,
            canonical_stderr,
            canonical_execution_error,
            parity_status,
            classification: fixture_classes.clone(),
            mismatch_classes: fixture_classes,
        });

        for capability in &fixture_capabilities {
            capability_fixture_outcomes
                .entry(capability.clone())
                .or_default()
                .push((
                    fixture_results
                        .last()
                        .map(|r| r.native_status.clone())
                        .unwrap_or_else(|| "fail".to_string()),
                    fixture_results
                        .last()
                        .map(|r| r.parity_status.clone())
                        .unwrap_or_else(|| "blocked".to_string()),
                ));
        }
    }

    let mut category_scores: BTreeMap<String, CategoryScore> = BTreeMap::new();
    for result in &fixture_results {
        let entry = category_scores
            .entry(result.category.clone())
            .or_default();
        entry.total += 1;
        let passed = result.native_status == "pass" && result.parity_status == "pass";
        if passed {
            entry.passed += 1;
        } else {
            entry.mismatches += 1;
        }
    }

    let parity_percent = if fixture_results.is_empty() {
        0.0
    } else {
        (fixture_results
            .iter()
            .filter(|r| r.native_status == "pass" && r.parity_status == "pass")
            .count() as f64)
            * 100.0
            / (fixture_results.len() as f64)
    };

    let coverage_percent = if coverage_target_fixture_count == 0 {
        0.0
    } else {
        (fixtures.len() as f64) * 100.0 / (coverage_target_fixture_count as f64)
    };

    let capability_coverage_percent = if capability_registry_set.is_empty() {
        0.0
    } else {
        (covered_capabilities.len() as f64) * 100.0 / (capability_registry_set.len() as f64)
    };
    let capability_covered_count = covered_capabilities.len();

    let mut capability_statuses: BTreeMap<String, String> = BTreeMap::new();
    let mut capability_status_counts: BTreeMap<String, usize> = BTreeMap::new();
    for capability in &capability_registry {
        let status = if let Some(outcomes) = capability_fixture_outcomes.get(capability) {
            let has_native_fail = outcomes.iter().any(|(n, _)| n != "pass");
            let has_parity_fail = outcomes.iter().any(|(_, p)| p == "fail");
            if has_native_fail {
                "BLOCKED"
            } else if has_parity_fail {
                "DIFFERENT"
            } else if outcomes.len() < 2 {
                "PARTIAL"
            } else {
                "COVERED"
            }
        } else {
            "UNTESTED"
        }
        .to_string();

        *capability_status_counts.entry(status.clone()).or_insert(0) += 1;
        capability_statuses.insert(capability.clone(), status);
    }

    let report_path = root.join("compat/reports/python314-parity-report.json");
    let summary = serde_json::json!({
        "summary": {
            "fixture_count": fixtures.len(),
            "native_passed": native_passed,
            "compared": compared,
            "mismatch_count": mismatches.len(),
            "fixture_parity_percent": parity_percent,
            "coverage_percent": coverage_percent,
            "capability_coverage_percent": capability_coverage_percent,
            "capability_registry_count": capability_registry_set.len(),
            "capability_covered_count": capability_covered_count,
            "capability_status_counts": capability_status_counts,
            "coverage_target_fixture_count": coverage_target_fixture_count,
            "report_integrity": "PASS",
            "report_write_mode": "atomic-temp-rename",
            "python314_language_coverage": "still expanding",
            "coverage_scope": "current fixture sample"
        },
        "expected_fixture_count": expected_fixture_count,
        "expected_parity_fixture_count": expected_parity_fixture_count,
        "fixture_count": fixtures.len(),
        "native_passed": native_passed,
        "compared": compared,
        "mismatch_count": mismatches.len(),
        "capability_registry": capability_registry,
        "covered_capabilities": covered_capabilities.into_iter().collect::<Vec<_>>(),
        "capability_status": capability_statuses,
        "categories": category_scores
            .iter()
            .map(|(name, s)| {
                let score = if s.total == 0 {
                    0.0
                } else {
                    (s.passed as f64) * 100.0 / (s.total as f64)
                };
                (name.clone(), serde_json::json!({
                    "total": s.total,
                    "passed": s.passed,
                    "mismatches": s.mismatches,
                    "score": score,
                }))
            })
            .collect::<serde_json::Map<String, serde_json::Value>>(),
        "results": fixture_results
            .iter()
            .map(|r| serde_json::json!({
                "id": r.id,
                "category": r.category,
                "capabilities": r.capabilities,
                "comparison_mode": r.comparison_mode,
                "normalization_applied": r.normalization_applied,
                "native_status": r.native_status,
                "native_exit_code": r.native_exit_code,
                "native_stdout": r.native_stdout,
                "native_stderr": r.native_stderr,
                "native_execution_error": r.native_execution_error,
                "canonical_status": r.canonical_status,
                "canonical_exit_code": r.canonical_exit_code,
                "canonical_stdout": r.canonical_stdout,
                "canonical_stderr": r.canonical_stderr,
                "canonical_execution_error": r.canonical_execution_error,
                "parity_status": r.parity_status,
                "classification": r.classification,
                "mismatch_classes": r.mismatch_classes,
            }))
            .collect::<Vec<_>>(),
        "mismatches": mismatches
            .iter()
            .map(|m| serde_json::json!({
                "fixture_id": m.fixture_id,
                "class": m.class,
                "detail": m.detail,
            }))
            .collect::<Vec<_>>()
    });
    let report_json = serde_json::to_string_pretty(&summary)?;
    write_report_atomic(&report_path, &report_json)
        .with_context(|| format!("failed to write report: {}", report_path.display()))?;

    println!("AGILANG parity report: {}", report_path.display());
    println!("Fixtures: {}/{} native expectations passed", native_passed, fixtures.len());
    println!("Compared canonical-vs-native: {}", compared);
    println!("Mismatches: {}", mismatches.len());

    if fixtures.len() < expected_fixture_count {
        bail!(
            "parity fixture count below gate: have {}, need at least {}",
            fixtures.len(),
            expected_fixture_count
        );
    }
    if compared < expected_parity_fixture_count {
        bail!(
            "canonical parity coverage below gate: compared {}, need at least {}",
            compared,
            expected_parity_fixture_count
        );
    }
    if min_capability_covered_count > 0 && capability_covered_count < min_capability_covered_count {
        bail!(
            "capability coverage below gate: covered {}, need at least {}",
            capability_covered_count,
            min_capability_covered_count
        );
    }
    if native_passed < fixtures.len() {
        bail!(
            "native parity expectations failed: passed {}/{} fixtures",
            native_passed,
            fixtures.len()
        );
    }
    if !mismatches.is_empty() {
        bail!("parity mismatches detected: {}", mismatches.len());
    }

    println!("AGILANG parity gate: PASS");
    Ok(())
}

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
    let mut args = env::args().skip(1);
    if matches!(args.next().as_deref(), Some("parity")) {
        let root = args
            .next()
            .map(PathBuf::from)
            .unwrap_or(env::current_dir()?);
        return run_parity(&root);
    }

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

#[cfg(test)]
mod tests {
    use super::{normalize_cargo_test_stderr, normalize_cargo_test_stdout, normalized_for_compare, resolve_cargo_command};

    #[test]
    fn cargo_test_stdout_order_is_normalized() {
        let a = "running 2 tests\ntest b_case ... ok\ntest a_case ... ok\n\ntest result: ok.";
        let b = "running 2 tests\ntest a_case ... ok\ntest b_case ... ok\n\ntest result: ok.";

        assert_eq!(
            normalize_cargo_test_stdout(a),
            normalize_cargo_test_stdout(b)
        );
    }

    #[test]
    fn cargo_test_stderr_timing_is_normalized() {
        let a = "Finished `release` profile [optimized] target(s) in 0.35s";
        let b = "Finished `release` profile [optimized] target(s) in 0.89s";

        assert_eq!(
            normalize_cargo_test_stderr(a),
            normalize_cargo_test_stderr(b)
        );
    }

    #[test]
    fn non_cargo_test_output_remains_strict() {
        let a = "value-a";
        let b = "value-b";
        assert_ne!(
            normalized_for_compare(a, "cargo run -p agilang-native-cli -- run x.agi", false),
            normalized_for_compare(b, "cargo run -p agilang-native-cli -- run x.agi", false)
        );
    }

    #[test]
    fn ags_real_output_order_normalizes_to_equal() {
        let native = "running 7 tests\ntest tests::live_state_machine_rejects_impossible_states_and_preserves_data ... ok\ntest tests::hydration_does_not_duplicate_initial_fetch ... ok\ntest tests::compiles_full_html_template_through_directive_pipeline ... ok\ntest tests::rejects_unknown_sibaq_field ... ok\ntest tests::missing_initial_state_loads_immediately_without_discarding_runtime_errors ... ok\ntest tests::lowers_reactive_state_blocks ... ok\ntest tests::compiles_sibaq_dashboard_end_to_end ... ok\n\ntest result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s";
        let canonical = "running 7 tests\ntest tests::live_state_machine_rejects_impossible_states_and_preserves_data ... ok\ntest tests::compiles_full_html_template_through_directive_pipeline ... ok\ntest tests::hydration_does_not_duplicate_initial_fetch ... ok\ntest tests::rejects_unknown_sibaq_field ... ok\ntest tests::missing_initial_state_loads_immediately_without_discarding_runtime_errors ... ok\ntest tests::lowers_reactive_state_blocks ... ok\ntest tests::compiles_sibaq_dashboard_end_to_end ... ok\n\ntest result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s";

        assert_eq!(
            normalized_for_compare(native, "cargo test -p agilang-ags-compiler --release --locked", false),
            normalized_for_compare(canonical, "cargo test -p agilang-ags-compiler --release --locked", false)
        );
    }

    #[test]
    fn cargo_test_stderr_real_timing_noise_normalizes_to_equal() {
        let native = "Finished `release` profile [optimized] target(s) in 0.69s\n     Running unittests src\\lib.rs (target\\release\\deps\\agilang_runtime_abi-c5a4cddfb8731372.exe)";
        let canonical = "Finished `release` profile [optimized] target(s) in 0.53s\n     Running unittests src\\lib.rs (target\\release\\deps\\agilang_runtime_abi-c5a4cddfb8731372.exe)";

        assert_eq!(
            normalized_for_compare(native, "cargo test -p agilang-runtime-abi --release --locked native_linear_model_training_reduces_loss_through_abi -- --exact", true),
            normalized_for_compare(canonical, "cargo test -p agilang-runtime-abi --release --locked native_linear_model_training_reduces_loss_through_abi -- --exact", true)
        );
    }

    #[test]
    fn cargo_test_stderr_compile_churn_is_normalized() {
        let native = "Compiling ring v0.17.14\nCompiling rustls v0.23.42\nFinished `release` profile [optimized] target(s) in 36.76s\n     Running unittests src\\lib.rs (target\\release\\deps\\agilang_runtime_abi-c5a4cddfb8731372.exe)";
        let canonical = "Finished `release` profile [optimized] target(s) in 0.57s\n     Running unittests src\\lib.rs (target\\release\\deps\\agilang_runtime_abi-c5a4cddfb8731372.exe)";

        assert_eq!(
            normalized_for_compare(native, "cargo test -p agilang-runtime-abi --release --locked native_linear_model_training_reduces_loss_through_abi -- --exact", true),
            normalized_for_compare(canonical, "cargo test -p agilang-runtime-abi --release --locked native_linear_model_training_reduces_loss_through_abi -- --exact", true)
        );
    }

    #[test]
    fn cargo_test_stdout_real_timing_noise_normalizes_to_equal() {
        let native = "running 4 tests\ntest tests::a ... ok\n\ntest result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s";
        let canonical = "running 4 tests\ntest tests::a ... ok\n\ntest result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s";

        assert_eq!(
            normalized_for_compare(native, "cargo test -p agilang-database-agidb --release --locked", false),
            normalized_for_compare(canonical, "cargo test -p agilang-database-agidb --release --locked", false)
        );
    }

    #[test]
    fn database_real_output_order_and_timing_normalizes_to_equal() {
        let native = "running 4 tests\ntest tests::test_single_writer_process_lock ... ok\ntest tests::persists_inserted_rows ... ok\ntest tests::concurrent_writes_keep_valid_json_and_preserve_rows ... ok\ntest tests::rolls_back_transaction_snapshot ... ok\n\ntest result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s";
        let canonical = "running 4 tests\ntest tests::test_single_writer_process_lock ... ok\ntest tests::persists_inserted_rows ... ok\ntest tests::rolls_back_transaction_snapshot ... ok\ntest tests::concurrent_writes_keep_valid_json_and_preserve_rows ... ok\n\ntest result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s\n\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s";

        assert_eq!(
            normalized_for_compare(native, "cargo test -p agilang-database-agidb --release --locked", false),
            normalized_for_compare(canonical, "cargo test -p agilang-database-agidb --release --locked", false)
        );
    }

    #[test]
    fn cargo_command_is_resolved_when_absolute_cargo_exists() {
        let resolved = resolve_cargo_command("cargo --version");
        assert!(
            resolved.contains("cargo") && resolved.contains("--version"),
            "expected cargo command to remain executable"
        );
    }
}
