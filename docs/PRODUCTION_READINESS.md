# AGILANG Native Production Readiness

This document defines the minimum engineering gates for treating AGILANG Native as a usable programming-language toolchain.

## Release gates

The v0.8 hardening layer additionally runs package-manager unit tests and executable negative compiler tests.

A release candidate must pass all of the following:

1. cargo fmt --all --check
2. cargo clippy --workspace --all-targets --all-features -- -D warnings
3. cargo run -p agilang-compat-audit --
4. ./scripts/validate-native.sh .
5. ./scripts/verify-zero-python.sh
6. cargo test --workspace --all-features
7. cargo build --workspace --release --all-features
8. cargo run -p agilang-runtime-cli -- doctor
9. cargo run -p agilang-native-cli -- check examples/native/hello-native.agi
10. cargo run -p agilang-native-cli -- run examples/native/hello-native.agi

The language smoke test is intentionally an actual compile-and-execute path. It is not satisfied by parser-only tests.

## Production language baseline

The baseline language path is:

`.agi source -> lexer -> parser -> typed HIR -> C code generation -> native compiler/linker -> AGILANG runtime ABI -> executable`

The runtime ABI is versioned separately from the compiler and must remain backward-compatible within an ABI major version.

## Platform policy

The supported native release targets are:

- Linux x86_64
- Linux aarch64
- Windows x86_64
- Windows aarch64
- macOS x86_64
- macOS Apple Silicon (aarch64)

CI must compile, test, and execute the hello-language smoke test on each supported host.

## Release discipline

Do not mark a release as production-ready solely because unit tests pass. The release gate must also prove:

- the compiler can produce a native executable;
- the executable can start and return successfully;
- the runtime library is discoverable through the installed toolchain;
- the compatibility audit passes;
- the native runtime contains no embedded Python dependency;
- formatting, linting, and all workspace tests pass.

## Known scope

Production readiness here means a usable native programming-language toolchain and a controlled runtime baseline. It does not claim complete one-to-one parity with every historical AGILANG framework, AIFlow backend, database provider, or blockchain integration. Those capabilities remain tracked by the compatibility matrix.
