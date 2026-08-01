#!/bin/bash
# Validate native-only AGILANG implementation
# This script ensures that all required compatibility files and policies are in place

set -e

ROOT="${1:-.}"
ERRORS=0

echo "=== AGILANG Native Compatibility Validation ==="
echo

# Check required policy file
if [ ! -f "$ROOT/docs/NATIVE_ONLY_EXECUTION_POLICY.md" ]; then
    echo "❌ Missing: docs/NATIVE_ONLY_EXECUTION_POLICY.md"
    ERRORS=$((ERRORS+1))
else
    echo "✅ docs/NATIVE_ONLY_EXECUTION_POLICY.md"
fi

# Check AI instructions
if [ ! -f "$ROOT/docs/AI_IMPLEMENTATION_INSTRUCTIONS.md" ]; then
    echo "❌ Missing: docs/AI_IMPLEMENTATION_INSTRUCTIONS.md"
    ERRORS=$((ERRORS+1))
else
    echo "✅ docs/AI_IMPLEMENTATION_INSTRUCTIONS.md"
fi

# Check compat system
if [ ! -f "$ROOT/compat/canonical.lock.json" ]; then
    echo "❌ Missing: compat/canonical.lock.json"
    ERRORS=$((ERRORS+1))
else
    echo "✅ compat/canonical.lock.json"
fi

if [ ! -f "$ROOT/compat/feature-matrix.csv" ]; then
    echo "❌ Missing: compat/feature-matrix.csv"
    ERRORS=$((ERRORS+1))
else
    echo "✅ compat/feature-matrix.csv"
fi

# Check branding
if [ ! -f "$ROOT/assets/branding/agilang-logo.png" ]; then
    echo "⚠️  Warning: assets/branding/agilang-logo.png not found (optional)"
else
    echo "✅ assets/branding/agilang-logo.png"
fi

# Check editor extension
if [ ! -f "$ROOT/editor/vscode-agilang/package.json" ]; then
    echo "❌ Missing: editor/vscode-agilang/package.json"
    ERRORS=$((ERRORS+1))
else
    echo "✅ editor/vscode-agilang/package.json"
fi

# Check for Python dependencies (should be none)
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
