# AGILANG Native Compute

Portable native CPU tensor operations, reverse-mode autograd, dense/linear models,
SGD and Adam primitives. No Python or WebAssembly runtime is used.

GPU/NPU execution is selected through `agilang-runtime-accelerator`; concrete
CUDA/DirectML/Vulkan/Metal providers remain optional platform modules because
those APIs require vendor SDKs and drivers.
