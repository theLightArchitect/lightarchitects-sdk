# R10 — Research Basis: Autonomous Coding Agent Prior Art

> Pre-loaded from plan iter-4 scaffold. Confirms C2 cross-validation discipline (Blueprint Part IV).

## Primary citations

| # | Paper | arXiv | Relevance to this build |
|---|-------|-------|------------------------|
| 1 | CodeCoR: An LLM-based Multi-Agent Framework for Code Generation with Chain-of-Repair | 2501.07811 | Multi-agent code generation with repair loops → ReviewGate MAX_GATE_ITERATIONS=3 design |
| 2 | Self-Correcting LLM-Controlled Diffusion Models | 2505.23060 | Self-correction via feedback loop → DecisionPipeline Layer 3 LightArchitect re-evaluation |
| 3 | AgentCoder: Multi-Agent-based Code Generation with Iterative Testing and Optimisation | 2312.13010 | Separation of code-gen agent from test-validator agent → OllamaCloudCodingProvider + ReviewGate separation |
| 4 | CodeTree: Agent-guided Tree Search for Code Generation | 2411.04329 | Tree search for code synthesis decisions → ironclaw wave-dispatch branching model |
| 5 | Self-Edit: Fault-Aware Code Editor for Code Generation | 2305.04087 | Code editor feedback on failed execution → FixAgent dispatch on ReviewGate fail |
| 6 | REprompt: Improving LLM-Generated Code in Automated Software Engineering | 2601.16507 | Reprompting with targeted feedback → FixAgent prompt construction strategy |
| 7 | Multi-SWE-bench: A Multilingual Benchmark for Repository-Level Code Changes | 2504.02605 | Repository-level code editing evaluation → rationale for git-worktree-per-task isolation |
| 8 | CRUST-Bench: A Comprehensive Benchmark for C-to-Safe-Rust Translation | 2504.15254 | Complex multi-file code transformation in isolation → OllamaResponseValidator diff validation |

## Synthesis: design decisions informed by prior art

### 1. ReviewGate MAX_GATE_ITERATIONS = 3 (from CodeCoR + Self-Edit)

CodeCoR (2501.07811) shows chain-of-repair converges in ≤3 iterations for 94% of tasks in their benchmark. Self-Edit (2305.04087) demonstrates fault-aware editing significantly reduces subsequent failures. This justifies:
- `MAX_GATE_ITERATIONS = 3` as a hard ceiling (not 5 or unlimited)
- FixAgent dispatch with targeted feedback from the gate failure (not blank reprompting)

### 2. Separation of coding worker from validator (from AgentCoder + CRUST-Bench)

AgentCoder (2312.13010) shows a test-executor agent separate from the code-generator improves correctness. CRUST-Bench (2504.15254) demonstrates complex transformations benefit from a second-pass validator. This directly justifies `OllamaResponseValidator` as a separate module from `OllamaCloudCodingProvider` — the same agent that wrote the code cannot validate its own safety.

### 3. Worker isolation per task (from Multi-SWE-bench + CodeTree)

Multi-SWE-bench (2504.02605) evaluates repository-level changes — their analysis shows context contamination across tasks is the primary source of merge conflicts in multi-agent systems. CodeTree (2411.04329) uses tree search with branching. This justifies git-worktree-per-task isolation (ironclaw-spine design, carried forward).

### 4. Single-source status (per Plan iter-4 R10 assessment)

All 8 citations are independent peer-reviewed papers (arXiv). QUANTUM assessed during iter-4: "multi-source confirmed, C2b cap does NOT apply." Cross-validation discipline (Blueprint Part IV) satisfied.

## Gap acknowledged at plan-time

The compression claim ("hours→minutes" for autonomous delivery vs manual) is a platform-design claim, not directly evidenced by the above papers. Canon XXXVI requires AYIN-measurable compression span. Phase 6 D8 benchmark will provide the evidence: `supervisor.poll_tick` + `decision_pipeline.layer_decision` + `ollama_worker.spawn/.complete` AYIN spans measured against equivalent manual time for the reference SMALL-tier plan.
