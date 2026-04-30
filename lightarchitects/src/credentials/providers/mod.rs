//! Concrete credential providers. Each lives in its own submodule; the
//! canonical service/file strings for that CLI are scoped to that module
//! and nothing else in the crate.

#[cfg(feature = "providers-anthropic")]
pub(crate) mod anthropic_cli;

#[cfg(feature = "providers-openai")]
pub(crate) mod openai_cli;

#[cfg(feature = "providers-google")]
pub(crate) mod google_cli;
