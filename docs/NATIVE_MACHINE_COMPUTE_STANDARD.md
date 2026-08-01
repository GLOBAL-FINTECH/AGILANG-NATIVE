# AGILANG Native Machine Compute Standard 0.6

AGILANG separates machine access into capability-controlled layers:

1. **Native CPU:** always available; scalar/SIMD detection, tensors, autograd,
   optimizers, hashing, randomness, networking, files, processes and databases.
2. **Accelerator provider:** optional CUDA, DirectML, Vulkan, Metal, OpenCL or
   NPU module. A provider must initialize successfully and report supported
   operations before dispatch. CPU fallback is explicit.
3. **Privileged platform provider:** drivers, physical memory, device registers,
   kernel interfaces and direct hardware control. These require OS permissions,
   signed modules and an explicit capability grant.

## Required backend contract

Every compute backend must expose device identity, availability, memory limits,
supported data types, supported operations, deterministic mode, synchronization,
error mapping and resource cleanup. Presence of a GPU does not imply usability.

## Security boundary

Cryptographic key material must never be logged, implicitly cloned or returned
through diagnostics. Secret buffers require explicit ownership and zeroization.
Randomness must come from the operating-system CSPRNG. Comparisons of authenticators
must use constant-time operations.

## Current production surface

The portable implementation includes native CPU tensors, rank-2 matrix operations,
reverse-mode autograd for add/multiply/ReLU/mean/matmul, SGD/Adam foundations,
SHA-256, HMAC-SHA256, OS randomness, constant-time comparison, and a versioned C ABI.

Concrete GPU/NPU kernels are intentionally provider modules because they require
vendor SDKs, drivers and hardware-specific validation. They must not be simulated.
