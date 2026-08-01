# AGILANG Native Runtime 0.6.0 upgrade

Implemented in this source release:

- hardware discovery and dispatch-policy crate;
- honest CPU backend selection and explicit accelerator-unavailable errors;
- CPU ISA reporting including SIMD and cryptographic instruction availability;
- portable native SHA-256 and HMAC-SHA256;
- operating-system cryptographic randomness;
- constant-time byte comparison;
- reverse-mode tensor autograd for add, elementwise multiply, ReLU, mean and matmul;
- native Adam optimizer state;
- accelerator and cryptographic C ABI functions;
- public C header updates;
- compiler/runtime intrinsic manifest;
- machine-compute security and provider standard.

Not falsely claimed as complete:

- CUDA, DirectML, Vulkan, Metal, OpenCL and NPU kernels require separate provider
  crates, vendor SDKs, drivers and hardware test machines;
- bare-metal and kernel-driver access requires privileged platform modules;
- language syntax for tensors/autograd still needs parser, semantic, HIR and
  code-generation integration against the canonical AGILANG specification;
- Cargo.lock must be regenerated and all Rust tests run on a machine with Rust.
