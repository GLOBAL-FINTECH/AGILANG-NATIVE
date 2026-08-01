# AGILANG Native Capability Matrix

This matrix separates a **native runtime implementation** from application-framework syntax and compiler exposure. A capability is only fully complete when all three layers are connected.

| Capability | Rust-native provider | Framework API | Compiler/ABI intrinsic | Status |
|---|---:|---:|---:|---|
| Password hashing | Argon2id | `agilang-framework-auth` | Pending | Provider complete |
| Session token entropy | OS CSPRNG | `SessionToken` | Pending | Provider complete |
| HTTP/1.1 client | Yes | `agilang-runtime-http` | Pending | Provider complete |
| HTTP/2 client | Yes | `agilang-runtime-http` | Pending | Provider complete |
| TLS client validation | rustls roots | `TlsPolicy` | Pending | Provider complete |
| HTTPS server | Pending | Existing HTTP server is plaintext | Pending | Not complete |
| LLM chat | OpenAI-compatible HTTP | `agilang-runtime-llm` | Pending | Provider complete |
| EVM JSON-RPC | Generic EVM endpoint | `agilang-runtime-evm` | Pending | Provider complete |
| EVM transaction signing | Pending secure-key provider | Pending | Pending | Not complete |
| Embedded EVM execution | Pending engine integration | Pending | Pending | Not complete |
| WebSocket | Pending | Pending | Pending | Not complete |
| OAuth/OIDC/MFA | Pending | Pending | Pending | Not complete |

## Required next integration

1. Add opaque native handles for HTTP, LLM, EVM, credentials, and sessions to `agilang-runtime-abi`.
2. Register compiler intrinsics and AGILANG standard modules so `.agi` source can call these providers directly.
3. Replace the plaintext framework server with bounded asynchronous parsing, request-size limits, rustls HTTPS, HTTP/2, WebSocket upgrades, graceful shutdown, and connection quotas.
4. Add database-backed user/session stores, CSRF protection, secure cookie attributes, email verification, password reset, MFA/WebAuthn, OAuth 2.1 and OpenID Connect.
5. Add a secure key vault and transaction signer before exposing EVM send/sign operations.

## Security decision

The previous `HmacSha256PasswordHasher` implementation was not HMAC-SHA256 and used a fast non-cryptographic construction. The compatibility type name now maps to Argon2id so existing source imports keep compiling while stored passwords migrate to a memory-hard format. Existing legacy hashes must be invalidated or migrated after successful credential verification through a separately controlled compatibility service.
