#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
failures=0
check_rejected() {
  local name="$1"; local source="$2"; local expected="$3"; local file
  file="$(mktemp /tmp/agilang-negative.XXXXXX.agi)"
  printf '%s\n' "$source" > "$file"
  if cargo run -q -p agilang-native-cli -- check "$file" >"$file.out" 2>&1; then
    echo "FAIL: $name was accepted"; failures=$((failures+1))
  elif ! grep -q "$expected" "$file.out"; then
    echo "FAIL: $name did not report $expected"; cat "$file.out"; failures=$((failures+1))
  else echo "PASS: $name"; fi
  rm -f "$file" "$file.out"
}
check_rejected "type mismatch" 'fn main() -> i32:
    let x: string = 10
    return 0' E2001
check_rejected "missing return" 'fn main() -> i32:
    let x: i32 = 0' E2008
check_rejected "unknown enum variant" 'enum Status:
    Pending
fn main() -> bool:
    return Status.Missing == Status.Pending' E2106
if (( failures )); then echo "FAIL: $failures negative test(s)"; exit 1; fi
echo "PASS: all negative compiler tests"
