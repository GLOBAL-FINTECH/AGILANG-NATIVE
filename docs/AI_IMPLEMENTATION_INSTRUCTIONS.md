# AGILANG Native Runtime — AI Implementation Instructions

## Mission

Continue the native Rust implementation without inventing a second AGILANG dialect. The public `GLOBAL-FINTECH/agilang` repository, pinned to an exact commit, is the behavioral oracle. The uploaded handbooks are the product and teaching contract, but any construct marked as a teaching pattern must not be reported as implemented until source and executable tests prove it.

## Mandatory operating rules

0. **Never introduce Python into the native execution path.** Do not generate Python, invoke Python subprocesses, require pip/venv, or use Python for training, inference, HTTP serving, authentication, framework execution, compilation, or project generation. AGI + AGS + generated browser JavaScript + the Rust native runtime are the complete production stack.

1. Never rename or redesign canonical AGI syntax merely because Rust makes another form easier.
2. Preserve `fn main() -> i32:` and process-success `return 0` end to end.
3. Maintain `.agi` for program/backend logic and `.ags` for presentation templates.
4. Every feature must move through: lexer → AST → parser → semantic analysis → typed IR → backend/codegen → runtime ABI → CLI/LSP → conformance fixtures.
5. Provider-only HTTP, TLS, LLM, EVM, or authentication code is not language-complete until exposed to `.agi` through stable modules and ABI ownership rules.
6. Distinguish `VERIFIED`, `SUPPORTED_HOSTED`, `EXPERIMENTAL`, `PLANNED`, and `DEPRECATED` in documentation and generated diagnostics.
7. Do not silently accept unsupported syntax. Emit a stable diagnostic with a migration hint.
8. Never implement authentication using custom cryptographic primitives. Passwords use Argon2id; session identifiers use an OS CSPRNG; browser sessions require secure cookie, CSRF, expiry, rotation, and revocation controls.
9. Never expose raw EVM private keys through ordinary strings. Use opaque secure-key handles and explicit signing policy.
10. Preserve backwards compatibility through versioned adapters and deprecation tests, not duplicate divergent parsers.

## Immediate implementation order

### Phase A — pin and compare

- Replace placeholders in `compat/canonical.lock.json` with the exact 40-character canonical Git commit.
- Export canonical CLI help, grammar examples, generated projects, and test outputs.
- Run `cargo run -p agilang-compat-audit -- .`.
- Build a differential runner that executes every verified fixture in canonical and native modes and compares stdout, stderr, exit code, generated files, and diagnostics.

### Phase B — core language parity

Implement in this order:

1. comparisons: `== != < <= > >=`
2. boolean operators: `not`, `and`, `or`
3. assignments
4. `if`, `else if`, `else`
5. list literals and indexing
6. map literals and string-key indexing
7. `for` and `while`
8. `include()` resolution
9. structs, enums, imports, and type aliases after verifying exact canonical forms

For collections, use runtime-owned opaque handles at the C ABI. Define retain/release behavior and bounds/key errors. Do not lower arrays or maps to unowned temporary C pointers.

### Phase C — AGS

- Define a formal `.ags` grammar from generated repository templates.
- Escape interpolation by default.
- Add explicit trusted/sanitized raw rendering.
- Implement layouts, includes, components, props, slots, state, events, forms, loading/error/empty states, and hydration only where canonical evidence exists.
- Add browser-level XSS, accessibility, mobile viewport, and reactive-state tests.

### Phase D — web and authentication

Connect native HTTP/TLS providers to language modules and implement:

- request/response abstractions
- routing, middleware, controllers, services, repositories, models, validation, and AGS rendering
- persistent users and sessions
- registration, login, logout, password reset, email verification
- roles and policy checks on the server
- CSRF, secure cookies, session rotation, expiry, revocation, rate limits, audit logs
- optional TOTP/WebAuthn and OAuth 2.1/OIDC after the base system passes security review

### Phase E — AIFlow and LLM

- Add typed dataset, tensor, model, optimizer, training, evaluation, checkpoint, inference, ONNX, GPU, and distributed interfaces.
- Bind the provider-neutral LLM client through `std.llm`.
- Support streaming, cancellation, token/usage metadata, timeouts, retries, provider errors, and secret redaction.
- Preserve constrained-compute controls: memory budgets, mixed precision, gradient accumulation, quantization, adapters, streaming datasets, caching, and reproducibility metadata.

### Phase F — EVM and blockchain

- Bind JSON-RPC through `std.evm`.
- Add chain ID, block, transaction, receipt, logs, contract calls, gas estimation, fee data, and subscriptions.
- Add transaction construction and signing only through opaque key handles.
- Use official execution-spec vectors and JSON-RPC conformance tests.
- Keep execution, beacon/consensus, validator, P2P, indexer, RPC, and dashboard processes independently observable.

## Definition of done for each feature

A feature is complete only when all are true:

- canonical syntax fixture exists
- native positive and negative parsing tests pass
- semantic type and diagnostic tests pass
- runtime ownership and error tests pass
- generated C/native code compiles on Windows, Linux, and macOS
- canonical/native differential output passes
- CLI and LSP expose it correctly
- documentation truth label is updated
- migration/deprecation behavior is tested
- security tests pass where relevant

## Required commands before release

```bash
cargo run -p agilang-compat-audit -- .
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --workspace --release
```

Then run canonical/native differential fixtures and generated-project end-to-end tests on Windows, Ubuntu, and macOS. Generate an SBOM, dependency audit, checksums, release notes, and rollback instructions.

## Current honest boundary

The package contains native providers for HTTP/TLS, LLM, EVM, Argon2id password hashing, and secure session-token generation. The bootstrap compiler supports the canonical typed `main` form and a limited expression/function subset. Collections, full control flow, include resolution, AGS compilation, complete authentication workflows, and `.agi` bindings for the provider crates remain unfinished and must follow the phases above.
