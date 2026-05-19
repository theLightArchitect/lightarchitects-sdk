//! Hardened security primitives for the architecture intelligence pipeline.
//!
//! All external-input paths (file reads, subprocess invocation, HTML emission) are
//! funnelled through the three sub-modules below before touching the OS.
//!
//! | Fold | Module | Threat |
//! |------|--------|--------|
//! | B1+S-1 | [`cmd_exec`] | command-injection via arbitrary binary / flag pass-through |
//! | H1+S-3 | [`path`]     | path-traversal + TOCTOU symlink race |
//! | H2+S-4 | [`encode`]   | XSS via un-encoded output in HTML emitter |

pub mod cmd_exec;
pub mod encode;
pub mod path;
