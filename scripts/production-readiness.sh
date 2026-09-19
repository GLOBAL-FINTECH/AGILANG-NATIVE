#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== AGILANG Native production readiness ==="
echo "Repository: $ROOT"
echo

echo "[1/8] Formatting"
cargo fmt --all --check

echo "[2/8] Static analysis"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "[3/8] Compatibility audit"
cargo run -p agilang-compat-audit --
echo "[4/8] Native-only policy"
./scripts/validate-native.sh .
./scripts/verify-zero-python.sh

echo "[5/8] Full test suite"
cargo test --workspace --all-features
cargo test -p agilang-package --all-features
./scripts/negative-tests.sh

echo "[6/8] Release build"
cargo build --workspace --release --all-features

echo "[7/8] Runtime diagnostics"
cargo run -p agilang-runtime-cli -- doctor

echo "[8/8] Language smoke test"
cargo run -p agilang-native-cli -- check examples/native/hello-native.agi
cargo run -p agilang-native-cli -- run examples/native/hello-native.agi

echo
echo "PASS: AGILANG Native production-readiness gates completed."
