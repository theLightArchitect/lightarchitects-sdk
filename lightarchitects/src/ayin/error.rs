//! Error types for the trace engine.

/// All errors that can occur within the trace engine.
#[derive(Debug, thiserror::Error)]
pub enum TraceError {
    /// Failed to serialize or deserialize trace data.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Failed to perform a file I/O operation.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A required field was missing when building a span.
    #[error("missing required field: {field}")]
    MissingField {
        /// The name of the missing field.
        field: String,
    },

    /// An invalid configuration value was provided.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// Confidence value out of range [0.0, 1.0].
    #[error("confidence {value} out of range [0.0, 1.0]")]
    ConfidenceOutOfRange {
        /// The invalid value.
        value: f64,
    },

    /// Strand weight out of range [0.0, 1.0].
    #[error("strand weight {value} out of range [0.0, 1.0]")]
    WeightOutOfRange {
        /// The invalid value.
        value: f64,
    },
}
