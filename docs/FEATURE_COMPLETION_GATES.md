# AGILANG Native Feature Completion Gates

A feature may not be marked `release-compatible` merely because a provider, parser node, or runtime function exists.

Every feature must pass all applicable gates:

1. Canonical syntax pinned against AGILANG Genesis.
2. Lexer and parser coverage.
3. AST representation with source spans.
4. Type-system representation.
5. Semantic validation, including negative diagnostics.
6. HIR/IR lowering.
7. AGILANG Native code generation.
8. Runtime ABI and standard-library exposure where applicable.
9. CLI and LSP exposure where applicable.
10. Positive and negative tests.
11. Executable smoke test on Windows and Linux.
12. Differential fixture against the pinned Genesis revision.
13. Documentation and examples.
14. Successful required CI checks.

## Current core-language closure set

The following matrix rows are governed by `.github/workflows/language-completion.yml`:

- `AGI-SYN-FUNCTION-001`
- `AGI-SYN-RETURN-001`
- `AGI-TYPE-I32-001`
- `AGI-COL-ARRAY-TYPE-001`
- `AGI-COL-ARRAY-ITERATE-001`
- `AGI-COL-ARRAY-NESTED-001`

Their status must remain `in-progress` until the workflow passes and the Genesis differential fixtures are recorded.

## Provider-complete is not language-complete

Authentication, HTTPS, LLM, and EVM RPC are separate completion programs. Each still requires direct `.agi` APIs, end-to-end application fixtures, security tests, CLI/LSP integration, and cross-platform execution proof.
