# AGILANG Native Catch-Up Requirements

## Objective

AGILANG Native must become a source-compatible, behavior-compatible, CLI-compatible, framework-compatible, and deployment-compatible native replacement for `GLOBAL-FINTECH/agilang`.

The compatibility target is pinned by [compat/canonical.lock.json](../compat/canonical.lock.json). Compatibility claims must remain fail-closed until differential tests pass against that exact canonical revision.

## Completion Rule

A feature is only complete after all applicable layers are covered:

```text
Canonical inventory
-> Lexer
-> Parser
-> AST
-> Type system
-> Semantic analysis
-> IR
-> Code generation
-> Runtime ABI
-> Standard library
-> Framework exposure
-> CLI exposure
-> LSP/tooling
-> Tests
-> Documentation
```

Allowed statuses:

```text
not-inventoried
specified
in-progress
provider-complete
language-complete
differential-pass
release-compatible
blocked
```

Only `release-compatible` means one-to-one compatibility.

## Milestone N0

The first executable work package is compatibility control:

1. Pin the canonical Genesis commit in `compat/canonical.lock.json`.
2. Replace broad feature rows with a granular canonical inventory in `compat/feature-matrix.csv`.
3. Create the differential harness roots:
   - `compat/harness/`
   - `compat/fixtures/`
   - `compat/results/`
4. Make CI fail if:
   - the canonical lock contains placeholders;
   - the canonical SHA is not a 40-character commit hash;
   - feature rows still use placeholder locations such as `<inventory-required>`.

Exit requirement:

```text
No placeholder compatibility metadata remains.
```

## Required Categories

The canonical inventory must be split into granular, testable rows across at least:

```text
lexer
syntax
expressions
types
control-flow
collections
modules
objects
errors
async
stdlib
filesystem
network
http
security
framework
routing
controllers
middleware
views
ags
auth
sessions
validation
database
orm
migrations
seeding
cli
project-generation
testing
lsp
formatter
ai
evm
blockchain
deployment
packaging
```

## Differential Harness Contract

The eventual harness must compare Native and Genesis for each fixture on:

```text
parse success or failure
diagnostic code
diagnostic severity
diagnostic source span
stdout
stderr
exit status
returned value
generated files
file contents
database mutations
HTTP responses
serialized JSON
side effects
runtime resource state
```

## Current N0 Scope in This Repository

This repository now treats the following as mandatory control gates:

- a real pinned canonical commit;
- a non-placeholder capture timestamp;
- compatibility audit failure on placeholder metadata;
- compatibility audit failure on coarse matrix rows that still use `<inventory-required>`;
- CI execution of the compatibility audit.

That is necessary groundwork. It is not itself proof of language or framework parity.
