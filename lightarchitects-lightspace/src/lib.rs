//! Pure-reducer engine for Lightspace JIT-reactive canvas state.
//!
//! The contract: `reduce(state, event) -> Result<state, ReducerError>` is the **sole**
//! mutation path. Implementers MUST NOT perform I/O, syscalls, clock reads, or RNG
//! inside `reduce()`. Snapshot and restore round-trip to byte-equivalent state.

pub mod error;
pub mod security;
pub mod snapshot;
pub mod types;

mod engine;

pub use engine::reducer::Lightspace;
pub use types::{CanvasEvent, CanvasState};
