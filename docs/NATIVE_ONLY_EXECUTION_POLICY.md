# AGILANG Native-Only Execution Policy

## Executive Summary

AGILANG is a **native-only platform**. Python is not part of the production stack and must never be invoked by the runtime, compiler, or deployment pipeline.

## Production Architecture

```
AGI Backend         → Backend logic, controllers, services, routes, policies, agents, AIFlow orchestration
AGS Reactive        → Reactive templates, components, state, forms, user interfaces
JavaScript/WASM     → Generated browser output only
Rust Native         → Compiler, runtime, ABI, TLS, networking, cryptography, database, EVM, AI execution
```

## Mandatory Platform Boundary

| Layer            | Responsibility                                                                        |
| ---------------- | ------------------------------------------------------------------------------------- |
| **AGI Backend**  | Controllers, services, routes, policies, agents and AIFlow orchestration              |
| **AGS Frontend** | Reactive templates, components, state, forms and user interfaces                      |
| **JavaScript**   | Generated browser output only                                                         |
| **Rust**         | Compiler, runtime, ABI, TLS, networking, cryptography, database, EVM and AI execution |

## What Python Cannot Do in Production

Python is permanently excluded from:

- Runtime execution
- Backend servers
- Authentication systems
- Compilation
- Project generation
- AI training
- LLM inference
- HTTP or TLS
- Database operations
- EVM execution
- Generated applications
- Package deployment
- Development tooling that ships with the binary

## Historical Reference

The earlier repository implementation may be inspected as a read-only behavioral oracle. Its syntax, fixtures and observable outputs may guide migration, but its Python implementation must not be copied into the native execution path. A canonical comparison harness must compare recorded fixtures or externally invoked canonical releases without linking Python into native applications.

## Development Rules

### Rule 1: No Python Wrappers

Native features must never be implemented as Python wrappers and later described as native.

❌ **Wrong Pattern:**

```
AGI → Python script → PyTorch
```

✅ **Correct Pattern:**

```
AGI → typed compiler → native AIFlow IR → Rust tensor runtime → CPU/CUDA/Vulkan
```

### Rule 2: AI Implementation Path

All native AI functionality must follow this pipeline:

```
AGI training/inference definition
        ↓
AGILANG typed compiler
        ↓
Native AIFlow intermediate representation
        ↓
Rust tensor/training runtime
        ↓
CPU, CUDA, DirectML, Vulkan or supported accelerator
```

### Rule 3: Web Application Path

Complete web applications must follow:

```
AGS browser interface
        ↓
AGI route/controller/service
        ↓
Native framework ABI
        ↓
Rust HTTP/TLS/database runtime
```

### Rule 4: No Silent Python Fallbacks

The runtime must never:

- Invoke Python as a fallback mechanism
- Use Python modules without explicit, documented user action
- Call Python from C FFI without clear user visibility
- Lazy-load Python dependencies at runtime

## Editor and Project Configuration

Every generated AGILANG project automatically receives:

```
.agilang/branding/agilang-logo.png
.vscode/settings.json                 (with file associations)
.vscode/extensions.json               (with editor recommendations)
.agilang/editor/agilang.tmLanguage.json
.agilang/editor/ags.tmLanguage.json
```

File associations registered:

```
*.agi → agilang
*.ags → agilang-ags
```

## Generated Project Requirement

Every generated application must state:

```
This application runs with AGI backend code, AGS reactive frontend
templates, generated browser JavaScript where required, and the Rust
native runtime. Python is not required.
```

## AI Continuation Instructions

Coding AI systems working on this codebase must:

✅ **MUST DO:**

- Implement new features in `.agi`, `.ags`, or Rust
- Expose Rust runtime capabilities through AGI modules
- Generate JavaScript/WASM from AGS for browser output
- Create native C ABIs for platform interop
- Build typed intermediate representations
- Use restrictive Python-to-AGI migration tools as **one-time utilities only**

❌ **MUST NOT:**

- Introduce Python into the compilation pipeline
- Add Python as a runtime dependency for AGI/AGS
- Implement backend features using Python and describe them as native
- Allow Python scripts to run during deployment without explicit user action
- Create Python wrappers around Rust components
- Dynamically invoke Python interpreters from native code
- Ship Python executables with the AGILANG binary

## Compatibility Audit

Run the compatibility audit to verify native-only compliance:

```bash
cargo run -p agilang-compat-audit -- .
```

Expected output:

```
AGILANG native compatibility audit: PASS
Python runtime dependency: NONE
AGI backend: REQUIRED
AGS frontend: REQUIRED
Syntax and logo assets: PRESENT
```

## Versioning and Breaking Changes

- **Major**: Core ABI changes, language syntax, platform architecture
- **Minor**: New native capabilities, additive syntax, optional AGI modules
- **Patch**: Bug fixes, performance improvements, documentation

## Future Milestones

This native-only foundation supports:

1. Versioned C ABI with proper error isolation ✅
2. Immutable value semantics and handle safety ✅
3. Multi-threaded async executor ✅
4. HTTP/TLS networking (Rust-native)
5. Cryptographic provider (Rust-native)
6. Database drivers (Rust-native)
7. EVM execution (Rust-native)
8. AIFlow tensor and training runtime (Rust-native)
9. LLVM AOT/JIT optimization
10. Portable bytecode format

All future work must follow this native-only policy without exception.

---

**Policy Version**: 1.0  
**Effective Date**: 2026-07-21  
**Status**: Mandatory for all contributions
