# AGILANG Python 3.14 Parity Matrix (Wave 1)

This matrix is the measurable compatibility baseline for AGILANG-NATIVE.

Scoring source:
- Fixture manifest: compat/python314/fixtures.json
- Runner: target/debug/agilang-compat-audit parity . (after building agilang-compat-audit)
- Report: compat/reports/python314-parity-report.json

Status legend:
- PASS: implemented and covered by parity fixture(s)
- PARTIAL: behavior exists but currently diverges or lacks canonical comparison
- MISSING: not covered/implemented yet
- DIFF: intentionally different from Python semantics

## Wave 1 Summary

| Area | Status | Fixture IDs |
|---|---|---|
| AGI syntax + basic entrypoint | PASS | agi-main-i32 |
| Functions | PASS | agi-functions |
| Arrays/indexing diagnostics | PASS | agi-array-index-error |
| Error behavior | PASS | agi-missing-return-error |
| Objects/struct semantics | PASS | agi-objects-struct |
| Modules/imports | PARTIAL | agi-modules-imports |
| AGS behavior | PASS | ags-compiler-suite |
| MVC/framework behavior | PARTIAL | framework-routing-fixture |
| ORM/database behavior | PASS | database-agidb-suite |
| Scientific functions | PASS | scientific-native-stats |
| ML primitives | PASS | ml-runtime-compute, ml-runtime-abi |

## Current Gaps to Green 1:1 Parity

1. Canonical-vs-native dual execution is now required for all seeded fixtures, but currently uses command-paired canonical baselines (per fixture command) instead of a full Python 3.14 executor.
2. Module/import parity needs canonical differential fixture alignment.
3. Framework fixture currently validates diagnostics but still indicates expected divergence.
4. Python-backed MySQL fixture dependency has been removed from hard failure, but native protocol fixture coverage should keep expanding.

## Gate Contract

The parity gate fails when:

1. Fixture count falls below expected_fixture_count in compat/python314/fixtures.json.
2. Canonical compared fixture count falls below expected_parity_fixture_count.
3. Any native fixture expectation fails.
4. Any canonical-vs-native mismatch is detected.

## Enabling Canonical Differential Runs

Set AGILANG_CANONICAL_CMD before running parity to override all fixture-level canonical commands.

Example:

```powershell
$env:AGILANG_CANONICAL_CMD = "agilang run {fixture}"
cargo build -p agilang-compat-audit
.\target\debug\agilang-compat-audit.exe parity .
```

Report includes:

- `summary` block with fixture-parity percentage and explicit language-coverage status.
- `categories` block with per-category total/passed/mismatch/score.
- Per-fixture metadata: `comparison_mode`, `normalization_applied`, `classification`.

Use a canonical command that runs fixtures from the pinned repository revision in compat/canonical.lock.json.
The immediate next upgrade is replacing command-paired baselines with true Python 3.14 differential execution for the AGI semantic fixture set.
