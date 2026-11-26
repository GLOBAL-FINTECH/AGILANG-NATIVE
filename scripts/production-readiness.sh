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
cargo test -p agilang-ownership --all-features
cargo test -p agilang-module-resolver --all-features
cargo test -p agilang-debugger --all-features
for agi_file in examples/native/*.agi; do
  cargo run -q -p agilang-fmt -- "$agi_file" --check
done
./scripts/negative-tests.sh

echo "[6/8] Release build"
cargo build --workspace --release --all-features

echo "[7/8] Runtime diagnostics"
cargo run -p agilang-runtime-cli -- doctor

echo "[8/8] Language smoke test"
cargo run -p agilang-native-cli -- check examples/native/hello-native.agi
cargo run -p agilang-native-cli -- run examples/native/hello-native.agi

# Every self-checking native example must compile, run and exit 0. Examples
# exit non-zero when a computed value is wrong, so this catches miscompiles
# (not just compile errors). Examples that need live network access are skipped.
AGILANG_BIN="$ROOT/target/release/agilang"
[ -x "$AGILANG_BIN.exe" ] && AGILANG_BIN="$AGILANG_BIN.exe"
NETWORK_EXAMPLES="http_test.agi"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cp examples/native/*.agi "$work/"
example_failures=0
for example in "$work"/*.agi; do
  name="$(basename "$example")"
  case " $NETWORK_EXAMPLES " in *" $name "*) echo "SKIP (network): $name"; continue ;; esac
  if (cd "$work" && "$AGILANG_BIN" run "$name" >"$name.log" 2>&1); then
    echo "PASS: examples/native/$name"
  else
    echo "FAIL: examples/native/$name"; cat "$work/$name.log"; example_failures=$((example_failures+1))
  fi
done
if (( example_failures )); then echo "FAIL: $example_failures native example(s)"; exit 1; fi

# A freshly generated framework project must pass check and build.
(cd "$work" && "$AGILANG_BIN" new readiness-app >/dev/null && cd readiness-app \
  && "$AGILANG_BIN" check && "$AGILANG_BIN" build)
echo "PASS: generated framework project checks and builds"

echo
echo "PASS: AGILANG Native production-readiness gates completed."
