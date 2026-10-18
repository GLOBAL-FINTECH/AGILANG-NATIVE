# AGILANG Native Vulkan Compute — Initial Backend

## Goal

Execute real f32 matrix multiplication on a Vulkan GPU such as the
AMD Radeon RX 6800 XT (16 GB). This is a **first vertical slice**, not yet
full tensor/autograd/LLM GPU acceleration.

The implementation uses the native Vulkan backend of wgpu 23.0.1 to
compile and dispatch a WGSL compute shader. It does not use NVIDIA CUDA
or pretend a CPU is a GPU. Software Vulkan adapters are rejected.

## Windows setup

1. Install a current AMD Radeon driver with Vulkan support.
2. Install Rust (stable, minimum AGILANG workspace Rust version 1.81)
   and the Visual Studio C++ build tools/MSVC target if needed.
3. From the repository root run:

```powershell
cargo test -p agilang-runtime-accelerator
cargo test -p agilang-runtime-accelerator --features vulkan
$env:AGILANG_TEST_VULKAN = "1"
cargo test -p agilang-runtime-accelerator --features vulkan hardware_gpu_matmul_opt_in -- --nocapture
cargo build -p agilang-runtime-abi --features vulkan --release
```

The first two test commands do not require a GPU. The third invokes the real
GPU test and must fail, rather than fake success, when no physical Vulkan GPU
is available. The Cargo.lock file needs to be updated and committed using a
Rust-equipped development machine when pulling new optional dependencies.

## Hardware verification

First confirmed GPU execution, 2026-10-18, on the reference workstation:

| Item | Value |
| --- | --- |
| GPU | AMD Radeon RX 6800 XT (discrete), Vulkan API 1.4.315 |
| Driver | AMD proprietary 26.8.1 (Windows driver 32.0.21045.5002) |
| OS / toolchain | Windows 11 Pro 10.0.26200, cargo 1.97.1, MSVC 2022 Build Tools |

| Check | Result |
| --- | --- |
| `cargo test -p agilang-runtime-accelerator --features vulkan` | 4 passed |
| `AGILANG_TEST_VULKAN=1` strict `hardware_gpu_matmul_opt_in` | passed (`RequireAccelerator`, no CPU fallback) |
| `cargo build -p agilang-runtime-abi --features vulkan --release` | built |
| `examples/vulkan_matmul_smoke.c` via MSVC + release DLL, policy `2` | `58 64 139 154` — GPU verification PASSED |
| `examples/native/gpu_matmul.agi` compiled by `agilang run`, policy `2` | `58 64 139 154` — `AGILANG GPU matmul: PASSED` |
| Same program linked against a runtime built without `--features vulkan` | fails with `Unsupported: Vulkan backend was not compiled`, exit 13 (no CPU fallback) |

This verifies correctness only; no performance measurement has been made yet.

## Exposed Rust API

```rust
use agilang_runtime_accelerator::{matmul_f32, DispatchPolicy};

let c = matmul_f32(
    &[1., 2., 3., 4., 5., 6.],
    &[7., 8., 9., 10., 11., 12.],
    2, 3, 2,
    DispatchPolicy::RequireAccelerator,
)?;
assert_eq!(c, vec![58., 64., 139., 154.]);
```

## C ABI

`agi_accelerator_matmul_f32(left, left_len, right, right_len, m, k, n, policy, out, out_capacity)`

- `0` = CPU only; `1` = prefer Vulkan with CPU fallback;
  `2` = require Vulkan GPU, no silent fallback.
- Matrix buffers are row-major, `float` / IEEE 754 f32.
- Output must hold at least `m * n` floats; function returns AGILANG's
  standard status code. Use `agi_runtime_last_error_json` on failure.
- Physical GPU VRAM is not invented from wgpu's allocation limits.

## `.agi` language builtin

Ordinary AGILANG programs call the GPU through the compiler builtin
`gpu_matmul`, with no Rust or C required:

```text
let c: array = gpu_matmul(a, b, m, k, n, policy)
```

- `a` and `b` are numeric lists holding row-major `A[m,k]` and `B[k,n]`;
  the result is a new numeric list holding `C[m,n]`.
- `m`, `k`, `n` and `policy` must be integers. A literal policy outside
  `0..=2` is a compile-time error.
- **Precision:** AGILANG lists hold f64. Inputs are narrowed to f32 for
  the device and the result is widened back to f64, so values carry f32
  precision (about 7 significant digits).
- **Errors:** there is no recoverable error path yet. If the runtime
  rejects the call (no Vulkan GPU under policy `2`, dimensions that do not
  match the list lengths, buffers beyond device limits), the program prints
  `gpu_matmul failed: <runtime error JSON>` to stderr and exits with the
  runtime status code.
- The C helper is only emitted for programs that call `gpu_matmul`, so
  other programs still link against runtime libraries built before it existed.

Run the example on a Vulkan GPU from the repository root:

```powershell
cargo build --release -p agilang-runtime-abi --features vulkan
cargo build --release -p agilang-native-cli
$env:AGILANG_RUNTIME_LIB = (Resolve-Path target\release\agilang_runtime_abi.lib).Path
.\target\release\agilang.exe run examples\native\gpu_matmul.agi
```

On Windows, wgpu always builds its WGL/OpenGL fallback alongside Vulkan, so
the AGILANG linker passes `user32`, `gdi32`, `opengl32` and `runtimeobject`
when linking compiled programs.

## Known limitations

- Simple 8x8-workgroup f32 matmul only; not yet an optimized tiled kernel.
- Each call creates GPU resources, pipeline and device. Context/pipeline caching
  and asynchronous multi-dispatch scheduling are future improvements.
- Buffers larger than device binding limits return an explicit error.
- CPU tensor operations and autograd currently use f64 and are unaffected.
- CI without Vulkan hardware can only validate compilation and CPU tests;
  GPU runs must be repeated on physical hardware (see Hardware verification).
- No guarantee of speedup until measured on the RX 6800 XT.
