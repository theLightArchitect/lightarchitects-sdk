---
title: Agent Loop Patterns — Verified Research Catalogue v1.0
purpose: Map each ReAct-family loop pattern to a LightArchitects strategy and decide composition model
sources:
  - mhtml: "/Users/kft/Downloads/AI Agent Loop Patterns _ Re-Act-style variants.mhtml (cholakovit, 2026-03-20)"
  - context7: "/websites/agent-patterns_readthedocs_io_en (7350 snippets, Source Reputation: High)"
  - context7: "/anthropics/anthropic-sdk-typescript (292 snippets, Source Reputation: High)"
  - context7: "/nothflare/claude-agent-sdk-docs (821 snippets)"
  - paper: Yao et al. 2023 (ReAct, arxiv 2210.03629)
  - paper: Shinn et al. 2023 (Reflexion, arxiv 2303.11366)
  - paper: Park et al. 2023 (Generative Agents)
  - paper: Wei et al. 2022 (Chain-of-Thought, arxiv 2201.11903)
  - paper: Yao et al. 2023 (Tree of Thoughts, arxiv 2305.10601)
  - paper: Xu et al. 2023 (REWOO, arxiv 2305.18323)
  - paper: Kim et al. 2023 (LLM Compiler / LLMCompiler, arxiv 2312.04511)
  - paper: Zhou et al. 2023 (LATS, arxiv 2310.04406)
  - paper: Zhou et al. 2024 (Self-Discover, arxiv 2402.03620)
  - paper: Shao et al. 2024 (STORM, arxiv 2402.14207)
  - blog: Anthropic 2024 ("Building Effective Agents")
created: 2026-06-03
status: v1.0 enriched
---

# Agent Loop Patterns — Verified Catalogue v1.0

Catalog of 17 canonical agent loop patterns with structure, pseudocode, and trade-offs. Section 2 maps each to the 19 `STRATEGY_PROFILES` entries and recommends a composition model.

---

## Section 1 — Pattern Catalogue

### L1 — Plain ReAct (Yao 2023)

```
INPUT → THINK → ACT → OBSERVE → … → RESULT
```

```python
def react(query, tools, max_steps=10):
    scratchpad = []
    for step in range(max_steps):
        thought, action = llm.think_and_act(query, scratchpad)
        if action is None: return llm.finalize(query, scratchpad)
        observation = tools.execute(action)
        scratchpad.append((thought, action, observation))
    return llm.finalize(query, scratchpad)
```

**Best for**: factual Q&A with external data, simple lookups.
**Cost**: O(N) LLM calls for N tool uses.

---

### L2 — Conversational ReAct / ReSpAct

```
INPUT → THINK → (OPTIONAL SPEAK/ASK) → THINK → ACT → OBSERVE → … → FINAL
```

User-in-the-loop variant. The agent asks clarifying questions before acting.

**Best for**: ambiguous user requests, missing parameters.
**Cost**: extra round-trip per clarification.

---

### L3 — ReAct + Description

```
INPUT → THINK → (OPTIONAL DESCRIBE) → ACT → OBSERVE → … → FINAL
```

Adds narration of intent before each action — debug-friendly, compliance-friendly.

**Best for**: audit logs, UX where users need to see what's happening.

---

### L4 — Multi-Action ReAct (Anthropic parallel tool use)

```
INPUT → THINK → ACT (multiple parallel tool calls) → OBSERVE (all) → THINK → … → FINAL
```

```python
def multi_action_react(query, tools, max_steps=10):
    scratchpad = []
    for step in range(max_steps):
        thought, actions = llm.think_and_act_parallel(query, scratchpad)
        if not actions: return llm.finalize(query, scratchpad)
        observations = parallel_map(tools.execute, actions)  # independent only
        scratchpad.append((thought, actions, observations))
```

**Best for**: independent lookups (search + calculator + price).
**Risk**: ordering bugs when calls have hidden deps; large Observe payloads overflow context.

---

### L5 — Iterative ReAct (sequential rounds)

Strict sequential ReAct with a hard `max_rounds` cap. Same atomic loop as L1 but emphasis on budget control. Each round sees fresh tool output before next choice.

**Best for**: dependent steps, cost-controlled runs.

---

### L6 — Reflexion (Shinn 2023)

```
[trial loop] PLAN(memory) → EXECUTE → EVALUATE → if fail: REFLECT → store_lesson → retry
```

```python
def reflexion_loop(task, max_trials=3):
    reflection_memory = []
    for trial in range(max_trials):
        plan = llm.plan_with_memory(task, reflection_memory)
        outcome = llm.execute(task, plan)
        eval_result = llm.evaluate(task, outcome)
        if eval_result == "success": return outcome
        reflection = llm.reflect(task, plan, outcome, eval_result)
        reflection_memory.append(reflection)
    return llm.best_answer(task, reflection_memory, outcome)
```

**Best for**: tasks where verification is possible (tests, ground truth available), multi-attempt code/math.
**Cost**: +1 LLM call per trial for reflection.

---

### L7 — ReAct + Memory (Park 2023)

```
INPUT → READ_LTM → REASON → ACT → OBSERVE → WRITE_STM → … → CONSOLIDATE_LTM → FINAL
```

Long-term + short-term memory bracketing the core loop. Vector store keyed by session, OR structured slots, OR Mamba SSM state (our `HelixSessionMemory`).

**Best for**: long-horizon sessions, personalization.
**Risk**: stale memories poison future turns; needs TTL + retrieval with citations.

---

### L8 — ReAct with Planning (Plan-and-Execute)

```
INPUT → PLAN (decompose into subgoals) → [for each subgoal: ReAct loop] → SYNTHESIZE → FINAL
```

```python
def plan_execute(query, tools):
    plan = llm.decompose(query)
    results = []
    for subgoal in plan:
        scratchpad = []
        for step in range(max_per_subgoal):
            thought, action = llm.think_and_act(subgoal, scratchpad)
            if action is None: break
            obs = tools.execute(action)
            scratchpad.append((thought, action, obs))
        results.append(llm.finalize(subgoal, scratchpad))
    return llm.synthesize(query, results)
```

**Best for**: multi-step tasks where plan can be shown to operator for approval.
**Risk**: brittle if initial plan wrong → needs replanning.

---

### L9 — CoT ReAct (Wei 2022 + ReAct)

ReAct with explicit chain-of-thought in the Think step. Trace shows full reasoning before each action.

**Best for**: math, structured problems, audit-heavy domains.
**Cost**: verbose traces increase tokens.

---

### L10 — Tree-of-Thought + ReAct (Yao 2023b)

```
INPUT → BRANCH (k candidate thoughts) → SCORE → SELECT top → THINK → ACT → OBSERVE → … → FINAL
```

Search-over-reasoning. Generates candidate thoughts, scores them, commits to top branch.

**Best for**: puzzles, planning, when first guess is often wrong.
**Cost**: branch_factor × depth × LLM calls upfront.

---

### L11 — Sandbox Execution Agent

```
INPUT → THINK → COMPOSE_CMD_OR_SCRIPT → EXECUTE_IN_SANDBOX → OBSERVE(stdout/stderr/exit/artifacts) → … → FINAL
```

Real compute in container/microVM/restricted worker. Observation = stdout/stderr/exit code/artifacts.

**Best for**: coding tasks, data wrangling, reproducible runs.
**Runtimes**: E2B, Firecracker, gVisor, custom worker pool.
**Risk**: sandbox escape, escape costs, long log truncation needs.

---

### L12 — ReAct + Learning

```
INPUT → THINK → ACT → OBSERVE → (if reward) → UPDATE_POLICY → … → FINAL
```

Three modes: (a) store corrected strategies in a policy store (cheapest), (b) periodic fine-tune from feedback batches, (c) full RL from rewards.

**Best for**: systems that need to improve over time without manual prompt editing.
**Risk**: noisy feedback amplifies biases; needs eval harness and rollback.

---

### L13 — REWOO (Plan-Work-Observe-Open, Xu 2023)

```
INPUT → PLAN (with placeholders) → EXECUTE all tools (no LLM in loop) → SYNTHESIZE → FINAL
```

**Critical property**: **2 LLM calls** regardless of tool count, vs **N+1** for ReAct. Plan upfront with placeholders for tool outputs (`#node1`, `#node2`), execute tools serially or in parallel, then synthesize.

```python
def rewoo(query, tools):
    plan = llm.plan_with_placeholders(query)  # 1 LLM call
    # Plan: [{"tool": "search", "args": {"q": "X"}, "id": "node1"},
    #        {"tool": "calc", "args": {"expr": "#node1 * 2"}, "id": "node2"}]
    results = {}
    for step in plan:
        resolved_args = resolve_placeholders(step.args, results)
        results[step.id] = tools.execute(step.tool, resolved_args)
    return llm.synthesize(query, results)  # 1 LLM call
```

**Best for**: predictable workflows, cost-sensitive operation, batch processing.
**Constraint**: requires plan to be derivable upfront without intermediate reasoning.

---

### L14 — LLM Compiler (Kim 2023)

```
INPUT → PLAN_DAG → EXECUTE (topological order with parallelism) → SYNTHESIZE → FINAL
```

Like REWOO but with explicit **dependency graph**. Independent nodes run in parallel; dependent nodes wait. Three roles: planner (emits DAG), executor (topo-sort + parallel dispatch), synthesizer (final answer).

```python
def llm_compiler(query, tools):
    dag = llm.plan_dag(query)  # nodes with depends_on edges
    results = {}
    while not all_complete(dag, results):
        ready = [n for n in dag if all(d in results for d in n.depends_on)]
        with ThreadPoolExecutor() as pool:
            futures = {pool.submit(tools.execute, n): n for n in ready}
            for f in as_completed(futures):
                node = futures[f]
                results[node.id] = f.result()
    return llm.synthesize(query, results)
```

**Best for**: independent parallel lookups (weather + population + currency), latency-critical apps.
**Constraint**: needs LLM that can author dependency graphs reliably.

---

### L15 — LATS (Language Agent Tree Search, Zhou 2023)

```
LOOP: SELECT (UCB) → EXPAND (k children) → EVALUATE (LLM scorer) → BACKPROP → … → BEST_PATH → SYNTHESIZE
```

**MCTS (Monte Carlo Tree Search) over reasoning paths.** UCB selection trades exploration vs exploitation. LLM acts as both expander (generate children) and evaluator (score states).

```python
def lats(task, max_iter=10, expansions=3):
    root = TreeNode(task)
    for _ in range(max_iter):
        node = select_best_leaf_ucb(root)
        children = llm.expand(node, n=expansions)
        for child in children:
            child.value = llm.evaluate(child)
            child.visits = 1
        for child in children:
            backpropagate(child)
    best_path = extract_best(root)
    return llm.synthesize(best_path)
```

**Best for**: complex reasoning where the right answer requires search (math olympiad, planning puzzles, debugging strategies).
**Cost**: very expensive — many LLM calls for expansion + evaluation.

---

### L16 — Self-Discovery (Zhou 2024)

```
TASK → SELECT_MODULES (from a reasoning module library) → ADAPT_MODULES_TO_TASK → COMPOSE_PLAN → EXECUTE_PLAN → SYNTHESIZE
```

**Meta-reasoning**: instead of using one fixed reasoning pattern (e.g. CoT), the agent picks from a library of **reasoning modules** (e.g. "decompose problem", "use analogies", "think step by step", "consider opposing views") and composes a custom plan.

```python
def self_discovery(task, module_library, max_modules=3):
    selected = llm.select_modules(task, module_library, max_select=max_modules)
    adapted = [llm.adapt_module(task, m) for m in selected]
    reasoning_plan = llm.create_plan(task, adapted)
    step_results = []
    for step in reasoning_plan:
        result = llm.execute_step(task, step, step_results)
        step_results.append(result)
    return llm.synthesize(task, step_results)
```

**Best for**: tasks where the right reasoning approach is unknown a priori; adaptive systems.
**Constraint**: needs a curated module library; quality of selection matters.

---

### L17 — STORM (Shao 2024)

```
TOPIC → OUTLINE → PERSPECTIVES → QUESTIONS (per section × perspective) → RETRIEVE → SYNTHESIZE_SECTIONS → COMPILE_REPORT
```

**Multi-perspective research synthesis.** Generates an outline, identifies stakeholder perspectives (physicist, engineer, business analyst…), generates per-section per-perspective questions, retrieves, then writes each section.

```python
def storm(topic, perspectives, retrieval_tool):
    outline = llm.outline(topic)
    active_perspectives = select_perspectives(topic, perspectives)
    questions = {}
    for section in outline:
        for p in active_perspectives:
            questions[(section, p)] = llm.generate_questions(topic, section, p)
    search_results = {k: [retrieval_tool(q) for q in qs] for k, qs in questions.items()}
    sections = {sec: llm.synthesize_section(topic, sec, results) for sec, results in search_results.items()}
    return llm.compile_report(topic, sections)
```

**Best for**: structured research reports, knowledge-base authoring, multi-stakeholder analyses.

---

## Section 2 — STRATEGY_PROFILES → Loop Pattern Mapping

Mapping the 19 entries in `STRATEGY_PROFILES` to canonical loop patterns, with the recommended composition model for our `StrategyToolExecutor`.

Composition model legend:
- **D** = Stateless tool (current — one step, fresh state)
- **B** = Stateful tool (LoopState persists across LLM calls)
- **A** = Sub-agent (run to completion per tool call)
- **C** = Mode switch (replaces outer loop)
- **X** = Not LLM-callable (host-only)

| # | Strategy | Class | Loop pattern | Best fit | Why |
|---|---|---|---|---|---|
| 1 | `build` | A (auto) | **L11 Sandbox Execution + L8 Plan-and-Execute** | **A (sub-agent)** | Long-running multi-phase pipeline (plan/code/test/gate); operator wants the OUTCOME not intermediate ticks |
| 2 | `secure` | A | **L11 Sandbox + L4 Multi-Action** (SERAPH scan modules in parallel) | **A (sub-agent)** | Real scans take minutes; LLM shouldn't micromanage probe selection mid-run |
| 3 | `scrum` | A | **L17 STORM** (multi-perspective synthesis via sibling personalities) | **A (sub-agent)** | Need full perspectives gathered before synthesis; partial run = misleading |
| 4 | `enrich` | A | **L7 ReAct + Memory** (helix LTM write through 8 layers) | **A (sub-agent)** | Atomic operation on memory; partial enrichment = corrupt knowledge graph |
| 5 | `gate` | A | **L6 Reflexion** (each gate dimension is an evaluator) | **X (NOT LLM-callable)** | SERAPH VETO — privilege boundary; gate evaluates LLM output, can't be LLM-driven |
| 6 | `scope_governor` | A | **L6 Reflexion** (5-gate AND-validation) | **X (NOT LLM-callable)** | SERAPH VETO — circuit-breaker on the LLM itself |
| 7 | `react` (generic investigation) | B | **L1 / L5 Plain or Iterative ReAct** | **C (mode switch)** | If outer is already a ReAct loop, nesting is fake structure; switch modes instead |
| 8 | `bcra` (Bridging Conversation Reasoning Agents) | B | **L2 ReSpAct** | **A (sub-agent)** | Multi-party dialogue facilitation has its own lifecycle |
| 9 | `cove` (Chain-of-Verification) | B | **L6 Reflexion** | **B (stateful tool)** | Each verification step is cheap; LLM benefits from incremental visibility |
| 10 | `itt` (Investigation Task Tree) | B | **L15 LATS** (tree search over investigation hypotheses) | **A (sub-agent)** | Tree search is its own loop with a clear termination |
| 11 | `reflexion` | B | **L6 Reflexion** (canonical) | **A (sub-agent)** | Multi-trial loop with reflection memory — run to halt |
| 12 | `multipass` (verifier) | B | **L6 Reflexion** + **L13 REWOO** (verify all rules upfront) | **B (stateful tool)** | Per-rule verification is cheap; LLM may want to abandon if early-fail |
| 13 | `red_team` | B | **L11 Sandbox + L10 ToT** (explore attack trees) | **A (sub-agent)** | Adversarial probing has its own loop with hypothesis branching |
| 14 | `drain` (queue drainer) | B | **L13 REWOO** (plan all drain ops, execute, synthesize) | **A (sub-agent)** | Drain is a batch operation; LLM shouldn't tick through individual items |
| 15 | `ensemble` (multi-model voting) | B | **Parallelization/voting** (Anthropic) + **L14 LLM Compiler** | **A (sub-agent)** | Voting is one-shot — collect all, decide, return |
| 16 | `ach` (Analysis of Competing Hypotheses) | B | **L10 ToT + L17 STORM** | **A (sub-agent)** | Multi-hypothesis evaluation with evidence matrix; full structure required |
| 17 | `critique_refine` | B | **L6 Reflexion** (single-trial variant) | **B (stateful tool)** | Each refinement iteration is cheap; LLM should drive when to stop |
| 18 | `react_with_memory` | B | **L7 ReAct + Memory** (canonical) | **C (mode switch)** | Already the outer loop pattern — nesting is recursive |
| 19 | `sandbox_exec` | B | **L11 Sandbox** | **B (stateful tool)** | LLM should drive code/script iteratively; sandbox cost per call is bounded |

---

## Section 3 — Composition Reevaluation

### What this mapping reveals

**1. Most heavy strategies (8 of 19) want sub-agent (A) composition.** They're complete units of work with their own internal lifecycle:
- `build`, `secure`, `enrich`, `scrum` (the 4 in our default allowlist) — all sub-agent
- `bcra`, `itt`, `reflexion`, `red_team`, `drain`, `ensemble`, `ach` — all sub-agent

**2. A few strategies want stateful tool (B) composition.** These are incremental and the LLM benefits from controlling cadence:
- `cove`, `multipass`, `critique_refine`, `sandbox_exec`

**3. Two strategies want NOT-LLM-callable (X)** — confirmed by SERAPH SCRUM VETO:
- `gate`, `scope_governor`

**4. Two strategies want mode switch (C)** because nesting them in our outer ReAct loop creates recursion:
- `react`, `react_with_memory`

**5. Current implementation (D — stateless single-step) doesn't fit any strategy well.** It's the lowest common denominator that doesn't deliver value for any class.

### Refined architecture proposal

```
StrategyToolExecutor
├── default allowlist: {build, secure, scrum, enrich} — all sub-agent (A)
├── extended allowlist: {cove, multipass, critique_refine, sandbox_exec} — stateful tool (B)
├── mode-switch tools: {react, react_with_memory} — outer loop terminates and inner takes over (C)
└── NEVER in allowlist: {gate, scope_governor} — host-only (X) [confirmed by SERAPH VETO]
```

**Implementation outline for sub-agent (A) execution** in `strategy_tools.rs::execute`:

```rust
async fn execute_as_subagent(strategy: RegisteredStrategy, context: String) -> ToolOutput {
    let initial_state = LoopState::new(context);
    let budget = budget_from_profile(&strategy);  // StepCapped(N) from LoopProfile
    let runner = LoopRunner::new(strategy, budget);
    let chain = ctx.chain.child()?;  // increment depth for sub-agent
    let mut stream = runner.run(initial_state, chain, session_id);

    let mut final_outcome = None;
    let mut pause_request = None;
    while let Some(step_result) = stream.next().await {
        match step_result?.outcome {
            Outcome::Continue(state) => continue,
            Outcome::Halt(output) => { final_outcome = Some(output); break; }
            Outcome::Pause(_, hitl) => { pause_request = Some(hitl); break; }
        }
    }

    if let Some(hitl) = pause_request {
        // Surface to outer LLM as a structured pause observation
        return ToolOutput {
            content: json!({"status": "paused", "question": hitl.question, "options": hitl.options}),
            is_error: false,  // pause is not an error
        };
    }

    ToolOutput {
        content: json!({"status": "halt", "summary": output.summary, "artifacts": output.artifacts}),
        is_error: false,
    }
}
```

**Implementation outline for stateful tool (B) execution** with a session-keyed state store:

```rust
pub struct StatefulStrategyStore {
    states: DashMap<(SessionId, StrategyId), LoopState>,
    expiry: TtlConfig,  // GC after build session ends or N minutes idle
}

async fn execute_as_stateful_tool(&self, key, strategy, context) -> ToolOutput {
    let state = self.store.entry(key).or_insert_with(|| LoopState::new(context));
    let outcome = strategy.step(state.clone(), &step_ctx).await?;
    match outcome {
        Outcome::Continue(new_state) => {
            self.store.insert(key, new_state.clone());
            ToolOutput { content: json!({"status": "continue", "phase": new_state.phase, ...}), ... }
        }
        Outcome::Halt(output) => {
            self.store.remove(&key);  // strategy done — clean up
            ToolOutput { content: json!({"status": "halt", ...}), ... }
        }
        Outcome::Pause(state, hitl) => {
            self.store.insert(key, state);  // persist for resume
            ToolOutput { content: json!({"status": "paused", ...}), ... }
        }
    }
}
```

### Honest trade-offs in this design

| Issue | Mitigation |
|---|---|
| Sub-agent invocations can take minutes — outer LLM may time out | Budget caps from `LoopProfile.budget_policy` (StepCapped(20) for secure, StepCapped(50) for build) bound wall-clock |
| Stateful tools leak state if build session crashes | TTL eviction + cleanup on `BuildSession::drop()` |
| Mode switch loses the outer conversational context | Persist outer scratchpad to `HelixSessionMemory` before mode switch; restore on completion |
| HITL Pause from sub-agent needs operator surface | Existing `ResumeRegistry` + question tool surface; sub-agent pauses become webshell question chips |
| Chain depth — sub-agent inside ReAct inside copilot = depth 3+ | `ChainContext::child()` already caps at 7 hops |
| Different strategies want different providers (Claude for code, local for fast) | `LoopProfile` could carry a `provider_hint` field; strategy executor inherits or overrides |

---

## Section 4 — Recommended Build Sequence

Three small builds, ordered by dependency:

1. **`react-loop-subagent`** (SMALL, ~3 hours)
   - Replace `strategy.step()` in `StrategyToolExecutor::execute` with `LoopRunner::new(strategy, budget).run(...).collect_to_halt()`
   - Wire `Outcome::Pause` to surface as structured `ToolOutput` (status: paused)
   - Default allowlist remains `{build, secure, scrum, enrich}` — all four become sub-agents
   - Verify with the existing webshell test (Claude subscription path) — no parser changes needed
   - Outcome: heavy strategies now run to completion per tool call; LLM sees one observation per strategy

2. **`react-loop-stateful-store`** (SMALL, ~4 hours, depends on #1)
   - Add `SessionStrategyStore` (DashMap with TTL)
   - Extend allowlist: `{cove, multipass, critique_refine, sandbox_exec}` via stateful path
   - Cleanup hook on `BuildSession` drop
   - Outcome: lightweight verification strategies become LLM-controllable

3. **`react-loop-mode-switch`** (SMALL, ~3 hours, depends on #1)
   - Detect `react` / `react_with_memory` in the outer LLM's tool_use
   - Persist outer scratchpad to `HelixSessionMemory`, terminate outer loop
   - Spawn inner ReAct with its own loop runner
   - On inner halt, restore outer context and surface final answer
   - Outcome: recursive ReAct calls do useful work instead of getting trapped in nesting

---

## Section 5 — Open questions for HITL

1. Do we want **per-strategy provider hints** in `LoopProfile` (e.g. `secure` → Claude, `build` → local for code, `enrich` → cheap model for batching)?
2. Should **sub-agent budget escalation** be allowed (e.g. start StepCapped(10), if HITL Pause then operator approves StepCapped(20))?
3. Where should **stateful tool state cleanup** happen — on `BuildSession.drop`, on N minutes idle, or both?
4. For **mode switch**: should the inner ReAct see the outer scratchpad as initial context, or start fresh? (Affects token budget.)
5. Do we need an `ENGAGE("strategy_id")` slash command in addition to LLM tool dispatch — for the operator to drop into mode-switch directly?
