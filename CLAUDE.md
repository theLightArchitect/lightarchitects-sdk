# CLAUDE.md

This file provides guidance to Claude Code when working with code in this repository.

## Overview

**la-sdk** is the Light Architects SDK — a Cargo workspace containing shared library crates for the LA ecosystem. All crates are Rust libraries (no binaries). GitHub: `TheLightArchitects/la-sdk` (private).

## Workspace Members

| Crate | Path | Purpose |
|-------|------|---------|
| `la-crypto` | `la-crypto/` | Cryptographic foundation: HKDF, HMAC, AES-256-GCM, Ed25519, SecretStore |

## Build Commands

```bash
# Quality gates (MANDATORY before commit)
make quality        # fmt --check + clippy (pedantic) + test

# Individual gates
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features

# Fix issues
make fix            # auto-fix fmt + clippy

# Benchmarks
cargo bench --workspace

# Doc generation
cargo doc --workspace --no-deps --open
```

## Workspace Conventions (Template for All Crates)

### Cargo.toml Pattern

New crates inherit from the workspace root:
```toml
[package]
name = "la-{name}"
version = "0.1.0"
description = "..."
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
# Use workspace deps: dep.workspace = true

[lints]
workspace = true
```

### Enforced Lints

| Lint | Level | Effect |
|------|-------|--------|
| `clippy::pedantic` | deny | Strict Rust idioms |
| `missing_docs` | deny | Every pub item documented |
| `unsafe_code` | deny | Must use `#[allow(unsafe_code)]` with `// SAFETY:` comment |

### Coding Standards

- NO `.unwrap()` / `.expect()` in library code (use `?` or `match`)
- NO `panic!()` — use `Result<T, E>`
- `unsafe` requires `// SAFETY:` comment and `#[allow(unsafe_code)]`
- Cyclomatic complexity <= 10, functions <= 60 lines
- All secret material in `SecretString` or `Zeroizing`
- Constant-time comparison via `subtle` crate (never hand-rolled XOR)
- Checked arithmetic (`checked_add`, `saturating_sub`)

### Adding a New Crate

1. Create `la-{name}/` directory with `Cargo.toml`, `src/lib.rs`
2. Add to `[workspace] members` in root `Cargo.toml`
3. Use `[workspace.dependencies]` for shared deps
4. Set `[lints] workspace = true`
5. Add `#![doc = "..."]` crate-level documentation
6. Add `/// # Examples` to all public functions

### Feature Flags

Feature flags should gate optional backends or heavy deps:
```toml
[features]
default = ["feature-a"]
feature-a = ["dep:optional-dep"]
```

## CI Pipeline

Three GitHub Actions jobs on push/PR to main:
- **Quality Gates**: fmt + clippy (pedantic, warnings-as-errors)
- **Tests**: `cargo test --workspace --all-features`
- **Security Audit**: `cargo audit` + `cargo deny check`

Pre-commit hook runs fmt + clippy (no tests — too slow for commit-time).

## Dependency Policy

All version specs live in `[workspace.dependencies]`. Member crates use `dep.workspace = true`. This prevents version drift when multiple crates share dependencies.

Approved ecosystems:
- **RustCrypto** (hkdf, hmac, sha2, aes-gcm, subtle)
- **Dalek** (ed25519-dalek, curve25519-dalek)
- **serde ecosystem** (serde, toml)
- **secrecy/zeroize** for secret handling
