# Native standard library API contract

Stable namespaces for v0.9:

- Native.Fs: read, write, append, exists, remove, rename, mkdir, list.
- Native.Net: TCP/UDP listeners and connections.
- Native.Http: HTTP/HTTPS client and server.
- Native.Json: encode/decode with deterministic UTF-8 behavior.
- Native.Crypto: hashing, HMAC, signatures and secure random bytes.
- Native.Process: spawn, wait, environment and exit status.
- Native.Async: tasks, channels, cancellation and bounded concurrency.

All APIs must be native-runtime backed, platform-tested and versioned. Python is not part of the runtime contract.
