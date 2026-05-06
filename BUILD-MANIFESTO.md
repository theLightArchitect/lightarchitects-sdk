# Light Architects Gateway Build Manifesto

This document outlines the canonical build process for the `lightarchitects-gateway` binary, the unified MCP server for the Light Architects platform.

## Prerequisites

- Rust toolchain (via `rustup`) with target `aarch64-apple-darwin` (or your host target)
- `cargo` and `rustc` available in PATH
- Optionally, `sccache` for faster rebuilds (note: sandbox restrictions may require disabling sccache)

## Build Steps

### 1. Prepare the Environment

Due to sandbox restrictions on certain systems, you may need to disable `sccache` by unsetting the `RUSTC_WRAPPER` environment variable:

```bash
export RUSTC_WRAPPER=   # unset or set to empty
```

If you wish to use `sccache` but encounter permission errors, set a writable cache directory:

```bash
export SCCACHE_DIR=/tmp/sccache
```

### 2. Build the Gateway

From the root of the `lightarchitects-sdk` workspace:

```bash
cargo build --release -p lightarchitects-gateway
```

This produces the binary at:
`target/release/lightarchitects`

### 3. Deploy the Binary

Copy the binary to the user's local bin directory and apply an ad-hoc codesign (required for macOS):

```bash
mkdir -p "$HOME/.lightarchitects/bin"
cp target/release/lightarchitects "$HOME/.lightarchitects/bin/lightarchitects"
codesign --force --sign - "$HOME/.lightarchitects/bin/lightarchitects"
```

### 4. Generate MCP Configuration

Create the MCP configuration file so that Claude Code (or other MCP hosts) can locate the gateway:

```bash
cat > "$HOME/.lightarchitects/lightarchitects.mcp.json" <<'EOF'
{
  "mcpServers": {
    "lightarchitects": {
      "command": "$HOME/.lightarchitects/bin/lightarchitects"
    }
  }
}
EOF
```

### 5. Verify Installation

Run the gateway with `--help` to see available subcommands:

```bash
"$HOME/.lightarchitects/bin/lightarchitects" --help
```

## Quality Gates

Before committing changes, ensure the following checks pass (if sandbox permits):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Note: In sandboxed environments, `cargo test` may fail due to network-dependent tests (e.g., `mockito::Server`). See the updated `auth::key_validator` tests for a pattern that avoids network calls.

## Troubleshooting

- **sccache errors**: If you see `Operation not permitted (os error 1)`, try unsetting `RUSTC_WRAPPER` and/or setting `SCCACHE_DIR` to a writable location like `/tmp/sccache`.
- **Missing dependencies**: Ensure all workspace members are present; the gateway depends on the `lightarchitects` crate (the unified SDK).

## References

- See the `Makefile` in this repository for convenience targets (`make deploy`, `make deploy-fast`).
- The `lightarchitects-gateway` crate's `Cargo.toml` lists features and dependencies.
