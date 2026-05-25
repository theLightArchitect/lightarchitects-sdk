//! `GraphSAGE` inductive embedding provider.
//!
//! Wraps a `BGE-384` [`EmbeddingProvider`] with a learned two-layer `ReLU`
//! projection that maps 384-dim semantic vectors → 128-dim structural vectors.
//!
//! # Architecture
//!
//! ```text
//! text ──► BGE-384 ──► W₀ (256×384) ──► ReLU ──► W₁ (128×256) ──► ReLU ──► L2-norm ──► 128-dim
//! ```
//!
//! The projection matrices are loaded from `sage_projection.bin` (a flat f32
//! array: W₀ concatenated with W₁, row-major). The nightly consolidator writes
//! `sage_embedding` on each Step by projecting stored BGE-384 vectors through
//! these weights via [`BgeSageProjectionPipeline`].
//!
//! # Graceful fallback
//!
//! If `sage_projection.bin` is absent the provider falls back to a
//! random-initialised identity projection, ensuring the structural slot
//! returns non-empty vectors for every Step without GDS nightly.
//!
//! # Security
//!
//! Matrix file size is checked via metadata before reading (max 4 MB guard,
//! `MAX_PROJECTION_BYTES`). Path validation is the caller's responsibility —
//! callers constructing the path from user-controlled input must resolve and
//! validate against an allowed prefix before passing to `load` (Cookbook §31).

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, warn};

use crate::helix::embedding::{EmbeddingError, EmbeddingProvider, EmbeddingResult};

// ── Projection constants ──────────────────────────────────────────────────────

/// Input dimensions (BGE-small embedding output).
const IN_DIMS: usize = 384;
/// Hidden layer width.
const HIDDEN_DIMS: usize = 256;
/// Output dimensions for the structural HNSW index.
const OUT_DIMS: usize = 128;

/// Maximum size of `sage_projection.bin` (guards against malformed files).
const MAX_PROJECTION_BYTES: usize = 4 * 1024 * 1024;

// ── ProjectionWeights ─────────────────────────────────────────────────────────

/// Row-major projection matrices: W₀ (HIDDEN × IN) then W₁ (OUT × HIDDEN).
pub struct ProjectionWeights {
    /// W₀ row-major f32 array — `HIDDEN_DIMS` rows × `IN_DIMS` cols.
    w0: Vec<f32>,
    /// W₁ row-major f32 array — `OUT_DIMS` rows × `HIDDEN_DIMS` cols.
    w1: Vec<f32>,
}

impl ProjectionWeights {
    /// Expected byte length for the binary file.
    const EXPECTED_FLOATS: usize = HIDDEN_DIMS * IN_DIMS + OUT_DIMS * HIDDEN_DIMS;
    const EXPECTED_BYTES: usize = Self::EXPECTED_FLOATS * 4;

    /// Load weights from `sage_projection.bin`.
    ///
    /// Returns `Err` only on file I/O failures. Missing or malformed files
    /// fall back to a random-stable identity projection via [`ProjectionWeights::random_stable`].
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if the file cannot be read or has wrong length.
    pub fn load(path: &Path) -> Result<Self, std::io::Error> {
        // Reject oversized files before allocating (DoS guard, Cookbook §31).
        let file_len = usize::try_from(std::fs::metadata(path)?.len()).unwrap_or(usize::MAX);
        if file_len > MAX_PROJECTION_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "sage_projection.bin: {file_len} bytes exceeds {MAX_PROJECTION_BYTES} limit"
                ),
            ));
        }
        let bytes = std::fs::read(path)?;
        if bytes.len() != Self::EXPECTED_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "sage_projection.bin: expected {} bytes, got {}",
                    Self::EXPECTED_BYTES,
                    bytes.len()
                ),
            ));
        }
        let floats: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let w0 = floats[..HIDDEN_DIMS * IN_DIMS].to_vec();
        let w1 = floats[HIDDEN_DIMS * IN_DIMS..].to_vec();
        Ok(Self { w0, w1 })
    }

    /// Load or fall back to random-stable identity projection.
    ///
    /// Uses a seeded LCG so the fallback is reproducible across process
    /// restarts — avoids cache invalidation churn before GDS runs.
    pub fn load_or_default(path: &Path) -> Self {
        match Self::load(path) {
            Ok(w) => {
                debug!(?path, "GraphSAGE projection weights loaded");
                w
            }
            Err(e) => {
                warn!(error = %e, ?path, "sage_projection.bin absent — using random-stable fallback");
                Self::random_stable()
            }
        }
    }

    /// Serialise weights to the binary file format.
    ///
    /// Used by the consolidator to persist the SVD-derived projection.
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        let mut bytes = Vec::with_capacity(Self::EXPECTED_BYTES);
        for &f in self.w0.iter().chain(self.w1.iter()) {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
        std::fs::write(path, &bytes)
    }

    /// Reproducible random-stable projection weights (LCG seeded at `0xDEAD_BEEF`).
    ///
    /// Produces L2-normalised row vectors — the fallback behaves like a real projection.
    /// Use in tests and when `sage_projection.bin` is absent.
    #[must_use]
    pub fn random_stable() -> Self {
        Self::random_stable_impl()
    }

    fn random_stable_impl() -> Self {
        let mut state: u64 = 0xDEAD_BEEF_CAFE_1234;
        let mut next = move || -> f32 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            // >> 32 extracts the full 32-bit high word; >> 33 would give only 31 bits
            // (range [0, 2^31-1]) which after /u32::MAX maps to [-1, ~0] — all negative.
            let bits = (state >> 32) as u32;
            // IEEE 754 bit trick: top 23 bits → mantissa of [1.0, 2.0) float → shift to [-1.0, 1.0).
            // Avoids integer→float cast entirely, so no precision-loss lint.
            let frac = f32::from_bits(0x3f80_0000 | (bits >> 9)) - 1.0_f32;
            frac * 2.0 - 1.0
        };
        let w0 = normalise_rows(
            (0..HIDDEN_DIMS * IN_DIMS).map(|_| next()).collect(),
            IN_DIMS,
        );
        let w1 = normalise_rows(
            (0..OUT_DIMS * HIDDEN_DIMS).map(|_| next()).collect(),
            HIDDEN_DIMS,
        );
        Self { w0, w1 }
    }

    /// Two-layer `ReLU` projection: text embedding → 128-dim structural vector.
    ///
    /// Per IBM GNN article: nonlinear activation (`ReLU`) between layers is
    /// **essential** for representational power. A single linear layer
    /// cannot capture neighbourhood structure.
    #[must_use]
    pub fn project(&self, bge: &[f32]) -> Vec<f32> {
        debug_assert_eq!(bge.len(), IN_DIMS, "input must be {IN_DIMS}-dim BGE vector");

        // Layer 0: W₀ (HIDDEN×IN) · bge → ReLU
        let h0 = matmul_relu(&self.w0, bge, HIDDEN_DIMS, IN_DIMS);
        // Layer 1: W₁ (OUT×HIDDEN) · h0 → ReLU
        let h1 = matmul_relu(&self.w1, &h0, OUT_DIMS, HIDDEN_DIMS);
        // L2-normalise output
        l2_normalise(h1)
    }
}

// ── GraphSageProvider ─────────────────────────────────────────────────────────

/// GraphSAGE-based structural embedding provider.
///
/// Delegates semantic embedding to an inner `BGE` provider, then applies
/// the two-layer `ReLU` projection to produce 128-dim structural vectors.
pub struct GraphSageProvider {
    inner: Arc<dyn EmbeddingProvider>,
    weights: Arc<ProjectionWeights>,
}

impl GraphSageProvider {
    /// Create a provider backed by `inner` (must produce `IN_DIMS`-dim vectors)
    /// with weights loaded from `projection_path`.
    ///
    /// Falls back to random-stable weights if the file is absent.
    #[must_use]
    pub fn new(inner: Arc<dyn EmbeddingProvider>, projection_path: &Path) -> Self {
        if inner.dimensions() != IN_DIMS {
            warn!(
                dims = inner.dimensions(),
                expected = IN_DIMS,
                "GraphSageProvider: inner provider dimensions mismatch — results may be incorrect"
            );
        }
        let weights = Arc::new(ProjectionWeights::load_or_default(projection_path));
        Self { inner, weights }
    }

    /// Create with pre-loaded weights (for testing and consolidator).
    #[must_use]
    pub fn with_weights(inner: Arc<dyn EmbeddingProvider>, weights: ProjectionWeights) -> Self {
        Self {
            inner,
            weights: Arc::new(weights),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for GraphSageProvider {
    async fn embed(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Delegate to BGE for semantic embeddings
        let semantic = self.inner.embed(texts).await?;

        // Apply two-layer ReLU projection to each embedding
        let weights = Arc::clone(&self.weights);
        let projected: Vec<Vec<f32>> = semantic
            .into_iter()
            .map(|bge| {
                if bge.len() != IN_DIMS {
                    return Err(EmbeddingError::InvalidInput(format!(
                        "inner provider returned {}-dim vector, expected {IN_DIMS}",
                        bge.len()
                    )));
                }
                Ok(weights.project(&bge))
            })
            .collect::<EmbeddingResult<Vec<_>>>()?;

        Ok(projected)
    }

    fn dimensions(&self) -> usize {
        OUT_DIMS
    }

    fn name(&self) -> &'static str {
        "graphsage"
    }

    fn max_batch_size(&self) -> usize {
        self.inner.max_batch_size()
    }
}

// ── Math helpers ──────────────────────────────────────────────────────────────

/// Row-major matrix-vector multiply with `ReLU` activation.
///
/// `w` is a flat row-major matrix of shape (rows × cols).
/// Returns `relu(W · x)` as a `rows`-dim vector.
fn matmul_relu(w: &[f32], x: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    debug_assert_eq!(w.len(), rows * cols);
    debug_assert_eq!(x.len(), cols);

    let mut out = Vec::with_capacity(rows);
    for row in 0..rows {
        let row_start = row * cols;
        let dot: f32 = w[row_start..row_start + cols]
            .iter()
            .zip(x.iter())
            .map(|(&wi, &xi)| wi * xi)
            .sum();
        out.push(dot.max(0.0)); // ReLU
    }
    out
}

/// L2-normalise a vector in-place, returning it.
///
/// If the norm is effectively zero (degenerate embedding), returns the
/// original vector unchanged to avoid NaN propagation.
fn l2_normalise(mut v: Vec<f32>) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-8 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

/// L2-normalise each row of a flat row-major matrix.
fn normalise_rows(mut flat: Vec<f32>, cols: usize) -> Vec<f32> {
    let rows = flat.len() / cols;
    for row in 0..rows {
        let start = row * cols;
        let norm: f32 = flat[start..start + cols]
            .iter()
            .map(|x| x * x)
            .sum::<f32>()
            .sqrt();
        if norm > 1e-8 {
            for x in &mut flat[start..start + cols] {
                *x /= norm;
            }
        }
    }
    flat
}

// ── Size guard for file reads ─────────────────────────────────────────────────

/// Assert at compile time that the projection file is within the size bound.
const _: () = {
    assert!(
        ProjectionWeights::EXPECTED_BYTES <= MAX_PROJECTION_BYTES,
        "sage_projection.bin expected size exceeds 4MB guard"
    );
};

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::helix::embedding::MockEmbeddingProvider;

    fn sage_provider() -> GraphSageProvider {
        let inner = Arc::new(MockEmbeddingProvider::new(IN_DIMS));
        // Use random-stable fallback weights (no file needed in tests)
        let weights = ProjectionWeights::random_stable();
        GraphSageProvider::with_weights(inner, weights)
    }

    #[test]
    fn test_provider_dimensions() {
        let p = sage_provider();
        assert_eq!(p.dimensions(), OUT_DIMS);
        assert_eq!(p.name(), "graphsage");
    }

    #[tokio::test]
    async fn test_embed_returns_correct_dims() {
        let p = sage_provider();
        let results = p.embed(&["hello world"]).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].len(), OUT_DIMS);
    }

    #[tokio::test]
    async fn test_embed_batch() {
        let p = sage_provider();
        let results = p.embed(&["one", "two", "three"]).await.unwrap();
        assert_eq!(results.len(), 3);
        for v in &results {
            assert_eq!(v.len(), OUT_DIMS);
        }
    }

    #[tokio::test]
    async fn test_embed_empty() {
        let p = sage_provider();
        let results = p.embed(&[]).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_output_l2_normalised() {
        let p = sage_provider();
        let results = p.embed(&["normalise check"]).await.unwrap();
        let norm: f32 = results[0].iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "GraphSAGE output must be L2-normalised, got norm={norm}"
        );
    }

    #[tokio::test]
    async fn test_deterministic_projection() {
        // Same input → same output (weights are deterministic)
        let p = sage_provider();
        let v1 = p.embed(&["reproducible"]).await.unwrap();
        let v2 = p.embed(&["reproducible"]).await.unwrap();
        assert_eq!(v1, v2, "Same input must produce same structural vector");
    }

    #[tokio::test]
    async fn test_different_inputs_differ() {
        let p = sage_provider();
        let v1 = p.embed(&["alpha"]).await.unwrap();
        let v2 = p.embed(&["beta"]).await.unwrap();
        assert_ne!(
            v1, v2,
            "Different inputs should produce different structural vectors"
        );
    }

    #[test]
    fn test_matmul_relu_zero_input() {
        // All-zero input → all-zero output (ReLU preserves zeros)
        let w = vec![1.0_f32; 2 * 3]; // 2×3
        let x = vec![0.0_f32; 3];
        let out = matmul_relu(&w, &x, 2, 3);
        assert_eq!(out, vec![0.0, 0.0]);
    }

    #[test]
    fn test_matmul_relu_clamps_negative() {
        // Row dot-product = -1 → clamped to 0 by ReLU
        let w = vec![-1.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0]; // 2×3
        let x = vec![1.0_f32, 0.0, 0.0];
        let out = matmul_relu(&w, &x, 2, 3);
        assert!(
            out[0].abs() < f32::EPSILON,
            "negative activation must be clamped to 0"
        );
        assert!(
            (out[1] - 1.0_f32).abs() < f32::EPSILON,
            "positive activation must pass through"
        );
    }

    #[test]
    fn test_l2_normalise_unit_vector() {
        let v = vec![1.0_f32, 0.0, 0.0];
        let out = l2_normalise(v);
        assert!((out[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalise_zero_vector() {
        // Zero vector should not produce NaN
        let v = vec![0.0_f32; 4];
        let out = l2_normalise(v.clone());
        assert_eq!(out, v, "zero vector should be returned unchanged");
    }

    #[test]
    fn test_projection_weights_roundtrip() {
        let weights = ProjectionWeights::random_stable();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        weights.save(tmp.path()).unwrap();

        let loaded = ProjectionWeights::load(tmp.path()).unwrap();
        assert_eq!(weights.w0.len(), loaded.w0.len());
        assert_eq!(weights.w1.len(), loaded.w1.len());
        // First few values must match
        for i in 0..10 {
            assert!(
                (weights.w0[i] - loaded.w0[i]).abs() < 1e-6,
                "w0[{i}] roundtrip mismatch"
            );
        }
    }

    #[test]
    fn test_projection_weights_wrong_size() {
        use std::io::Write;

        let tmp = tempfile::NamedTempFile::new().unwrap();
        let mut f = tmp.reopen().unwrap();
        f.write_all(&[0u8; 100]).unwrap();

        let result = ProjectionWeights::load(tmp.path());
        assert!(result.is_err(), "wrong-size file should be rejected");
    }

    #[test]
    fn test_projection_size_constants() {
        assert_eq!(IN_DIMS, 384);
        assert_eq!(HIDDEN_DIMS, 256);
        assert_eq!(OUT_DIMS, 128);
        assert_eq!(
            ProjectionWeights::EXPECTED_FLOATS,
            256 * 384 + 128 * 256,
            "float count must match W₀ + W₁ dimensions"
        );
    }
}
