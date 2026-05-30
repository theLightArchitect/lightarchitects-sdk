//! Convergence traits — re-exported from the `la-loops` public crate.
//!
//! All convergence signal types and strategy-agnostic traits are defined in
//! `la-loops` so downstream consumers can take a lightweight dep without
//! pulling in the full SDK. See `la_loops` for full documentation.

pub use la_loops::{
    BlastScore, ConvergenceGate, ConvergenceResult, InterestDecay, IntervalWatch, NPassVerifier,
    QueueDrain,
};
