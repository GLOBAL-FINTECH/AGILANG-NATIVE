# Getting Started with AGILANG Native

Welcome to AGILANG, a native-only programming platform where Python is **never required**.

## What is AGILANG?

AGILANG is a **modern, compiled programming language** that runs on native platforms (Windows, Linux, macOS) with a design principle:

```
AGI         → Backend logic, services, business rules (AGILANG)
AGS         → Reactive web templates with server rendering (AGILANG)
Generated   → JavaScript/WebAssembly for browser hydration
Runtime     → 100% native Rust with C ABI bindings
```

**Python is permanently excluded** from runtime execution, compilation, deployment, and generated applications.

## Installation

### Option 1: Build from Source

```bash
# Clone the repository
git clone https://github.com/GLOBAL-FINTECH/agilang
cd agilang-native-runtime

# Build the compiler and runtime
cargo build --release

# Install the CLI
cargo install --path crates/agilang-native-cli
```

### Option 2: Download Binary Release

[GitHub Releases](https://github.com/GLOBAL-FINTECH/agilang/releases) — Download the latest precompiled binary for your platform.

## Your First AGILANG Program

### 1. Create a file: `hello.agi`

```agi
fn main() -> i32:
    print("Hello from native AGILANG!")
    return 0
```

### 2. Compile and run

```bash
agilang-native run hello.agi
```

**Output**:

```
Hello from native AGILANG!
```

That's it! No Python required. No interpreter. Just native execution.

## Understanding the Syntax

### Functions

```agi
fn add(a: i32, b: i32) -> i32:
    let sum: i32 = a + b
    return sum

fn main() -> i32:
    let result: i32 = add(5, 3)
    print(result)
    return 0
```

**Key points**:

- Functions use `fn` keyword
- Parameters have type annotations (`:` followed by type)
- Return type specified with `->`
- Function body is indentation-delimited
- `return` statement exits with a value

### Variables

```agi
fn main() -> i32:
    let x: i32 = 42           // Typed variable
    let name: string = "Alice" // String type
    let active: bool = true    // Boolean
    let ratio: f64 = 3.14      // Float

    print(name)
    return 0
```

**Supported types**:

- `i32`, `i64` — Signed integers
- `u32`, `u64` — Unsigned integers
- `f32`, `f64` — Floating point
- `bool` — True/false
- `string` — UTF-8 text
- `void` — No value (for function return type)

### Expressions

```agi
fn main() -> i32:
    let a: i32 = 10
    let b: i32 = 20

    // Arithmetic
    let sum: i32 = a + b
    let diff: i32 = b - a
    let product: i32 = a * 2
    let quotient: i32 = b / 2

    // Comparisons
    let is_greater: bool = b > a      // true
    let is_equal: bool = a == a       // true
    let is_not_equal: bool = a != b   // true

    // Logical operations
    let both_true: bool = true && false   // false
    let either_true: bool = true || false // true

    return 0
```

### Printing Output

```agi
fn main() -> i32:
    print("Single string argument")
    print("Expressions come later in full syntax")
    return 0
```

## Building Your First Project

### Create a project

```bash
agilang-native new myapp
cd myapp
```

This generates:

```
myapp/
├── .agilang/manifest.toml
├── src/
│   └── main.agi
├── .vscode/settings.json
└── README.md
```

### Edit `src/main.agi`

```agi
fn main() -> i32:
    print("My first AGILANG app!")
    return 0
```

### Build and run

```bash
agilang-native build src/main.agi --output myapp

# On Windows:
./myapp.exe

# On Linux/macOS:
./myapp
```

## What You Can Build (Today)

✅ **Console applications** — Print output, compute values, return status codes

✅ **Native libraries** — Compile functions for C/C++ interop

✅ **Audit & validation** — Ensure native-only compliance with `agilang audit`

## What's Coming (Roadmap)

### Phase 2 (Current)

- [ ] `if` / `else` conditional branching
- [ ] `while` / `for` loops
- [ ] `Array<T>` and `Map<K, V>` collections
- [ ] Module system (`use`, `module`)

### Phase 3 (Q3 2026)

- [ ] AGS templates with server-side rendering
- [ ] Browser hydration and reactive updates
- [ ] Form binding and validation

### Phase 4 (Q4 2026)

- [ ] AIFlow tensor runtime (inference)
- [ ] Training kernels with GPU support
- [ ] Model checkpoints and quantization

### Phase 5 (Q1 2027)

- [ ] Python-to-AGI migration tool
- [ ] Artifact conversion utilities

### Phase 6 (Q2 2027)

- [ ] Full compatibility validation
- [ ] Parity testing framework

## Compliance Guarantee

Every AGILANG program:

- ✅ Compiles to native C
- ✅ Links native libraries
- ✅ Executes as native binary
- ✅ Uses zero Python code
- ✅ Requires zero Python runtime
- ✅ Works offline without interpreters

Run the audit to verify:

```bash
agilang-native audit
```

Expected output:

```
AGILANG native compatibility audit: PASS
Python runtime dependency: NONE
AGI backend: REQUIRED
AGS frontend: REQUIRED
Syntax and logo assets: PRESENT
```

## Editor Support

### VS Code

1. Install the **AGILANG Language Support** extension:

   ```
   ext install global-fintech.agilang-language-support
   ```

2. VS Code will automatically recognize `.agi` and `.ags` files

3. Features:
   - ✅ Syntax highlighting
   - ✅ Bracket matching
   - ✅ Indentation rules
   - 🔄 IntelliSense (coming soon)
   - 🔄 Go to definition (coming soon)

## Common Commands

```bash
# Check syntax without compiling
agilang-native check main.agi

# Compile to native binary
agilang-native build main.agi

# Compile and run
agilang-native run main.agi

# Generate C code (inspect what compiles)
agilang-native build main.agi --emit-c

# Verify native-only compliance
agilang-native audit

# Show version and info
agilang-native version
agilang-native info
```

## Next Steps

1. **Install**: Follow installation instructions above
2. **Create**: `agilang-native new myapp`
3. **Edit**: Open `src/main.agi` in VS Code
4. **Run**: `agilang-native run src/main.agi`
5. **Build**: `agilang-native build src/main.agi`
6. **Learn**: Read [AGILANG Language Guide](LANGUAGE_GUIDE.md)
7. **Explore**: Check out [examples/native/](../examples/native/)

## Troubleshooting

### "agilang-native command not found"

```bash
# Ensure cargo is installed, then:
cargo install --path crates/agilang-native-cli
```

### "C compiler not found"

Install a C compiler for your platform:

- **Windows**: [MSVC Build Tools](https://visualstudio.microsoft.com/)
- **Linux**: `sudo apt-get install build-essential`
- **macOS**: `xcode-select --install`

### "Runtime library not found"

Build the runtime:

```bash
cargo build --release -p agilang-runtime-abi
```

## Community & Support

- **GitHub Issues**: [Report bugs](https://github.com/GLOBAL-FINTECH/agilang/issues)
- **Discussions**: [Ask questions](https://github.com/GLOBAL-FINTECH/agilang/discussions)
- **Documentation**: Read the [official docs](https://github.com/GLOBAL-FINTECH/agilang)

## License

AGILANG is licensed under Apache-2.0. See [LICENSE](../LICENSE) for details.

---

**Welcome to native, Python-free development!**

Next: Read [LANGUAGE_GUIDE.md](LANGUAGE_GUIDE.md) for deeper language concepts.
