# Changelog

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
