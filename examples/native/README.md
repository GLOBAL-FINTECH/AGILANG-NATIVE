# AGILANG Native Runtime Examples

This directory contains example AGILANG programs demonstrating the native compiler and runtime.

## Running Examples

```bash
# Compile and run an example
agilang-native run examples/native/hello-native.agi

# See the generated C code
agilang-native build examples/native/hello-native.agi --emit-c

# Build to executable
agilang-native build examples/native/hello-native.agi --output hello
./hello  # or .\hello.exe on Windows
```

## Examples

### `hello-native.agi`

**Purpose**: Demonstrates the native-only platform

**Features**:

- Simple string printing
- Platform messaging
- Native execution confirmation

**Output**:

```
AGILANG Native Runtime v0.5.0
Copyright 2026 Global FinTech

AGI Backend → Native Rust Compiler & Runtime
AGS Templates → Server Rendering + Browser Hydration
Python is not required.
```

**No Python involved. Pure native execution.**

---

### `bootstrap-features.agi`

**Purpose**: Shows the compiler bootstrap features

**Features**:

- Function definitions
- Type annotations
- Variable declarations
- Conditional blocks (if statement structure)
- Output documentation

**What it demonstrates**:

```
✅ Functions (fn)
✅ Type annotations (i32, i64, string, bool)
✅ Variables (let, const)
✅ Return statements
✅ Function calls
✅ Binary operations
✅ String and numeric literals

🔄 In Development (coming in Phase 2)
Control flow (if/else, while, for)
Collections (array, map)
Module system
AGS templates
AIFlow tensor runtime
```

---

### `bootstrap-tests.agi`

**Purpose**: Validates the compiler with a test suite

**Features**:

- Arithmetic operations
- String handling
- Boolean logic
- Function return values
- Test structure

**What it validates**:

- Integer arithmetic (add, subtract, multiply, divide)
- String literals and printing
- Boolean values and conditions
- Function parameters and returns
- Multi-function programs

---

## C Examples

### `smoke.c`

**Purpose**: Validates C ABI integration

**Features**:

- Calls the native AGILANG runtime ABI directly
- Demonstrates value creation and manipulation
- Shows error handling

**Build and run**:

```bash
# (Requires AGILANG runtime library)
# Build instructions specific to your platform
```

---

## Building from Examples

### Start with Hello World

```bash
cp examples/native/hello-native.agi myapp.agi
agilang-native run myapp.agi
```

### Inspect Generated Code

```bash
agilang-native build examples/native/hello-native.agi --emit-c
cat build/generated/hello-native.c
```

This shows you the exact C code the compiler generated — useful for learning how AGILANG compiles to native.

### Modify and Experiment

Edit the `.agi` file, then recompile:

```bash
agilang-native run myapp.agi --verbose
```

The `--verbose` flag shows compilation progress and any type errors.

---

## Platform-Specific Notes

### Windows

```bash
# Run examples
agilang-native run examples\native\hello-native.agi

# Output goes to console
```

### Linux/macOS

```bash
# Run examples
agilang-native run examples/native/hello-native.agi

# Make executables executable
chmod +x ./hello
./hello
```

---

## No Python

These examples prove that AGILANG programs:

- ✅ Compile to native C
- ✅ Link native libraries
- ✅ Execute as native binaries
- ✅ Require zero Python
- ✅ Use zero Python code

Verify with:

```bash
agilang-native audit
# Output: Python runtime dependency: NONE
```

---

## Next Steps

1. **Run an example**: `agilang-native run examples/native/hello-native.agi`
2. **Create your own**: `agilang-native new myapp`
3. **Build a binary**: `agilang-native build examples/native/hello-native.agi --output hello`
4. **Explore the generated C**: Use `--emit-c` to see under the hood

---

**Status**: Examples work with bootstrap compiler (Phase 1-2)

Planned examples coming with each phase:

- **Phase 3**: Web applications (AGI + AGS)
- **Phase 4**: AI/ML with native tensors
- **Phase 5**: Migrated Python code (before/after)
- **Phase 6**: Parity tests with canonical implementation
