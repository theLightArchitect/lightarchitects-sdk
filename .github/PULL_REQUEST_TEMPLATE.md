## Summary

<!-- What does this PR do? One paragraph. -->

## Changes

<!-- Bullet list of what changed. -->

## Acceptance Criteria

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --workspace --all-features` passes
- [ ] No new `.unwrap()` / `.expect()` / `panic!()` in production code
- [ ] All new public items have doc comments
- [ ] `cargo deny check` passes (no new CVEs, no banned licenses)

## Notes

<!-- Anything reviewers should pay special attention to. -->
