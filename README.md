# AGILANG Native Runtime

AGILANG Native Runtime is the platform-independent runtime kernel intended for code emitted by the native AGILANG compiler. It is implemented in Rust and does not require Python, `pip`, Node.js, Java, or another language runtime.

## Release status

Version **0.2.0** is a production-oriented runtime foundation and ABI release candidate. It is suitable for compiler integration, embedding, value exchange, native platform discovery, bounded asynchronous work, and C ABI validation. It is **not yet the complete AGILANG standard library**: HTTP/TLS, cryptography, database drivers, EVM host functions, ATCP, payment rails, garbage collection for cyclic objects, and the LLVM compiler remain separate milestones.

## Implemented

- Versioned C ABI with semantic major/minor/patch encoding
- Panic containment at every fallible C ABI boundary
- Thread-local structured last-error reporting
- Generation-safe opaque handles that reject stale IDs
- Deterministic handle release and leak counters
- Null, Boolean, signed/unsigned integer, float, UTF-8 string, bytes, array, map, and error values
- Immutable/persistent array and map operations
- Structural equality and tagged JSON interchange
- Native multi-thread asynchronous executor
- Bounded channels, timeouts, cancellation, and timers
- Cross-platform process and platform discovery
- Filesystem policy with root restrictions, write controls, and maximum read sizes
- Static and dynamic runtime libraries
- C/C++ header and C integration smoke test
- Windows, Linux, and macOS CI definitions
- Cross-platform release artifact workflow

## Build and verify

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features
cargo build --workspace --release
cargo run -p agilang-runtime-cli -- doctor
```

## Runtime CLI

```bash
agilang-runtime version
agilang-runtime info
agilang-runtime smoke
agilang-runtime doctor
```

## ABI use

```c
#include "agilang_runtime.h"

AgiHandle value = agi_value_int(1990);
int64_t result = 0;
AgiStatus status = agi_value_read_int(value, &result);
agi_handle_release(value);
```

Every non-zero status can be diagnosed with `agi_runtime_last_error_json`. Buffers returned by the runtime must be released with `agi_buffer_free`. Every non-zero `AgiHandle` owned by the caller must eventually be released with `agi_handle_release`.

## Outputs

- Windows: `agilang_runtime_abi.dll`, import/static library, `agilang-runtime.exe`
- Linux: `libagilang_runtime_abi.so`, `libagilang_runtime_abi.a`, `agilang-runtime`
- macOS: `libagilang_runtime_abi.dylib`, `libagilang_runtime_abi.a`, `agilang-runtime`

## Security model

Rust types never cross the public binary boundary. Generated code uses only `include/agilang_runtime.h`. Opaque handles contain slot and generation fields, preventing a released handle from becoming valid when a slot is reused. Runtime panics are converted into `AGI_PANIC` instead of unwinding through foreign code.
