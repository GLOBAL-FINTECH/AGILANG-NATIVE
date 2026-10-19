use std::fs;
use std::path::PathBuf;
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

fn write_manifest(root: &PathBuf, native_cmd: &str, canonical_cmd: &str) {
    let manifest = format!(
        "{{\n  \"schema\": \"AGI-PARITY-1\",\n  \"expected_fixture_count\": 1,\n  \"expected_parity_fixture_count\": 1,\n  \"fixtures\": [\n    {{\n      \"id\": \"exit-check\",\n      \"category\": \"semantic\",\n      \"path\": \"compat/python314/dummy.agi\",\n      \"native_cmd\": \"{}\",\n      \"canonical_cmd\": \"{}\",\n      \"expected\": {{ \"exit_code\": 0 }}\n    }}\n  ]\n}}\n",
        native_cmd.replace('\\', "\\\\").replace('"', "\\\""),
        canonical_cmd
            .replace('\\', "\\\\")
            .replace('"', "\\\""),
    );

    fs::write(root.join("compat/python314/fixtures.json"), manifest).expect("write manifest");
}

#[test]
fn parity_returns_non_zero_on_mismatch() {
    let root = temp_root("mismatch");
    write_manifest(&root, &echo_cmd("native"), &echo_cmd("canonical"));

    let status = Command::new(env!("CARGO_BIN_EXE_agilang-compat-audit"))
        .arg("parity")
        .arg(&root)
        .status()
        .expect("run parity binary");

    assert!(!status.success(), "expected non-zero exit on mismatch");
}

#[test]
fn parity_returns_zero_on_full_match() {
    let root = temp_root("match");
    write_manifest(&root, &echo_cmd("same"), &echo_cmd("same"));

    let status = Command::new(env!("CARGO_BIN_EXE_agilang-compat-audit"))
        .arg("parity")
        .arg(&root)
        .status()
        .expect("run parity binary");

    assert!(status.success(), "expected zero exit when parity passes");
}
