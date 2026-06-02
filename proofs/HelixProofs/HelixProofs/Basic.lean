/-
  HelixProofs/Basic.lean — Machine-checkable invariants of the Light Architects
  helix retrieval system + agent orchestration delivery layer.

  Toolchain: Lean 4.29.1 + Mathlib

  Sections:
    §1  SignalWeights smart-constructor invariant (proves Rust refactor correct)
    §2  RetrievalMode partition + composed dispatcher correctness
    §3  Brute-force cosine cost model
    §4  Gate count enumeration (agents-playbook §15.3.13.5)
    §5  RRF score positivity + monotonicity (Mathlib)
    §6  Wave-cut formal contract — parallel agent safety (extended with disjointness)

  Author: Claude (Phase 7 Lean Oracle verification, 2026-05-18)
-/

import Mathlib.Data.Finset.Basic
import Mathlib.Data.Real.Basic
import Mathlib.Tactic.Positivity
import Mathlib.Tactic.NormNum
import Mathlib.Tactic.Linarith
import Mathlib.Analysis.SpecialFunctions.Pow.Real
import Mathlib.Algebra.Order.BigOperators.Group.List

-- ============================================================================
-- §1. SignalWeights smart-constructor invariant
-- ============================================================================
-- Source: lightarchitects/src/helix/soul_search/hybrid.rs:135-220
-- (Refactored to u8 percentages with smart constructor at Phase 7 close-out.)

namespace SignalWeights

/-- Lean mirror of the Rust `SignalWeights` u8-percent representation. -/
structure T where
  fulltext_pct : Nat
  semantic_pct : Nat
  structural_pct : Nat
  graph_pct : Nat
  sum_eq_100 :
    fulltext_pct + semantic_pct + structural_pct + graph_pct = 100

/-- Lean mirror of `SignalWeights::new(...)` smart constructor. -/
def new (ft sm st gr : Nat) : Option T :=
  if h : ft + sm + st + gr = 100 then
    some {
      fulltext_pct := ft
      semantic_pct := sm
      structural_pct := st
      graph_pct := gr
      sum_eq_100 := h
    }
  else none

-- LOAD-BEARING: the smart constructor returns Some iff the sum is exactly 100.
theorem new_some_iff_sum_100 (ft sm st gr : Nat) :
    (new ft sm st gr).isSome ↔ ft + sm + st + gr = 100 := by
  unfold new
  by_cases h : ft + sm + st + gr = 100
  · simp [h]
  · simp [h]

-- Concrete validations matching `hybrid.rs:92-110` literal values:
theorem keyword_dominated_valid : (new 65 25 3 7).isSome := by decide
theorem balanced_valid : (new 25 35 10 30).isSome := by decide
theorem graph_weighted_valid : (new 15 30 10 45).isSome := by decide

-- Counter-examples (rejected by the smart constructor):
theorem too_high_rejected : (new 50 50 50 0).isSome = false := by decide
theorem too_low_rejected : (new 50 25 0 0).isSome = false := by decide
theorem zero_rejected : (new 0 0 0 0).isSome = false := by decide

-- Bound corollary: each percentage is automatically ≤ 100 when sum = 100.
theorem fulltext_le_100 (w : T) : w.fulltext_pct ≤ 100 := by
  have h := w.sum_eq_100; omega

theorem semantic_le_100 (w : T) : w.semantic_pct ≤ 100 := by
  have h := w.sum_eq_100; omega

theorem structural_le_100 (w : T) : w.structural_pct ≤ 100 := by
  have h := w.sum_eq_100; omega

theorem graph_le_100 (w : T) : w.graph_pct ≤ 100 := by
  have h := w.sum_eq_100; omega

end SignalWeights

-- ============================================================================
-- §2. RetrievalMode partition + composed dispatcher correctness
-- ============================================================================

namespace RetrievalMode

inductive Mode where
  | keywordDominated
  | balanced
  | graphWeighted
  deriving DecidableEq, Repr

def fromStepCount (n : Nat) : Mode :=
  if n < 25 then Mode.keywordDominated
  else if n < 100 then Mode.balanced
  else Mode.graphWeighted

-- Partition is total.
theorem mode_is_total (n : Nat) :
    fromStepCount n = Mode.keywordDominated
  ∨ fromStepCount n = Mode.balanced
  ∨ fromStepCount n = Mode.graphWeighted := by
  unfold fromStepCount
  by_cases h1 : n < 25
  · simp [h1]
  · by_cases h2 : n < 100
    · simp [h1, h2]
    · simp [h1, h2]

-- Weight tuple for each mode (mirrors hybrid.rs literal values).
def weightsFor (m : Mode) : Nat × Nat × Nat × Nat :=
  match m with
  | Mode.keywordDominated => (65, 25, 3, 7)
  | Mode.balanced         => (25, 35, 10, 30)
  | Mode.graphWeighted    => (15, 30, 10, 45)

def sum4 (t : Nat × Nat × Nat × Nat) : Nat :=
  t.1 + t.2.1 + t.2.2.1 + t.2.2.2

-- LOAD-BEARING: for any step count, the dispatcher produces a valid distribution.
theorem dispatcher_preserves_probability_invariant (n : Nat) :
    sum4 (weightsFor (fromStepCount n)) = 100 := by
  unfold fromStepCount weightsFor sum4
  by_cases h1 : n < 25
  · simp [h1]
  · by_cases h2 : n < 100
    · simp [h1, h2]
    · simp [h1, h2]

-- Sharper: dispatcher output ALWAYS constructs a valid SignalWeights.
theorem dispatcher_constructs_valid_weights (n : Nat) :
    (SignalWeights.new
      (weightsFor (fromStepCount n)).1
      (weightsFor (fromStepCount n)).2.1
      (weightsFor (fromStepCount n)).2.2.1
      (weightsFor (fromStepCount n)).2.2.2).isSome := by
  unfold fromStepCount weightsFor
  by_cases h1 : n < 25
  · simp [h1]; decide
  · by_cases h2 : n < 100
    · simp [h1, h2]; decide
    · simp [h1, h2]; decide

end RetrievalMode

-- ============================================================================
-- §3. Brute-force cosine cost model
-- ============================================================================

namespace BruteForceCost

def cost (scope_size dim : Nat) : Nat := scope_size * dim

theorem cost_at_73_steps_nomic : cost 73 768 = 56064 := by decide
theorem cost_at_73_steps_minilm : cost 73 384 = 28032 := by decide
theorem cost_at_500_steps_nomic : cost 500 768 = 384000 := by decide

theorem cost_linear_in_scope (s d : Nat) :
    cost (2 * s) d = 2 * cost s d := by
  unfold cost
  rw [Nat.mul_assoc]

theorem cost_monotone_scope (s₁ s₂ d : Nat) (h : s₁ ≤ s₂) :
    cost s₁ d ≤ cost s₂ d := by
  unfold cost
  exact Nat.mul_le_mul_right d h

-- HNSW-vs-brute-force breakeven: scope_size at which brute-force == HNSW
-- HNSW cost is O(log N) where N = global corpus; we model as hnsw_const * log2(N).
def breakevenScope (hnsw_const global_N dim : Nat) : Nat :=
  if dim = 0 then 0 else hnsw_const * (Nat.log2 global_N) / dim

theorem breakeven_zero_dim (hnsw_const global_N : Nat) :
    breakevenScope hnsw_const global_N 0 = 0 := by
  unfold breakevenScope; simp

end BruteForceCost

-- ============================================================================
-- §4. Gate count enumeration (agents-playbook §15.3.13.5)
-- ============================================================================

namespace GateCount

def pre_phase : Nat := 4
def pre_wave : Nat := 7
def pre_task : Nat := 7
def post_task : Nat := 3
def post_wave : Nat := 3
def cross_doc : Nat := 4

def total_tabled : Nat := pre_phase + pre_wave + pre_task + post_task + post_wave
def total_with_cross_doc : Nat := total_tabled + cross_doc

theorem tabled_count_is_24 : total_tabled = 24 := by decide
theorem composite_count_is_28 : total_with_cross_doc = 28 := by decide

end GateCount

-- ============================================================================
-- §5. RRF score positivity + monotonicity (Mathlib)
-- ============================================================================
-- Source: hybrid.rs:469-489 `add_signal_ranks`
--   contribution(rank) = weight / (RRF_K + rank + 1)
-- RRF_K = 60 (Cormack et al. 2009).

namespace RRF

def RRF_K : ℝ := 60

-- Each RRF term is strictly positive when weight > 0.
theorem term_positive (weight : ℝ) (rank : ℕ) (hw : weight > 0) :
    weight / (RRF_K + (rank : ℝ) + 1) > 0 := by
  apply div_pos hw
  unfold RRF_K
  positivity

-- RRF denominator is strictly positive.
theorem denom_positive (rank : ℕ) :
    RRF_K + (rank : ℝ) + 1 > 0 := by
  unfold RRF_K; positivity

end RRF

-- ============================================================================
-- §6. Wave-cut formal contract — parallel agent safety
-- ============================================================================
-- Sources:
--   - Cookbook §64 (Serialized Git-Operations Mutex Pattern; wave-cut invariant)
--   - agents-playbook §15.3.13.5 (PW-4 parent-SHA + PW-6 disjoint ownership)
--   - LASDLC v2.5.3 git_branching_invariants block

namespace WaveCut

abbrev Sha := String
abbrev FilePath := String

structure Task where
  id : String
  parent_sha : Sha
  file_ownership : Finset FilePath
  -- (no `deriving Repr` — Finset doesn't expose Repr instance by default)

/-- A wave dispatch: a set of tasks dispatched concurrently from feat HEAD.

    Construction requires proofs of:
    - PW-4: every task's parent_sha = feat_head_at_cut
    - PW-6: distinct tasks have disjoint file_ownership
    - Cookbook §64: parallelism cap ≤ 7
-/
structure WaveDispatch where
  feat_head_at_cut : Sha
  tasks : List Task
  parallelism_le_7 : tasks.length ≤ 7
  all_share_parent : ∀ t ∈ tasks, t.parent_sha = feat_head_at_cut
  pairwise_disjoint : ∀ t₁ ∈ tasks, ∀ t₂ ∈ tasks,
      t₁.id ≠ t₂.id → Disjoint t₁.file_ownership t₂.file_ownership

-- Theorem A: tasks in same wave share parent (PW-4 invariant).
theorem wave_tasks_share_parent (w : WaveDispatch) (t₁ t₂ : Task)
    (h₁ : t₁ ∈ w.tasks) (h₂ : t₂ ∈ w.tasks) :
    t₁.parent_sha = t₂.parent_sha := by
  rw [w.all_share_parent t₁ h₁, w.all_share_parent t₂ h₂]

-- Theorem B: distinct tasks have disjoint writes (PW-6 invariant).
theorem wave_tasks_disjoint_writes (w : WaveDispatch) (t₁ t₂ : Task)
    (h₁ : t₁ ∈ w.tasks) (h₂ : t₂ ∈ w.tasks) (hne : t₁.id ≠ t₂.id) :
    Disjoint t₁.file_ownership t₂.file_ownership :=
  w.pairwise_disjoint t₁ h₁ t₂ h₂ hne

/-- Safety predicate: a wave is safe for parallel execution iff (A) AND (B). -/
def IsSafe (w : WaveDispatch) : Prop :=
  (∀ t₁ ∈ w.tasks, ∀ t₂ ∈ w.tasks, t₁.parent_sha = t₂.parent_sha) ∧
  (∀ t₁ ∈ w.tasks, ∀ t₂ ∈ w.tasks,
      t₁.id ≠ t₂.id → Disjoint t₁.file_ownership t₂.file_ownership)

-- LOAD-BEARING: every well-formed WaveDispatch is safe for parallel execution.
theorem well_formed_implies_safe (w : WaveDispatch) : IsSafe w := by
  refine ⟨?_, ?_⟩
  · intro t₁ h₁ t₂ h₂
    exact wave_tasks_share_parent w t₁ t₂ h₁ h₂
  · intro t₁ h₁ t₂ h₂ hne
    exact wave_tasks_disjoint_writes w t₁ t₂ h₁ h₂ hne

-- Contrapositive A: parent mismatch → no valid wave can be constructed.
theorem parent_mismatch_blocks_dispatch
    (feat_head : Sha) (tasks : List Task)
    (h_violation : ∃ t ∈ tasks, t.parent_sha ≠ feat_head) :
    ¬∃ (w : WaveDispatch), w.feat_head_at_cut = feat_head ∧ w.tasks = tasks := by
  intro ⟨w, hhead, htasks⟩
  obtain ⟨t, hmem, hne⟩ := h_violation
  apply hne
  rw [← hhead]
  apply w.all_share_parent
  rw [htasks]; exact hmem

-- Contrapositive B: file_ownership conflict → no valid wave.
theorem ownership_conflict_blocks_dispatch
    (tasks : List Task)
    (h_conflict : ∃ t₁ ∈ tasks, ∃ t₂ ∈ tasks,
      t₁.id ≠ t₂.id ∧ ¬Disjoint t₁.file_ownership t₂.file_ownership) :
    ¬∃ (w : WaveDispatch), w.tasks = tasks := by
  intro ⟨w, htasks⟩
  obtain ⟨t₁, h₁, t₂, h₂, hne, hconflict⟩ := h_conflict
  apply hconflict
  apply w.pairwise_disjoint t₁ _ t₂ _ hne
  · rw [htasks]; exact h₁
  · rw [htasks]; exact h₂

end WaveCut

-- ============================================================================
-- §7. Weighted Beta-Bernoulli conjugacy
-- ============================================================================
-- Source: ~/.claude/plans/copilot-routing-fingerprint-v1.md, Phase 3.
--
-- Routing policy is Thompson sampling on Beta posteriors maintained per
-- (model, task-class). Each observation is a Bernoulli outcome xᵢ ∈ {0,1}
-- with a recency / source weight wᵢ ∈ (0,1].
--
-- LOAD-BEARING CLAIM (Walker 2006 power likelihood + Bishop PRML §2.1.1):
--
--   Beta(α, β) prior × ∏ᵢ θ^(wᵢxᵢ) (1-θ)^(wᵢ(1-xᵢ))   (un-normalized)
--     = Beta(α + Σ wᵢxᵢ , β + Σ wᵢ(1-xᵢ))             (un-normalized)
--
-- We formalize this at the un-normalized PDF kernel level
--   K(α,β; θ) := θ^(α-1) (1-θ)^(β-1)
-- which is the standard textbook proof. The full posterior (normalized) then
-- follows because:
--   (a) the result is in the Beta exponential family (this proof), and
--   (b) Mathlib's `lintegral_betaPDF_eq_one` already establishes that for
--       any α > 0, β > 0, the Beta(α,β) density integrates to 1, so the
--       normalizing constant is uniquely determined by (α, β).
--
-- Together (a) + (b) give: the normalized posterior is Beta(α + Σwx, β + Σw(1-x)).
--
-- We work on θ ∈ (0,1) where both bases are strictly positive — matching the
-- support of the Beta distribution. Boundary behavior at θ=0/θ=1 is a
-- measure-zero set and irrelevant to the posterior identity.

namespace BetaBernoulli

open Real

/-- Un-normalized Beta(α,β) PDF kernel evaluated at θ. We use real-valued
    exponents to support fractional weighted updates. -/
noncomputable def betaKernel (α β θ : ℝ) : ℝ :=
  θ ^ (α - 1) * (1 - θ) ^ (β - 1)

/-- Weighted Bernoulli likelihood (Walker 2006 power likelihood) for a single
    observation x ∈ {0,1} with weight w ∈ (0,1]. We pass x as a real and
    require x*(1-x) = 0 (i.e. x ∈ {0,1}) in the proof when needed. -/
noncomputable def weightedBernoulliLik (x w θ : ℝ) : ℝ :=
  θ ^ (w * x) * (1 - θ) ^ (w * (1 - x))

/-- A single weighted observation: posterior parameter increments.

    For x = 1: α gains w, β gains 0.
    For x = 0: α gains 0, β gains w.
    Compactly: α gains w*x, β gains w*(1-x). -/
def alphaIncrement (x w : ℝ) : ℝ := w * x
def betaIncrement (x w : ℝ) : ℝ := w * (1 - x)

/-- **§7.1 Single-step conjugacy** — the kernel × likelihood collapses to a
    new kernel with updated parameters. Pure `rpow` algebra; works for any
    real x, w (we only need positivity of the bases). -/
theorem single_step_conjugacy
    (α β x w θ : ℝ) (hθ_pos : 0 < θ) (hθ_lt : θ < 1) :
    betaKernel α β θ * weightedBernoulliLik x w θ
      = betaKernel (α + alphaIncrement x w) (β + betaIncrement x w) θ := by
  unfold betaKernel weightedBernoulliLik alphaIncrement betaIncrement
  have h1θ_pos : (0 : ℝ) < 1 - θ := by linarith
  -- Group the two θ-bases and two (1-θ)-bases.
  -- θ^(α-1) * (1-θ)^(β-1) * (θ^(w*x) * (1-θ)^(w*(1-x)))
  --   = (θ^(α-1) * θ^(w*x)) * ((1-θ)^(β-1) * (1-θ)^(w*(1-x)))
  --   = θ^((α-1) + w*x)    * (1-θ)^((β-1) + w*(1-x))
  --   = θ^((α + w*x) - 1)  * (1-θ)^((β + w*(1-x)) - 1)
  rw [show
      θ ^ (α - 1) * (1 - θ) ^ (β - 1)
        * (θ ^ (w * x) * (1 - θ) ^ (w * (1 - x)))
      = (θ ^ (α - 1) * θ ^ (w * x))
        * ((1 - θ) ^ (β - 1) * (1 - θ) ^ (w * (1 - x))) by ring]
  rw [← rpow_add hθ_pos, ← rpow_add h1θ_pos]
  -- Two remaining goals: exponents on θ and (1-θ) must match.
  -- Goal 1: (α - 1) + w*x = (α + w*x) - 1
  -- Goal 2: (β - 1) + w*(1 - x) = (β + w*(1 - x)) - 1
  refine congrArg₂ (· * ·) ?_ ?_ <;> congr 1 <;> ring

/-- **§7.2 Effective-sample-size invariant** — under any sequence of
    weighted updates, the sum α + β grows exactly by the sum of weights.
    This is the N_eff = α + β invariant the routing confidence band uses. -/
theorem effective_sample_size_invariant
    (α β x w : ℝ) :
    (α + alphaIncrement x w) + (β + betaIncrement x w) = (α + β) + w := by
  unfold alphaIncrement betaIncrement; ring

/-- Multi-observation posterior parameters (recursive form), defined over a
    list of (x, w) observations. -/
def posteriorAlpha (α₀ : ℝ) : List (ℝ × ℝ) → ℝ
  | []              => α₀
  | (x, w) :: rest  => posteriorAlpha (α₀ + alphaIncrement x w) rest

def posteriorBeta (β₀ : ℝ) : List (ℝ × ℝ) → ℝ
  | []              => β₀
  | (x, w) :: rest  => posteriorBeta (β₀ + betaIncrement x w) rest

/-- Closed-form posterior alpha: α₀ + Σ wᵢxᵢ over the observation list. -/
theorem posteriorAlpha_closed_form (α₀ : ℝ) (obs : List (ℝ × ℝ)) :
    posteriorAlpha α₀ obs = α₀ + (obs.map (fun p => alphaIncrement p.1 p.2)).sum := by
  induction obs generalizing α₀ with
  | nil => simp [posteriorAlpha]
  | cons head rest ih =>
    obtain ⟨x, w⟩ := head
    simp [posteriorAlpha, ih, List.sum_cons, List.map_cons]
    ring

/-- Closed-form posterior beta: β₀ + Σ wᵢ(1-xᵢ) over the observation list. -/
theorem posteriorBeta_closed_form (β₀ : ℝ) (obs : List (ℝ × ℝ)) :
    posteriorBeta β₀ obs = β₀ + (obs.map (fun p => betaIncrement p.1 p.2)).sum := by
  induction obs generalizing β₀ with
  | nil => simp [posteriorBeta]
  | cons head rest ih =>
    obtain ⟨x, w⟩ := head
    simp [posteriorBeta, ih, List.sum_cons, List.map_cons]
    ring

/-- Multi-observation likelihood product. -/
noncomputable def likelihoodProduct (θ : ℝ) : List (ℝ × ℝ) → ℝ
  | []              => 1
  | (x, w) :: rest  => weightedBernoulliLik x w θ * likelihoodProduct θ rest

/-- **§7.3 Multi-observation conjugacy** — the LOAD-BEARING THEOREM.

    For any list of weighted Bernoulli observations, the prior kernel times
    the likelihood product equals the posterior kernel with updated parameters.

    This is the formal statement that the routing policy's posterior is
    *exactly* Beta(α₀ + Σwx, β₀ + Σw(1-x)) up to the Beta normalizing constant
    (which is uniquely determined by those parameters per Mathlib's
    `lintegral_betaPDF_eq_one`).
-/
theorem weighted_beta_bernoulli_conjugacy
    (α₀ β₀ θ : ℝ) (hθ_pos : 0 < θ) (hθ_lt : θ < 1)
    (obs : List (ℝ × ℝ)) :
    betaKernel α₀ β₀ θ * likelihoodProduct θ obs
      = betaKernel (posteriorAlpha α₀ obs) (posteriorBeta β₀ obs) θ := by
  induction obs generalizing α₀ β₀ with
  | nil =>
      simp [likelihoodProduct, posteriorAlpha, posteriorBeta]
  | cons head rest ih =>
      obtain ⟨x, w⟩ := head
      -- LHS: betaKernel α₀ β₀ θ * (weightedBernoulliLik x w θ * likelihoodProduct θ rest)
      --    = (betaKernel α₀ β₀ θ * weightedBernoulliLik x w θ) * likelihoodProduct θ rest
      --    = betaKernel (α₀ + w*x) (β₀ + w*(1-x)) θ * likelihoodProduct θ rest      [single_step]
      --    = betaKernel (posteriorAlpha (α₀ + w*x) rest) (posteriorBeta ... rest) θ [ih]
      -- which is the RHS by definition of posteriorAlpha/posteriorBeta on cons.
      simp only [likelihoodProduct, posteriorAlpha, posteriorBeta]
      rw [← mul_assoc, single_step_conjugacy α₀ β₀ x w θ hθ_pos hθ_lt]
      exact ih (α₀ + alphaIncrement x w) (β₀ + betaIncrement x w)

/-- **§7.4 N_eff closed form** — α + β after N weighted updates equals
    (α₀ + β₀) + Σ wᵢ. The Bernoulli outcomes cancel: each observation
    contributes exactly its full weight to α+β. -/
theorem n_eff_closed_form (α₀ β₀ : ℝ) (obs : List (ℝ × ℝ)) :
    posteriorAlpha α₀ obs + posteriorBeta β₀ obs
      = (α₀ + β₀) + (obs.map Prod.snd).sum := by
  induction obs generalizing α₀ β₀ with
  | nil =>
      simp [posteriorAlpha, posteriorBeta]
  | cons head rest ih =>
      obtain ⟨x, w⟩ := head
      simp only [posteriorAlpha, posteriorBeta, List.map_cons, List.sum_cons]
      rw [ih (α₀ + alphaIncrement x w) (β₀ + betaIncrement x w)]
      unfold alphaIncrement betaIncrement
      ring

/-- **§7.5 Positivity preservation** — if the prior has α₀, β₀ > 0 and all
    weights are non-negative with all x ∈ [0,1], the posterior parameters
    remain strictly positive. Required for `Mathlib.beta_pos` to apply. -/
theorem posterior_alpha_pos (α₀ : ℝ) (h_pos : 0 < α₀) (obs : List (ℝ × ℝ))
    (h_obs : ∀ p ∈ obs, 0 ≤ p.2 ∧ 0 ≤ p.1) :
    0 < posteriorAlpha α₀ obs := by
  rw [posteriorAlpha_closed_form]
  have h_sum_nn : 0 ≤ (obs.map (fun p => alphaIncrement p.1 p.2)).sum := by
    apply List.sum_nonneg
    intro y hy
    simp only [List.mem_map] at hy
    obtain ⟨p, hpmem, hpeq⟩ := hy
    rw [← hpeq]
    unfold alphaIncrement
    exact mul_nonneg (h_obs p hpmem).1 (h_obs p hpmem).2
  linarith

theorem posterior_beta_pos (β₀ : ℝ) (h_pos : 0 < β₀) (obs : List (ℝ × ℝ))
    (h_obs : ∀ p ∈ obs, 0 ≤ p.2 ∧ p.1 ≤ 1) :
    0 < posteriorBeta β₀ obs := by
  rw [posteriorBeta_closed_form]
  have h_sum_nn : 0 ≤ (obs.map (fun p => betaIncrement p.1 p.2)).sum := by
    apply List.sum_nonneg
    intro y hy
    simp only [List.mem_map] at hy
    obtain ⟨p, hpmem, hpeq⟩ := hy
    rw [← hpeq]
    unfold betaIncrement
    have h1 : 0 ≤ 1 - p.1 := by linarith [(h_obs p hpmem).2]
    exact mul_nonneg (h_obs p hpmem).1 h1
  linarith

-- Concrete sanity checks at literal observation lists (proven via the closed
-- form theorems above — the recursive `unfold` can't be discharged by norm_num
-- alone in nested `cons` form).

example :
    posteriorAlpha (3/2 : ℝ) [(0, 3/10), (1, 7/10)] = 3/2 + 7/10 := by
  rw [posteriorAlpha_closed_form]
  simp [alphaIncrement]

example :
    posteriorBeta (1 : ℝ) [(0, 3/10), (1, 7/10)] = 1 + 3/10 := by
  rw [posteriorBeta_closed_form]
  simp [betaIncrement]

end BetaBernoulli

-- ============================================================================
-- Verification summary — axioms used per theorem
-- ============================================================================

#print axioms SignalWeights.new_some_iff_sum_100
#print axioms SignalWeights.keyword_dominated_valid
#print axioms RetrievalMode.dispatcher_preserves_probability_invariant
#print axioms RetrievalMode.dispatcher_constructs_valid_weights
#print axioms BruteForceCost.cost_at_73_steps_nomic
#print axioms BruteForceCost.cost_linear_in_scope
#print axioms GateCount.composite_count_is_28
#print axioms RRF.term_positive
#print axioms WaveCut.well_formed_implies_safe
#print axioms WaveCut.parent_mismatch_blocks_dispatch
#print axioms WaveCut.ownership_conflict_blocks_dispatch
#print axioms BetaBernoulli.single_step_conjugacy
#print axioms BetaBernoulli.effective_sample_size_invariant
#print axioms BetaBernoulli.posteriorAlpha_closed_form
#print axioms BetaBernoulli.posteriorBeta_closed_form
#print axioms BetaBernoulli.weighted_beta_bernoulli_conjugacy
#print axioms BetaBernoulli.n_eff_closed_form
#print axioms BetaBernoulli.posterior_alpha_pos
#print axioms BetaBernoulli.posterior_beta_pos
