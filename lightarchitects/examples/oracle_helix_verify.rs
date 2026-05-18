//! Oracle verification of the 4D double-helix animation mathematics.
//!
//! Dispatches four geometric claims to Leanstral (Lean 4 formal proofs),
//! `DeepSeek` (step-by-step derivation), and Qwen (numerical/edge-case checks).
//!
//! Run with:
//!   `cargo run --example oracle_helix_verify -p lightarchitects`

use lightarchitects::oracle::{OracleClient, OracleMode};

const PROMPT: &str = r"
We are verifying four mathematical properties of a 4D double-helix animation.
Vertices live in ℝ⁴. Three rotation operators are applied before a perspective
projection to ℝ³. Verify each claim precisely.

═══════════════════════════════════════════════════════════════════════════════
CLAIM 1 — SO(4) membership of the three rotation operators
═══════════════════════════════════════════════════════════════════════════════

The operators are defined as:
  T_ZW(a)(x,y,z,w) = (x,  y,  z·cos a − w·sin a,  z·sin a + w·cos a)
  T_XW(a)(x,y,z,w) = (x·cos a − w·sin a,  y,  z,  x·sin a + w·cos a)
  T_YW(a)(x,y,z,w) = (x,  y·cos a − w·sin a,  z,  y·sin a + w·cos a)

Claim: For every a ∈ ℝ, each operator is an element of SO(4) — i.e., it is a
linear isometry of ℝ⁴ (preserves ‖·‖₂) with determinant +1.

Lean 4 theorem stubs:

  import Mathlib.LinearAlgebra.Matrix.Orthogonal
  import Mathlib.Analysis.SpecialFunctions.Trigonometric.Basic

  -- ZW-plane rotation matrix (acts on coordinates 2 and 3, 0-indexed)
  def rotZW (a : ℝ) : Matrix (Fin 4) (Fin 4) ℝ :=
    !![1, 0, 0, 0;
       0, 1, 0, 0;
       0, 0, Real.cos a, -(Real.sin a);
       0, 0, Real.sin a,  Real.cos a]

  theorem rotZW_mem_SO4 (a : ℝ) :
      Matrix.det (rotZW a) = 1 ∧
      (rotZW a).transpose * (rotZW a) = 1 := by
    sorry

  -- State and prove the analogous theorems for T_XW and T_YW.

Key question: Does det(T_ZW(a)) = 1 hold for ALL a ∈ ℝ, and can you provide a
clean proof using sin²a + cos²a = 1? Which Mathlib lemma gives this?

═══════════════════════════════════════════════════════════════════════════════
CLAIM 2 — Gram-Schmidt produces an orthonormal frame in ℝ⁴
═══════════════════════════════════════════════════════════════════════════════

Given a unit vector `axis ∈ ℝ⁴`, not parallel to e₃=(0,0,1,0) or e₄=(0,0,0,1),
the explicit construction:
  u' = e₃ − ⟨e₃, axis⟩·axis
  u  = u' / ‖u'‖
  v' = e₄ − ⟨e₄, axis⟩·axis − ⟨e₄ − ⟨e₄, axis⟩·axis, u⟩·u
  v  = v' / ‖v'‖

Claim: ‖u‖ = 1, ‖v‖ = 1, ⟨u, axis⟩ = 0, ⟨v, axis⟩ = 0, ⟨u, v⟩ = 0.

(This is Gram-Schmidt orthonormalization of {e₃, e₄} w.r.t. the span of `axis`.)

Lean 4 theorem stub:

  import Mathlib.Analysis.InnerProductSpace.GramSchmidt

  variable (axis : EuclideanSpace ℝ (Fin 4)) (haxis : ‖axis‖ = 1)

  -- The claim in words: the explicit Gram-Schmidt step produces an
  -- orthonormal pair {u, v} both orthogonal to `axis`.
  -- Which Mathlib theorem (gramSchmidt_orthonormal, inner_gramSchmidt_eq_zero,
  -- or similar) most directly gives this?

State the precondition precisely: for which vectors `axis` does the construction
fail (u' = 0 or v' = 0)? What is the exact geometric condition?

═══════════════════════════════════════════════════════════════════════════════
CLAIM 3 — The two helix strands are antipodal on the Clifford torus
═══════════════════════════════════════════════════════════════════════════════

The two strands are (for constants R, r ∈ ℝ and winding number n = 3):
  s₁(t) = (R·cos t,     R·sin t,     r·cos(nt),    r·sin(nt))
  s₂(t) = (R·cos(t+π),  R·sin(t+π),  r·cos(nt+π),  r·sin(nt+π))

Claim: s₂(t) = −s₁(t) for all t ∈ ℝ, for any R, r, and any integer n.

Lean 4 proof:

  import Mathlib.Analysis.SpecialFunctions.Trigonometric.Basic

  theorem strand_antipodal (R r t : ℝ) (n : ℤ) :
      let s1 : Fin 4 → ℝ := ![R * Real.cos t,
                               R * Real.sin t,
                               r * Real.cos (↑n * t),
                               r * Real.sin (↑n * t)]
      let s2 : Fin 4 → ℝ := ![R * Real.cos (t + Real.pi),
                               R * Real.sin (t + Real.pi),
                               r * Real.cos (↑n * t + Real.pi),
                               r * Real.sin (↑n * t + Real.pi)]
      s2 = -s1 := by
    simp [Real.cos_add_pi, Real.sin_add_pi]
    -- Does this close? Or is funext + fin_cases needed?

Which Mathlib lemmas are needed? Does `simp [Real.cos_add_pi, Real.sin_add_pi]`
close this goal, or is there an extra `funext` / `Fin.ext` step?

═══════════════════════════════════════════════════════════════════════════════
CLAIM 4 — Perspective projection denominator is always positive
═══════════════════════════════════════════════════════════════════════════════

The projection function is:
  proj(v : ℝ⁴) : ℝ³,  where  s = 2.6 / max(2.2 − v[3], 0.3)

Claim: max(2.2 − v[3], 0.3) ≥ 0.3 > 0 for all v[3] ∈ ℝ.
Therefore s is always finite and well-defined (no division by zero).

Lean 4 proof:

  theorem proj4_denom_pos (w : ℝ) : max (2.2 - w) (0.3 : ℝ) ≥ 0.3 := by
    exact le_max_right _ _

  theorem proj4_denom_pos' (w : ℝ) : 0 < max (2.2 - w) (0.3 : ℝ) :=
    lt_of_lt_of_le (by norm_num : (0 : ℝ) < 0.3) (le_max_right _ _)

Verify: Do these proofs compile in current Mathlib? Are the types correct
(`0.3 : ℝ` vs `(3 : ℝ) / 10`)? Is `le_max_right` the right lemma name?
";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let oracle = OracleClient::builder()
        .ollama_endpoint("http://localhost:11434")
        .build()?;

    println!("Dispatching to Leanstral (Lean 4) + DeepSeek + Qwen …\n");

    let verdict = oracle.query(PROMPT).mode(OracleMode::Prove).call().await?;

    println!("{verdict}");

    Ok(())
}
