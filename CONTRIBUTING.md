# Contributing

Thank you for your interest in contributing to `lightarchitects`.

## Opening Issues

- Search existing issues before filing a new one.
- For bug reports: include the crate version, OS, Rust toolchain (`rustc --version`), and a minimal reproducer.
- For feature requests: describe the use case, not just the implementation.
- Security issues: see [SECURITY.md](SECURITY.md) — do **not** open a public issue.

## Pull Requests

1. **Fork** the repository and create a branch off `main`.
2. **Make quality pass** before pushing:

   ```bash
   make quality   # fmt --check + clippy (pedantic) + unit + integration tests
   ```

3. **Keep changes focused** — one logical change per PR. Refactors and features in separate PRs.
4. **Doc comments required** — all public items must have doc comments (`missing_docs = "deny"`).
5. **No `.unwrap()` / `.expect()` in production code** — use `?` or explicit `match`.
6. **Tests required** — new behaviour needs a test. Minimum coverage: 90%.
7. Open the PR and fill in the template.

## Coding Standards

Canonical reference: `~/lightarchitects/soul/helix/user/standards/canon/builders-cookbook.md`

Key rules enforced by CI:
- `clippy::pedantic` as errors
- Cyclomatic complexity ≤ 10
- Functions ≤ 60 lines
- `unsafe` requires a `// SAFETY:` comment
- No `panic!()` in production code

## Running Tests

```bash
make test           # unit + integration tests
make doctest        # doc-example tests
make test-features  # isolated per-feature-combo tests (~102s)
```

## Commit Messages

Lead with the type: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`.
Keep the subject line under 72 characters. Reference issues with `Fixes #N` in the body.
