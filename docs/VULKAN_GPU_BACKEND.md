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

The intrinsic declaration is present in `runtime/intrinsics.json`, but that
manifest alone does **not** make a `.agi` language-level function callable.
The compiler frontend, symbol/type checking, C code generation, and
application test must still be wired to the new C ABI signature.

## Known limitations

- Simple 8x8-workgroup f32 matmul only; not yet an optimized tiled kernel.
- Each call creates GPU resources, pipeline and device. Context/pipeline caching
  and asynchronous multi-dispatch scheduling are future improvements.
- Buffers larger than device binding limits return an explicit error.
- CPU tensor operations and autograd currently use f64 and are unaffected.
- The AMD GPU must be physically tested on Windows; CI without Vulkan hardware
  can only validate compilation and CPU tests.
- No guarantee of speedup until measured on the RX 6800 XT.
