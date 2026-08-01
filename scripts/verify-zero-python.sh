#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

forbidden='(^|/)(\.venv|venv|__pycache__)(/|$)|(^|/)(requirements[^/]*\.txt|Pipfile|pyproject\.toml)$|\.py$|pyo3|cpython|python[0-9.]*-sys'
repository_files="$(git ls-files --cached --others --exclude-standard)"
violations="$(printf '%s\n' "$repository_files" | grep -Ei "$forbidden" || true)"
if [ -n "$violations" ]; then
  echo "Zero-Python policy violation:" >&2
  echo "$violations" >&2
  exit 1
fi
manifests="$(printf '%s\n' "$repository_files" | grep -E '(^|/)Cargo\.(toml|lock)$' || true)"
if [ -n "$manifests" ] && printf '%s\n' "$manifests" | xargs grep -HnEi '(^|[^a-z])(pyo3|cpython|python[0-9.]*-sys)([^a-z]|$)' 2>/dev/null; then
  echo "Python runtime dependency detected" >&2
  exit 1
fi
echo "PASS: production runtime has no Python or embedded CPython dependency"
