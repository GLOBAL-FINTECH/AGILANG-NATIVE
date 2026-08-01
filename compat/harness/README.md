# Differential Harness Scaffold

This directory is reserved for the canonical-vs-native differential runner described in [docs/AGILANG_NATIVE_CATCHUP_REQUIREMENTS.md](../../docs/AGILANG_NATIVE_CATCHUP_REQUIREMENTS.md).

The harness must eventually execute the same `.agi` or `.ags` fixture through:

- `GLOBAL-FINTECH/agilang` at the pinned commit in `compat/canonical.lock.json`
- this Native workspace revision

and compare:

- parse success or failure
- diagnostics
- stdout and stderr
- exit status
- generated files
- database mutations
- HTTP responses
- serialized JSON
- side effects

This scaffold is intentionally minimal until the actual differential executor is implemented.
