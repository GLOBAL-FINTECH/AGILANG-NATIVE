# AGILANG Native Migration Phase 1

This merge-ready foundation adds native source management, diagnostics, lexer, AST, parser, compiler orchestration, and the `agilang-native` CLI.

## Current commands

```powershell
cargo run -p agilang-native-cli -- tokens examples/native/hello.agi
cargo run -p agilang-native-cli -- ast examples/native/hello.agi
cargo run -p agilang-native-cli -- check examples/native/hello.agi
```

This phase intentionally does not yet claim native execution or code generation. It establishes the compiler frontend boundary while preserving the validated runtime v0.2 crates.
