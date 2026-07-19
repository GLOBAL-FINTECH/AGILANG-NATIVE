# AGILANG Native Runtime Architecture

## Boundary

Compiler-generated machine code calls the versioned C ABI in `agilang-runtime-abi`. Rust's native ABI is never exposed. Breaking ABI changes require a new major ABI; additive changes increment the minor ABI.

## Value ownership

AGILANG values are immutable runtime objects. Collections use copy-on-write/persistent semantics. Native callers hold opaque 64-bit handles. The low 32 bits identify a registry slot and the high 32 bits carry its generation. Releasing a handle increments the generation, which makes stale handles invalid even after slot reuse.

## Failure isolation

Every fallible exported operation executes behind `catch_unwind`. No Rust panic is permitted to cross the ABI. Errors are stored per operating-system thread and can be retrieved as structured JSON. A status code is always returned for fallible operations.

## Runtime layers

```text
Generated AGILANG machine code / LLVM JIT / C host
                         │
                 agilang_runtime.h
                         │
              Stable versioned C ABI
                         │
      ┌──────────────────┼──────────────────┐
      │                  │                  │
  Core values       Async executor     Platform policy
  and handles       and channels       and filesystem
```

## Production completion gates

The full AGILANG runtime reaches 1.0 only after these additional gates:

1. Callable/closure ABI and stack-frame metadata
2. Structured exceptions with source spans and native stack traces
3. Cycle-aware managed heap or compiler-enforced ownership model
4. HTTP/1.1, HTTP/2, WebSocket, DNS, TLS, and certificate policy
5. Cryptographic provider with secure key handles and zeroization
6. Native module loader, signed packages, and dependency lockfiles
7. Database provider interfaces and transaction semantics
8. EVM, blockchain, ATCP, AI, payment, and web standard modules
9. LLVM AOT/JIT intrinsic registration and conformance suite
10. Fuzzing, sanitizers, ABI compatibility tests, SBOM, signing, MSI/DEB/RPM/PKG installers

## Compatibility policy

- ABI major: breaking symbol, layout, ownership, or behavior change
- ABI minor: additive symbol or capability
- ABI patch: implementation fix without contract changes
- Runtime package version: follows semantic versioning independently of ABI version
