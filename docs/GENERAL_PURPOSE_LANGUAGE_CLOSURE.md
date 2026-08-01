# AGILANG General-Purpose Language Closure

Version 0.7.0 adds native resource-aware computation, realtime WebSocket transport, WebRTC signaling and capability reporting, safe AGS state parsing, and ABI exposure for RAM/SSD and WebRTC capabilities.

## Implemented in this package

- Native RAM inventory and conservative computation budgets.
- SSD-backed memory-mapped spill buffers for streaming-capable workloads.
- Explicit storage-tier selection rather than pretending SSD equals RAM.
- Native WebSocket client and server transport.
- Built-in WebRTC signaling hub.
- Feature-gated native peer connection and STUN/TURN provider integration.
- AGS hydration state parsed using JSON.parse rather than eval.
- Runtime capability ABI for resources and WebRTC.
- Existing native HTTP/HTTPS client, HTTP server, AGS compiler, SSR and hydration remain integrated.

## Production boundaries

A general-purpose language cannot honestly bundle a universally functional TURN service with zero deployment configuration. TURN requires public routing, credentials, certificates, open UDP/TCP ports and relay bandwidth. AGILANG now includes the provider boundary and signaling layer; full media and relay support is compiled using the `full-webrtc` feature and must be deployed with valid network configuration.

Likewise, GPU execution requires a concrete provider such as CUDA, DirectML, Vulkan or Metal. The accelerator runtime must report initialized provider availability before dispatch. CPU fallback remains mandatory unless an application explicitly requests accelerator-only execution.

## Validation commands

```powershell
cargo test -p agilang-runtime-resource
cargo test -p agilang-runtime-realtime
cargo test -p agilang-runtime-webrtc
cargo test -p agilang-ags-hydration
cargo test -p agilang-runtime-abi
cargo test --workspace
```

Build full WebRTC provider integration:

```powershell
cargo build --release -p agilang-runtime-webrtc --features full-webrtc
```
