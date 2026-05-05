//! Vault-as-git module — git-native vault CLI and pre-push validation.
//!
//! This module provides:
//! - [`prepush`] — pre-push validation: `NEVER_published_paths` regex guards and
//!   wikilink leakage detection for both the private soul-vault and the public
//!   companion repo.

pub mod prepush;
