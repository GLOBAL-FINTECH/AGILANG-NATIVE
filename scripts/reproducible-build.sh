#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-0}"
export TZ=UTC
export LC_ALL=C
cargo build --workspace --release --locked --all-features
cargo run -q -p agilang-pkg -- check .
echo "PASS: deterministic build environment configured (SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH)"
