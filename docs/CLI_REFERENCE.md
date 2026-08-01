# AGILANG Native CLI Reference

## Installation

### From Source

```bash
git clone https://github.com/GLOBAL-FINTECH/agilang
cd agilang-native-runtime
cargo build --release
cargo install --path crates/agilang-native-cli
```

### From Release Binary

Download the latest release from [GitHub Releases](https://github.com/GLOBAL-FINTECH/agilang/releases).

## Commands

### `agilang-native build`

Compile an `.agi` file to a native executable.

**Syntax**:

```bash
agilang-native build [OPTIONS] <entry-file>
```

**Options**:

- `--output`, `-o <path>` — Output executable path (default: derived from input)
- `--emit-c` — Generate C code only (don't compile to binary)
- `--keep-gen` — Keep generated files after compilation
- `--verbose`, `-v` — Print detailed build progress
- `--release` — Build with optimizations (default: debug)

**Examples**:

```bash
# Compile main.agi to ./main (or ./main.exe on Windows)
agilang-native build main.agi

# Compile with custom output path
agilang-native build main.agi --output myapp

# Generate C code without compiling
agilang-native build main.agi --emit-c

# Verbose output with optimization
agilang-native build main.agi --verbose --release
```

### `agilang-native run`

Compile and immediately execute an `.agi` file.

**Syntax**:

```bash
agilang-native run [OPTIONS] <entry-file> [ARGS...]
```

**Options**:

- `--verbose`, `-v` — Print compilation progress
- `--release` — Compile with optimizations

**Examples**:

```bash
# Compile and run
agilang-native run main.agi

# Verbose output
agilang-native run main.agi --verbose

# Pass arguments to program
agilang-native run main.agi arg1 arg2
```

### `agilang-native new`

Generate a new AGILANG project.

**Syntax**:

```bash
agilang-native new [OPTIONS] <project-name>
```

**Options**:

- `--template <name>` — Project template: `basic`, `web`, `cli` (default: `basic`)
- `--authors <name>` — Set author name in manifest

**Examples**:

```bash
# Create a basic project
agilang-native new myapp

# Create a web project (AGI backend + AGS templates)
agilang-native new myapp --template web

# Create a CLI tool
agilang-native new myapp --template cli
```

**Generated Structure**:

```
myapp/
├── .agilang/
│   ├── manifest.toml       # Project configuration
│   ├── branding/
│   │   └── agilang-logo.png
│   └── editor/
│       └── settings.json
├── src/
│   ├── main.agi            # Entry point
│   └── lib.agi             # Library code
├── examples/               # Example programs
├── tests/                  # Test files
├── .vscode/
│   ├── settings.json       # File associations
│   └── extensions.json     # Recommended extensions
└── README.md
```

### `agilang-native http-client`

Import the AGILANG HTTP client wrapper into an existing project.

**Syntax**:

```bash
agilang-native http-client [--force]
```

**Options**:

- `--force` - Overwrite existing `app/Http/HttpClient.agi` and `docs/HTTP_CLIENT_RUNTIME_CONTRACT.md`

**Behavior**:

- Creates `app/Http/HttpClient.agi`
- Creates `docs/HTTP_CLIENT_RUNTIME_CONTRACT.md`
- Requires running inside an AGILANG project containing `agilang.toml`

### `agilang-native check`

Type-check an `.agi` file without compiling.

**Syntax**:

```bash
agilang-native check [OPTIONS] <entry-file>
```

**Options**:

- `--verbose`, `-v` — Show detailed diagnostics

**Examples**:

```bash
# Quick syntax and type check
agilang-native check main.agi

# Detailed output
agilang-native check main.agi --verbose
```

### `agilang-native test`

Run tests from a test file.

**Syntax**:

```bash
agilang-native test [OPTIONS] [pattern]
```

**Options**:

- `--verbose`, `-v` — Show test output
- `--release` — Compile tests with optimizations

**Examples**:

```bash
# Run all tests
agilang-native test

# Run tests matching pattern
agilang-native test arithmetic_

# Verbose test output
agilang-native test --verbose
```

### `agilang-native audit`

Verify native-only compliance and compatibility.

**Syntax**:

```bash
agilang-native audit [root-dir]
```

**Examples**:

```bash
# Audit current directory
agilang-native audit

# Audit specific path
agilang-native audit /path/to/project
```

**Output**:

```
AGILANG native compatibility audit: PASS
Python runtime dependency: NONE
AGI backend: REQUIRED
AGS frontend: REQUIRED
Syntax and logo assets: PRESENT
```

### `agilang-native version`

Show version information.

```bash
$ agilang-native version
AGILANG Native Runtime v0.5.0
Native workspace: 0.5.0
Rust compiler: 1.81+
Target: native-only, no Python required
```

### `agilang-native info`

Show system and configuration information.

```bash
$ agilang-native info
AGILANG Native Runtime Environment

Platform:           Windows (x86_64) / Linux (x86_64) / macOS (aarch64)
Rust version:       1.81+
C compiler:         MSVC / GCC / Clang (auto-detected)
Runtime library:    agilang_runtime_abi
Editor support:     VS Code (agilang-language-support)
Python required:    NO
```

### `agilang-native migrate`

Migrate Python AI code to native AGILANG (Phase 5 feature).

**Syntax**:

```bash
agilang-native migrate [OPTIONS] --source <path> --output <path>
```

**Options**:

- `--source <path>` — Python source directory
- `--output <path>` — Output directory for generated .agi files
- `--strict` — Fail on unsupported constructs
- `--report` — Generate migration report

**Examples**:

```bash
# Migrate Python AI training code
agilang-native migrate \
  --source pytorch_models/ \
  --output agi_models/ \
  --report
```

## Exit Codes

| Code | Meaning                    |
| ---- | -------------------------- |
| 0    | Success                    |
| 1    | Compilation error          |
| 2    | Usage error                |
| 3    | File not found             |
| 4    | Permission denied          |
| 5    | Runtime error              |
| 64   | Native compatibility error |

## Environment Variables

| Variable              | Purpose                                          |
| --------------------- | ------------------------------------------------ |
| `AGILANG_HOME`        | Installation directory (auto-detected)           |
| `AGILANG_RELEASE`     | Force release mode (set to 1 to enable)          |
| `AGILANG_VERBOSE`     | Enable verbose logging                           |
| `AGILANG_RUNTIME_LOG` | Log level for runtime (DEBUG, INFO, WARN, ERROR) |

## Tips & Best Practices

### 1. Use `--emit-c` to Inspect Generated Code

```bash
agilang-native build main.agi --emit-c
cat build/generated/main.c
```

### 2. Check for Issues Before Building

```bash
agilang-native check main.agi --verbose
```

### 3. Keep Build Artifacts for Debugging

```bash
agilang-native build main.agi --keep-gen
# Generated C files stay in build/generated/
```

### 4. Verify Native-Only Compliance

```bash
agilang-native audit
# Confirms no Python dependencies
```

## Troubleshooting

### Error: "C compiler not found"

**Cause**: No C compiler installed or not in PATH

**Solution**:

- **Windows**: Install [MSVC Build Tools](https://visualstudio.microsoft.com/downloads/)
- **Linux**: `sudo apt-get install build-essential`
- **macOS**: `xcode-select --install`

### Error: "Runtime library not found"

**Cause**: `agilang_runtime_abi.lib` (Windows) or `.a`/`.so` (Unix) not found

**Solution**:

```bash
cargo build --release -p agilang-runtime-abi
# This builds the runtime library required for linking
```

### Slow Compilation

**Cause**: Debug mode includes debug symbols

**Solution**:

```bash
agilang-native build main.agi --release
# Enables optimizations (slower to compile, faster to run)
```

---

**Document Version**: 1.0  
**Status**: CLI complete for bootstrap phase
