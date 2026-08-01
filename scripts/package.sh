#!/usr/bin/env bash
set -euo pipefail
TARGET="${1:?target required}"
FORMAT="${2:-tar.gz}"
VERSION=$(awk -F'"' '/^version = / {print $2; exit}' Cargo.toml)
NAME="agilang-runtime-${VERSION}-${TARGET}"
STAGE="dist/${NAME}"
rm -rf dist
mkdir -p "$STAGE/bin" "$STAGE/lib" "$STAGE/include"
cp include/agilang_runtime.h "$STAGE/include/"
cp README.md ARCHITECTURE.md LICENSE "$STAGE/" 2>/dev/null || true
BIN="target/${TARGET}/release/agilang-runtime"
[[ "$TARGET" == *windows* ]] && BIN="${BIN}.exe"
cp "$BIN" "$STAGE/bin/"
find "target/${TARGET}/release" -maxdepth 1 -type f \( -name '*agilang_runtime_abi*.so' -o -name '*agilang_runtime_abi*.dylib' -o -name '*agilang_runtime_abi*.dll' -o -name '*agilang_runtime_abi*.a' -o -name '*agilang_runtime_abi*.lib' \) -exec cp {} "$STAGE/lib/" \;
if [[ "$FORMAT" == "zip" ]]; then
  (cd dist && 7z a "${NAME}.zip" "$NAME")
else
  tar -C dist -czf "dist/${NAME}.tar.gz" "$NAME"
fi
