#!/usr/bin/env bash
set -euo pipefail

ROOT="${1:-${AGILANG_INSTALL_ROOT:-$HOME/.local/share/agilang}}"
BIN_DIR="$ROOT/bin"
LIB_DIR="$ROOT/lib"
INCLUDE_DIR="$ROOT/include"
RUNTIME_DIR="$ROOT/runtime"
STDLIB_DIR="$ROOT/stdlib"
USER_BIN="${AGILANG_USER_BIN:-$HOME/.local/bin}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLI="$REPO_ROOT/target/release/agilang"
STATIC_LIB="$REPO_ROOT/target/release/libagilang_runtime_abi.a"
SHARED_LIB="$REPO_ROOT/target/release/libagilang_runtime_abi.so"
HEADER="$REPO_ROOT/include/agilang_runtime_abi.h"

if [[ ! -x "$CLI" || ! -f "$STATIC_LIB" ]]; then
  echo "Building AGILANG Linux release toolchain..."
  cargo build --manifest-path "$REPO_ROOT/Cargo.toml" -p agilang-runtime-abi --release
  cargo build --manifest-path "$REPO_ROOT/Cargo.toml" -p agilang-native-cli --release
fi

mkdir -p "$BIN_DIR" "$LIB_DIR" "$INCLUDE_DIR" "$RUNTIME_DIR" "$STDLIB_DIR" "$USER_BIN"
install -m 0755 "$CLI" "$BIN_DIR/agilang"
install -m 0644 "$STATIC_LIB" "$LIB_DIR/libagilang_runtime_abi.a"
if [[ -f "$SHARED_LIB" ]]; then
  install -m 0755 "$SHARED_LIB" "$LIB_DIR/libagilang_runtime_abi.so"
fi
if [[ -f "$HEADER" ]]; then
  install -m 0644 "$HEADER" "$INCLUDE_DIR/agilang_runtime_abi.h"
fi

ln -sfn "$BIN_DIR/agilang" "$USER_BIN/agilang"

cat > "$ROOT/toolchain.json" <<EOF
{
  "toolchain": "AGILANG Native",
  "version": "0.7.0",
  "abi_version": "1.3.0",
  "target": "$(uname -m)-unknown-linux-gnu",
  "compiler": "bin/agilang",
  "runtime_library": "lib/libagilang_runtime_abi.a",
  "runtime_shared_library": "lib/libagilang_runtime_abi.so"
}
EOF

cat > "$RUNTIME_DIR/runtime-manifest.json" <<EOF
{
  "schema": "AGILANG-RUNTIME-1",
  "version": "0.7.0",
  "abi_version": "1.3.0",
  "platform": "linux",
  "architecture": "$(uname -m)",
  "static_library": "../lib/libagilang_runtime_abi.a",
  "shared_library": "../lib/libagilang_runtime_abi.so"
}
EOF

echo "Installed AGILANG Linux toolchain to $ROOT"
echo "Command link: $USER_BIN/agilang"
echo "Ensure $USER_BIN is on PATH."
"$BIN_DIR/agilang" version --verbose
