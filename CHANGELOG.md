# Next

- Added an explicit production-readiness gate covering formatting, Clippy, compatibility audit, native-only policy, full workspace tests, release build, runtime diagnostics, and an actual `.agi` compile-and-execute smoke test.
- Added macOS x86_64 and Apple Silicon native toolchain target support in the build/runtime manifest layer.
- Updated the native hello-world smoke example to the current 0.7.0 runtime version.
- Fixed native codegen for strings: `+` now concatenates (`agi_string_concat`) and `==`, `!=`, `<`, `<=`, `>`, `>=` compare contents (`agi_string_compare`) instead of emitting C pointer arithmetic and pointer comparison.
- Fixed diagnostics from imported modules being rendered against the entry file (wrong path, line and caret). Spans now carry a file id and imported sources are registered for rendering.
- Framework projects (an `agilang.toml` with a `[routes]` section, as created by `agilang new`) no longer try to compile `bootstrap/app.agi` as a standalone native program: `agilang check` and `agilang build` validate routes and views, and `agilang run` starts `agilang serve`. Passing an explicit file (`agilang build src/main.agi`) still compiles natively.
- Fixed `agilang-package` failing to compile (missing semicolon), which broke `cargo test --workspace`.
- Fixed Linux build of `agilang-build` (`installed_runtime_candidates` mutated a non-`mut` vector).
- Health endpoint and LSP `serverInfo` now report the real toolchain version instead of a hardcoded `0.5.0`; the generated `ApplicationServiceTest` no longer asserts a version that contradicts its own service.
- Synchronized `Cargo.lock` with the 0.8.0 workspace manifests.

# Changelog

## 0.8.0 - Native production hardening

- Added deterministic package manifest/lock tooling via agilang-pkg.
- Added native REPL, negative compiler tests and reproducible-build gates.
- Added debugger launch configuration and tagged multi-platform release workflow.
- Expanded production CI gates for package and compiler rejection tests.


## 0.6.0 — Native-only stack and permanent language identity

- Declared Python prohibited from the production runtime, backend, compiler host, model training, model inference, project generation and generated applications.
- Defined AGI as backend/application logic and AGS as the reactive frontend source surface.
- Allowed JavaScript only as generated browser output where the browser requires it.
- Added a Rust-native compatibility audit executable.
- Added the official AGILANG logo to the toolchain and every generated project.
- Added `.agi` and `.ags` language identifiers, TextMate syntax grammars, VS Code package metadata, file associations and editor recommendations.
- Updated the native project generator to emit branding and syntax-highlighting assets automatically.

## 0.2.0 - 2026-07-19

### Added
- Generation-safe opaque handle registry with stale-handle rejection
- Versioned ABI 1.0.0 encoding
- Panic containment at FFI boundaries
- Thread-local structured last-error JSON
- Full primitive constructors/readers
- UTF-8 string and byte-buffer ABI
- Persistent arrays and maps
- Structural equality and tagged JSON conversion
- Bounded asynchronous channels
- Cancellation tokens and cancellable operations
- Sandboxed filesystem policy
- Platform diagnostics and runtime doctor command
- C integration smoke test
- Windows/Linux/macOS CI and release workflows

### Changed
- Rust edition standardized to 2021 for broad stable-toolchain compatibility
- Runtime package version advanced from 0.1.0 to 0.2.0
- ABI type numbering now reserves zero for invalid handles/types

### Security
- Released handles cannot become valid after registry slot reuse
- Rust panics are converted to AGI_PANIC
- Filesystem reads support size limits and configured roots

## 0.6.0-native-compute
- Expanded native tensor operations: reshape, subtraction, Hadamard product,
  transpose, sigmoid, softmax, sum, mean, and checked element access.
- Added reusable native `DenseLayer` and trainable multivariate `LinearModel`.
- Added C ABI v1.1 tensor/model handle registries with deterministic release.
- Added native compute capability reporting with zero Python/WASM requirement.
- Added generated-C list reserve/free/pop/insert/remove and nested-list cleanup.
- Added zero-Python production dependency enforcement script.

## 0.6.0

- Added native hardware discovery and accelerator dispatch policy.
- Added portable native SHA-256, HMAC-SHA256, OS CSPRNG, and constant-time comparison.
- Added reverse-mode tensor autograd and Adam optimizer foundations.
- Added cryptographic and accelerator functions to ABI 1.2.
- Added machine-compute provider/security standard and intrinsic manifest.
