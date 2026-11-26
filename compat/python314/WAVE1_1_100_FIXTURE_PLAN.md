# Wave 1.1 Fixture Expansion Plan (12 -> 100)

This plan starts immediately after Wave 1 closes at 12 fixtures with 0 mismatches.

## Current Checkpoint Status

- Stage A (12 -> 25): COMPLETE
  - fixture_count: 25
  - native_passed: 25
  - compared: 25
  - mismatch_count: 0
  - fixture_parity_percent: 100.0
  - exit code: 0
  - frozen regression manifest: compat/python314/fixtures-wave1_1-25-regression.json
- Stage B (25 -> 50): ACTIVE

## Expansion Stages

1. Stage A: 12 -> 25
2. Stage B: 25 -> 50
3. Stage C: 50 -> 100

## Stage B Allocation (25 -> 50)

- Numeric semantics / precedence: +4
- Strings / Unicode: +4
- Collections / slicing / nesting: +4
- Functions / defaults / recursion / closures: +4
- Objects / methods / inheritance: +3
- Exceptions / propagation / finally: +2
- Modules / imports: +2
- Iterators / generators: +2

## Stage B Gate

- fixture_count = 50
- native_passed = 50
- compared = 50
- mismatch_count = 0
- fixture_parity_percent = 100.0
- exit_code = 0

## Capability Coverage Registry

Stage B introduces capability-based coverage accounting alongside fixture parity.

Report metrics now include:

- fixture_parity_percent
- capability_coverage_percent
- capability_registry_count
- capability_covered_count

## Category Targets (100 Total)

- numbers/operators: 10
- strings/unicode: 10
- arrays/lists: 10
- maps/sets/tuples: 10
- functions/arguments: 10
- scope/closures: 10
- control flow: 8
- classes/objects: 10
- exceptions: 8
- modules/imports: 6
- iterators/generators: 4
- diagnostics/negative: 4

## Rules

- Compare strict semantics by default.
- Only apply normalization for known nondeterministic cargo test command output.
- Any mismatch requires:
  1. fixture reproduction
  2. regression test
  3. implementation fix
  4. full parity rerun

## Coverage Interpretation

- Current fixture parity can reach 100% while language coverage is still expanding.
- Keep both metrics visible in reports:
  - current fixture parity percent
  - python 3.14 language coverage status

## Seed Source

Initial 25-candidate seed manifest:

- compat/python314/fixtures-wave1_1-seed.json
