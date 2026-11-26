use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("agilang-compat-audit-{tag}-{nanos}"));
    fs::create_dir_all(dir.join("compat/python314")).expect("create manifest dir");
    fs::create_dir_all(dir.join("compat/reports")).expect("create reports dir");
    dir
}

#[cfg(windows)]
fn echo_cmd(text: &str) -> String {
    format!("cmd /C echo {text}")
}

#[cfg(not(windows))]
fn echo_cmd(text: &str) -> String {
    format!("sh -lc 'printf %s {text}'")
}

fn write_manifest(root: &Path) {
    let native_cmd = echo_cmd("same");
    let canonical_cmd = echo_cmd("same");
    let manifest = format!(
        "{{\n  \"schema\": \"AGI-PARITY-1\",\n  \"expected_fixture_count\": 1,\n  \"expected_parity_fixture_count\": 1,\n  \"coverage_target_fixture_count\": 10,\n  \"capability_registry\": [\"numbers.bigint\", \"strings.unicode\"],\n  \"fixtures\": [\n    {{\n      \"id\": \"schema-fixture\",\n      \"category\": \"syntax\",\n      \"path\": \"compat/python314/dummy.agi\",\n      \"native_cmd\": \"{}\",\n      \"canonical_cmd\": \"{}\",\n      \"capabilities\": [\"numbers.bigint\"],\n      \"expected\": {{ \"exit_code\": 0 }}\n    }}\n  ]\n}}\n",
        native_cmd.replace('\\', "\\\\").replace('"', "\\\""),
        canonical_cmd.replace('\\', "\\\\").replace('"', "\\\""),
    );
    fs::write(root.join("compat/python314/fixtures.json"), manifest).expect("write manifest");
}

#[test]
fn parity_report_contains_wave1_summary_and_category_blocks() {
    let root = temp_root("schema");
    write_manifest(&root);
    let status = Command::new(env!("CARGO_BIN_EXE_agilang-compat-audit"))
        .arg("parity")
        .arg(&root)
        .status()
        .expect("run parity command");
    assert!(
        status.success(),
        "parity command should pass for schema validation fixture"
    );

    let report_path = root.join("compat/reports/python314-parity-report.json");
    let report_text = fs::read_to_string(&report_path).expect("read parity report");
    let report: Value = serde_json::from_str(&report_text).expect("parse parity report json");

    let summary = report.get("summary").expect("summary block present");
    assert!(
        summary.get("fixture_parity_percent").is_some(),
        "summary.fixture_parity_percent present"
    );
    assert!(
        summary.get("coverage_percent").is_some(),
        "summary.coverage_percent present"
    );
    assert!(
        summary.get("capability_coverage_percent").is_some(),
        "summary.capability_coverage_percent present"
    );
    assert!(
        summary.get("capability_status_counts").is_some(),
        "summary.capability_status_counts present"
    );
    assert!(
        summary.get("report_integrity").is_some(),
        "summary.report_integrity present"
    );
    assert!(
        summary.get("report_write_mode").is_some(),
        "summary.report_write_mode present"
    );
    assert!(
        summary.get("coverage_scope").is_some(),
        "summary.coverage_scope present"
    );

    assert!(
        report.get("capability_registry").is_some(),
        "capability_registry present"
    );
    assert!(
        report.get("covered_capabilities").is_some(),
        "covered_capabilities present"
    );
    assert!(
        report.get("capability_status").is_some(),
        "capability_status present"
    );

    let categories = report.get("categories").expect("categories block present");
    assert!(
        categories.get("syntax").is_some(),
        "categories.syntax present"
    );

    let results = report
        .get("results")
        .and_then(Value::as_array)
        .expect("results array present");
    assert!(!results.is_empty(), "results array should not be empty");

    let first = &results[0];
    assert!(
        first.get("capabilities").is_some(),
        "result.capabilities present"
    );
    assert!(
        first.get("comparison_mode").is_some(),
        "comparison_mode present"
    );
    assert!(
        first.get("normalization_applied").is_some(),
        "normalization_applied present"
    );
    assert!(
        first.get("classification").is_some(),
        "classification present"
    );
}
