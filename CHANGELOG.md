# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
This project uses semantic versioning.

## [Unreleased]

### Added

- Initial workspace scaffold: renamed `la-sdk` → `l-arc-sdk`, `la-crypto` → `l-arc-crypto`
- `l-arc-crypto` — scripture-forged cryptographic foundation (HKDF, HMAC, AES-256-GCM, Ed25519, SecretStore)
- Workspace-level lint configuration (`clippy::pedantic`, `missing_docs`, `unsafe_code = deny`)
- GitHub Actions CI: quality gates, macOS/Linux test matrix, MSRV check, cargo-audit, cargo-deny
- `rustfmt.toml` (edition 2024, max_width 100), `clippy.toml` (cognitive-complexity-threshold 10)
- `deny.toml` — license allowlist (MIT, Apache-2.0, BSD-2/3, ISC, Unicode)
- `dependabot.yml` — weekly Cargo dependency updates (RustCrypto group, secret-handling group)
- `.githooks/pre-commit` — fmt + clippy gate before every commit
