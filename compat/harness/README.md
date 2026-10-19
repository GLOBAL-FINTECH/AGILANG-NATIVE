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

## Wave 1 Runner

Wave 1 parity execution is now wired through:

```bash
cargo build -p agilang-compat-audit
./target/debug/agilang-compat-audit parity .
```

On Windows PowerShell:

```powershell
cargo build -p agilang-compat-audit
.\target\debug\agilang-compat-audit.exe parity .
```

Inputs:

- `compat/python314/fixtures.json`

Outputs:

- `compat/reports/python314-parity-report.json`

Optional canonical command override:

- `AGILANG_CANONICAL_CMD`

By default, the runner enforces fixture-count gates and native expectations, and it performs
canonical-vs-native comparisons when a canonical command is configured.
