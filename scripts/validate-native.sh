#!/bin/bash
# Validate native-only AGILANG implementation
# This script ensures that all required compatibility files and policies are in place

set -e

ROOT="${1:-.}"
ERRORS=0

echo "=== AGILANG Native Compatibility Validation ==="
echo

if [ ! -f "$ROOT/docs/NATIVE_ONLY_EXECUTION_POLICY.md" ]; then
    echo "❌ Missing: docs/NATIVE_ONLY_EXECUTION_POLICY.md"
    ERRORS=$((ERRORS+1))
else
    echo "✅ docs/NATIVE_ONLY_EXECUTION_POLICY.md"
fi

if [ ! -f "$ROOT/docs/AI_IMPLEMENTATION_INSTRUCTIONS.md" ]; then
    echo "❌ Missing: docs/AI_IMPLEMENTATION_INSTRUCTIONS.md"
    ERRORS=$((ERRORS+1))
else
    echo "✅ docs/AI_IMPLEMENTATION_INSTRUCTIONS.md"
fi

if [ ! -f "$ROOT/docs/AGILANG_NATIVE_CATCHUP_REQUIREMENTS.md" ]; then
    echo "❌ Missing: docs/AGILANG_NATIVE_CATCHUP_REQUIREMENTS.md"
    ERRORS=$((ERRORS+1))
else
    echo "✅ docs/AGILANG_NATIVE_CATCHUP_REQUIREMENTS.md"
fi

if [ ! -f "$ROOT/compat/canonical.lock.json" ]; then
    echo "❌ Missing: compat/canonical.lock.json"
    ERRORS=$((ERRORS+1))
else
    echo "✅ compat/canonical.lock.json"
    if grep -q "REPLACE_WITH" "$ROOT/compat/canonical.lock.json"; then
        echo "❌ Placeholder values remain in compat/canonical.lock.json"
        ERRORS=$((ERRORS+1))
    fi
fi

if [ ! -f "$ROOT/compat/feature-matrix.csv" ]; then
    echo "❌ Missing: compat/feature-matrix.csv"
    ERRORS=$((ERRORS+1))
else
    echo "✅ compat/feature-matrix.csv"
    if grep -q "<inventory-required>" "$ROOT/compat/feature-matrix.csv"; then
        echo "❌ Placeholder canonical locations remain in compat/feature-matrix.csv"
        ERRORS=$((ERRORS+1))
    fi
fi

if [ ! -f "$ROOT/compat/harness/README.md" ]; then
    echo "❌ Missing: compat/harness/README.md"
    ERRORS=$((ERRORS+1))
else
    echo "✅ compat/harness/README.md"
fi

if [ ! -f "$ROOT/assets/branding/agilang-logo.png" ]; then
    echo "⚠️  Warning: assets/branding/agilang-logo.png not found (optional)"
else
    echo "✅ assets/branding/agilang-logo.png"
fi

if [ ! -f "$ROOT/editor/vscode-agilang/package.json" ]; then
    echo "❌ Missing: editor/vscode-agilang/package.json"
    ERRORS=$((ERRORS+1))
else
    echo "✅ editor/vscode-agilang/package.json"
fi

if grep -q "pyo3\|cpython\|python" "$ROOT/Cargo.toml" 2>/dev/null; then
    echo "❌ ERROR: Python dependencies found in Cargo.toml"
    ERRORS=$((ERRORS+1))
else
    echo "✅ No Python dependencies in Cargo.toml"
fi

echo
if [ $ERRORS -eq 0 ]; then
    echo "✅ AGILANG native compatibility: PASS"
    echo "Python runtime dependency: NONE"
    exit 0
else
    echo "❌ AGILANG native compatibility: FAIL ($ERRORS errors)"
    exit 1
fi
