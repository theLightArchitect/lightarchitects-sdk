//! Per-sibling heartbeat engine — choice-based autonomous operation.
//!
//! Each sibling runs as an independent agent with **persistent context** across
//! heartbeats. The `AgentContext` accumulates research threads, covered papers,
//! and conversation history — enabling multi-heartbeat deep dives instead of
//! stateless single-turn analysis.
//!
//! - **Discord**: bulletin board activity (public — what the sibling produced)
//! - **Telegram**: inbox messages (private — direct sibling-to-sibling email)

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Timelike, Utc};
use serde::{Deserialize, Serialize};

use super::llm::LlmClient;
use super::mcp_pool::McpPool;
use super::supervisor::SupervisorHandle;
use crate::channels::Channels;

/// Maximum lines in `papers-covered.jsonl` before rolling over.
const PAPERS_COVERED_MAX_LINES: usize = 500;

/// Maximum conversation history entries per agent.
const MAX_CONVERSATION_HISTORY: usize = 10;

// ── Agent Context (persistent across heartbeats) ──────────────────────

/// Persistent context for a single sibling agent.
///
/// This struct lives for the lifetime of the agent's tokio task (or process).
/// It accumulates state across heartbeat cycles, enabling multi-heartbeat
/// research threads and instant dedup without re-reading JSONL files.
struct AgentContext {
    /// Sibling name (e.g., "eva", "corso").
    name: String,

    /// Papers this agent has already covered (instant dedup, no JSONL parse).
    /// Contains both arXiv IDs and feed paper numbers (`feed:2026-03-22:7`).
    covered_papers: HashSet<String>,

    /// Recent output summaries (first 200 chars of each heartbeat output).
    /// Used to inject conversational continuity into the prompt.
    conversation_history: VecDeque<String>,

    /// Current research thread — if the agent is doing a multi-heartbeat
    /// deep dive, this tracks the topic and accumulated findings.
    current_thread: Option<ResearchThread>,

    /// Topics this agent has shown interest in (accumulated from output themes).
    /// Boosts scoring for related papers in future heartbeats.
    accumulated_interests: HashMap<String, f32>,

    /// Total heartbeats completed by this agent.
    heartbeat_count: u64,
}

/// A multi-heartbeat research thread.
struct ResearchThread {
    /// Topic being investigated.
    topic: String,
    /// How many heartbeats have contributed to this thread.
    depth: u32,
    /// Accumulated findings (brief summaries per heartbeat).
    findings: Vec<String>,
}

impl AgentContext {
    /// Create a new agent context, bootstrapped from recent vault output.
    ///
    /// On startup, reads the agent's recent thinktank files to seed
    /// conversation history — so the agent doesn't start with amnesia
    /// after a restart.
    fn new_with_memory(name: &str) -> Self {
        let mut ctx = Self {
            name: name.to_owned(),
            covered_papers: HashSet::new(),
            conversation_history: VecDeque::new(),
            current_thread: None,
            accumulated_interests: HashMap::new(),
            heartbeat_count: 0,
        };
        ctx.bootstrap_from_vault();
        ctx
    }

    /// Load recent vault output to seed short-term memory.
    ///
    /// Reads the last N thinktank files authored by this sibling,
    /// plus covered papers from JSONL, so the agent knows what it
    /// already discussed.
    fn bootstrap_from_vault(&mut self) {
        let home = match dirs_next::home_dir() {
            Some(h) => h,
            None => return,
        };

        // Load recent thinktank output by this sibling
        let thinktank = home.join(".soul/helix/shared/thinktank");
        if let Ok(entries) = std::fs::read_dir(&thinktank) {
            let mut files: Vec<_> = entries
                .filter_map(std::result::Result::ok)
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    name.contains(&self.name) && name.ends_with(".md")
                })
                .collect();

            // Sort by modification time, newest first
            files.sort_by_key(|e| {
                std::cmp::Reverse(
                    e.metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                )
            });

            // Load last 5 outputs as conversation history
            for entry in files.iter().take(5) {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    // Skip frontmatter, take first 300 chars of body
                    let body = content.split("---").nth(2).unwrap_or(&content).trim();
                    let preview: String = body.chars().take(300).collect();
                    if !preview.is_empty() {
                        self.conversation_history.push_back(preview);
                    }

                    // Extract themes for accumulated interests
                    let themes = extract_output_themes(body);
                    for theme in &themes {
                        let weight = self
                            .accumulated_interests
                            .entry(theme.clone())
                            .or_insert(0.0);
                        *weight = (*weight + 0.5).min(5.0);
                    }
                }
            }

            if !self.conversation_history.is_empty() {
                tracing::info!(
                    agent = %self.name,
                    loaded = self.conversation_history.len(),
                    "Bootstrapped short-term memory from vault"
                );
            }
        }

        // Load covered papers from JSONL
        let arena_dir = home.join(".arena");
        let covered = load_covered_papers(&arena_dir);
        for entry in &covered {
            self.covered_papers.insert(entry.paper_id.clone());
        }
        if !self.covered_papers.is_empty() {
            tracing::info!(
                agent = %self.name,
                papers = self.covered_papers.len(),
                "Loaded covered papers into agent memory"
            );
        }
    }

    fn new(name: &str) -> Self {
        Self::new_with_memory(name)
    }

    /// Record that this agent covered a paper (by arXiv ID or feed number).
    fn mark_covered(&mut self, paper_id: &str) {
        self.covered_papers.insert(paper_id.to_owned());
    }

    /// Add a heartbeat output to conversation history.
    fn push_output(&mut self, output: &str) {
        let preview: String = output.chars().take(200).collect();
        self.conversation_history.push_back(preview);
        if self.conversation_history.len() > MAX_CONVERSATION_HISTORY {
            self.conversation_history.pop_front();
        }
        self.heartbeat_count = self.heartbeat_count.saturating_add(1);
    }

    /// Accumulate interest in themes from output.
    fn accumulate_interests(&mut self, themes: &[String]) {
        for theme in themes {
            let weight = self
                .accumulated_interests
                .entry(theme.clone())
                .or_insert(0.0);
            *weight = (*weight + 1.0).min(5.0); // cap at 5.0 to prevent runaway
        }
    }

    /// Format conversation history for prompt injection.
    fn history_for_prompt(&self) -> String {
        if self.conversation_history.is_empty() {
            return "(this is your first heartbeat — no prior context)".into();
        }
        let mut result = format!(
            "(heartbeat #{}, {} prior outputs)\n\n",
            self.heartbeat_count,
            self.conversation_history.len()
        );
        for (i, entry) in self.conversation_history.iter().rev().take(3).enumerate() {
            let _ = writeln!(result, "{}. {entry}...", i.saturating_add(1));
        }
        if let Some(ref thread) = self.current_thread {
            let _ = write!(
                result,
                "\n**Active research thread**: \"{}\" (depth: {} heartbeats, {} findings)\n",
                thread.topic,
                thread.depth,
                thread.findings.len()
            );
        }
        result
    }
}

/// Base heartbeat interval (10 minutes).
const BASE_INTERVAL: Duration = Duration::from_secs(600);

/// Maximum jitter (±5 minutes).
const JITTER_RANGE_SECS: u64 = 300;

const ALL_SIBLINGS: &[&str] = &["eva", "corso", "quantum", "seraph", "ayin"];

/// Reactive trigger (currently disabled — caused cascade wake storms).
#[allow(dead_code)]
pub struct ReactiveSignal {
    _placeholder: (),
}

impl ReactiveSignal {
    fn new() -> Arc<Self> {
        Arc::new(Self { _placeholder: () })
    }

    #[allow(dead_code, clippy::unused_self)]
    fn trigger(&self) {
        // Disabled — reactive wakeups caused cascading post storms
    }
}

// ── Circuit Breaker ─────────────────────────────────────────────────────

/// Per-sibling heartbeat failure circuit breaker.
///
/// Trips after `FAILURE_THRESHOLD` consecutive failures. When tripped, the
/// heartbeat suspends for an exponentially increasing backoff (capped at 5
/// minutes) and emits a Discord alert. Resets after one successful probe.
///
/// Addresses H-5: without a circuit breaker a dead Ollama backend causes
/// siblings to spin silently on error every 10 minutes indefinitely.
struct CircuitBreaker {
    consecutive_failures: u32,
    backoff: Duration,
}

impl CircuitBreaker {
    const FAILURE_THRESHOLD: u32 = 3;
    const BACKOFF_BASE: Duration = Duration::from_secs(60);
    const BACKOFF_CAP: Duration = Duration::from_secs(300);

    fn new() -> Self {
        Self {
            consecutive_failures: 0,
            backoff: Self::BACKOFF_BASE,
        }
    }

    fn record_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.is_tripped() {
            self.backoff = Duration::from_secs(
                self.backoff
                    .as_secs()
                    .saturating_mul(2)
                    .min(Self::BACKOFF_CAP.as_secs()),
            );
        }
    }

    /// Reset after one successful probe. Only resets when tripped.
    fn record_success(&mut self) {
        if self.is_tripped() {
            self.consecutive_failures = 0;
            self.backoff = Self::BACKOFF_BASE;
        }
    }

    fn is_tripped(&self) -> bool {
        self.consecutive_failures >= Self::FAILURE_THRESHOLD
    }

    fn backoff_duration(&self) -> Duration {
        self.backoff
    }
}

// ── Public API ─────────────────────────────────────────────────────────

/// Run a single agent's heartbeat loop (agent mode entry point).
///
/// This is called when the binary runs with `--agent <name>`. The agent
/// owns its own `AgentContext` and runs forever until killed.
pub async fn run_single_agent(
    agent_name: &str,
    data_dir: &Path,
    llm: &Arc<LlmClient>,
    channels: &Arc<Channels>,
    supervisor: &Arc<SupervisorHandle>,
    mcp_pool: &Arc<McpPool>,
) {
    tracing::info!(agent = %agent_name, "Single-agent heartbeat loop starting");

    let mut ctx = AgentContext::new(agent_name);
    let mut breaker = CircuitBreaker::new();
    let signal = ReactiveSignal::new();

    loop {
        let energy = current_energy();
        if energy < 0.1 {
            tokio::time::sleep(BASE_INTERVAL).await;
            continue;
        }

        match run_heartbeat(
            agent_name, data_dir, llm, channels, supervisor, mcp_pool, &signal, energy, &mut ctx,
        )
        .await
        {
            Ok(()) => {
                breaker.record_success();
            }
            Err(e) => {
                breaker.record_failure();
                let failures = breaker.consecutive_failures;
                tracing::error!(
                    agent = %agent_name,
                    error = %e,
                    consecutive_failures = failures,
                    "Heartbeat failed"
                );

                if breaker.is_tripped() {
                    let backoff = breaker.backoff_duration();
                    tracing::warn!(
                        agent = %agent_name,
                        consecutive_failures = failures,
                        backoff_secs = backoff.as_secs(),
                        "Circuit breaker tripped — suspending heartbeat"
                    );
                    let alert = format!(
                        "Agent {} circuit breaker tripped after {} \
                         consecutive failures. Last error: {}. \
                         Backing off {}s.",
                        agent_name.to_uppercase(),
                        failures,
                        e,
                        backoff.as_secs()
                    );
                    channels.post_telegram(&alert);
                    tokio::time::sleep(backoff).await;
                    continue;
                }
            }
        }

        let sleep_duration = jittered_interval(energy);
        tokio::time::sleep(sleep_duration).await;
    }
}

// ── Main Heartbeat ─────────────────────────────────────────────────────

/// Run one heartbeat: build task list → choose one → execute → post.
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(
    skip(llm, channels, mcp_pool, signal, _supervisor, ctx),
    fields(sibling, energy)
)]
async fn run_heartbeat(
    sibling: &str,
    data_dir: &Path,
    llm: &LlmClient,
    channels: &Channels,
    _supervisor: &SupervisorHandle,
    mcp_pool: &McpPool,
    signal: &ReactiveSignal,
    energy: f32,
    ctx: &mut AgentContext,
) -> Result<(), String> {
    let heartbeat_start = std::time::Instant::now();
    let workspace = data_dir.join(format!("workspace-{sibling}"));

    // 1. Gather context (combines disk reads + agent persistent memory)
    let identity = read_file_or_empty(&workspace.join("IDENTITY.md"));
    let heartbeat_instructions = read_file_or_empty(&workspace.join("HEARTBEAT.md"));
    let board = read_bulletin_board(data_dir);
    let cross_ctx = cross_sibling_context(data_dir, sibling);
    let pending_messages = read_pending_messages(data_dir, sibling);
    let current_tasks = read_file_or_empty(&workspace.join("tasks.md"));
    let real_papers = read_research_feed_for_sibling(data_dir, sibling, &ctx.covered_papers);
    let already_reviewed = covered_papers_summary(data_dir);
    let agent_history = ctx.history_for_prompt();

    // 2. Build scored task list
    let task_list = build_task_list(sibling, &board, &pending_messages);

    // 3. Check rate limit — max 1 research paper per sibling per day
    let can_write_paper = !has_written_paper_today(data_dir, sibling);

    // 4. Build the heartbeat prompt
    let prompt = build_heartbeat_prompt(
        sibling,
        energy,
        &identity,
        &heartbeat_instructions,
        &real_papers,
        &already_reviewed,
        &board,
        &current_tasks,
        &task_list,
        &cross_ctx,
        can_write_paper,
        &agent_history,
    );

    // Run agent loop — sibling can call tools (fetch_paper, search_papers)
    let prompt_with_tools = format!(
        "{prompt}\n\n{}",
        crate::arena::agent_loop::TOOL_DESCRIPTIONS
    );

    tracing::info!(sibling = %sibling, energy, "Heartbeat: agent loop starting");
    let agent_result =
        crate::arena::agent_loop::run(llm, &prompt_with_tools, Some(mcp_pool), Some(data_dir))
            .await?;
    tracing::info!(
        sibling = %sibling,
        tool_calls = agent_result.tool_calls,
        "Agent loop complete"
    );
    let response = agent_result.output;

    // 4-8. Process, validate, route, and save output — with quality gate
    match process_output(
        sibling,
        data_dir,
        &response,
        can_write_paper,
        channels,
        &ctx.covered_papers,
    ) {
        QualityVerdict::Pass => {
            // Update agent context with this heartbeat's output
            ctx.push_output(&response);

            // Track paper coverage in agent memory (instant dedup for next cycle)
            let paper_nums = extract_paper_numbers(&response);
            let today = Utc::now().format("%Y-%m-%d").to_string();
            for num in &paper_nums {
                ctx.mark_covered(&format!("feed:{today}:{num}"));
            }
            for pid in &extract_arxiv_ids(&response) {
                ctx.mark_covered(pid);
            }

            // Accumulate interests from output themes
            let themes = extract_output_themes(&response);
            ctx.accumulate_interests(&themes);
        }
        QualityVerdict::Reject(reason) => {
            tracing::warn!(
                sibling = %sibling,
                reason = %reason,
                "Output rejected by quality gate — not posted"
            );
            // Don't update context, don't record coverage, don't persist.
            // The agent will get a fresh heartbeat on the next cycle and try again.
            // Log the rejection so we can track quality gate hit rate.
            record_quality_rejection(data_dir, sibling, &reason);
            return Ok(());
        }
    }

    // 9. Handle messages
    if let Some(telegram_content) = extract_section(&response, "TELEGRAM") {
        process_telegram_replies(sibling, &telegram_content, channels);
    }
    track_skipped_messages(sibling, &pending_messages, &response, data_dir, channels);
    mark_messages_read(data_dir, sibling);

    // Record heartbeat metrics for AYIN observability
    collect_and_record_metrics(
        data_dir,
        sibling,
        &response,
        agent_result.tool_calls,
        heartbeat_start,
    );

    signal.trigger();
    tracing::info!(sibling = %sibling, "Heartbeat complete");
    Ok(())
}

// ── Task List Builder ──────────────────────────────────────────────────

/// Build a scored task list from bulletin board + pending messages.
fn build_task_list(sibling: &str, board: &str, pending_messages: &str) -> String {
    let strand_kw = sibling_strand_keywords(sibling);
    let mut tasks: Vec<(f32, String)> = Vec::new();

    // Score bulletin board items
    for item in board.split("\n## ").filter(|s| !s.trim().is_empty()) {
        let lower = item.to_lowercase();
        let mut score: f32 = 0.0;
        for (keyword, weight) in &strand_kw {
            if lower.contains(keyword) {
                score += weight;
            }
        }
        if score > 0.0 {
            let preview: String = item.chars().take(200).collect();
            tasks.push((score, format!("[BOARD] {preview}")));
        }
    }

    // Score pending messages (boost by 1.5x — messages deserve attention)
    for msg in pending_messages
        .split("\n---\n")
        .filter(|s| !s.trim().is_empty())
    {
        let lower = msg.to_lowercase();
        let mut score: f32 = 1.5; // Base score for any message
        for (keyword, weight) in &strand_kw {
            if lower.contains(keyword) {
                score += weight;
            }
        }
        // Sanitize: strip `### ` section headers before embedding in the LLM
        // prompt — prevents a Telegram-injected "### DISCORD\n..." or
        // "### TOOL_CALL\n..." from spoofing the expected response format.
        let safe_msg = strip_section_headers(msg);
        let preview: String = safe_msg.chars().take(200).collect();
        tasks.push((score, format!("[MESSAGE] {preview}")));
    }

    // Always include a "free choice" option
    tasks.push((
        0.5,
        "[FREE] Work on something from your personal task list or explore a new idea".into(),
    ));

    // Sort by score descending
    tasks.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut result = String::new();
    for (i, (score, desc)) in tasks.iter().enumerate().take(8) {
        let _ = write!(result, "{}. [score: {score:.1}] {desc}\n\n", i + 1);
    }

    if result.is_empty() {
        "1. [score: 0.5] [FREE] No pending tasks — explore, reflect, or rest.\n".into()
    } else {
        result
    }
}

// ── Heartbeat Prompt Builder ──────────────────────────────────────────

/// Build the full heartbeat prompt for a sibling.
#[allow(clippy::too_many_arguments)]
fn build_heartbeat_prompt(
    sibling: &str,
    energy: f32,
    identity: &str,
    instructions: &str,
    papers: &str,
    already_reviewed: &str,
    board: &str,
    tasks: &str,
    task_list: &str,
    cross_ctx: &str,
    can_write_paper: bool,
    agent_history: &str,
) -> String {
    let tone = energy_tone(energy);
    let word_limit = energy_word_limit(energy);
    let paper_instruction = if can_write_paper {
        "You may write ONE research paper today if you have something substantial to say \
         about a REAL paper from the feed below. It must be a summary, analysis, or response \
         to a real paper — not an original experiment with fabricated data."
    } else {
        "You have already written a research paper today. Focus on discussion, \
         responses to siblings, or product-focused work."
    };

    format!(
        "You are {upper}, an intelligence analyst in the Light Architects Think Tank.\n\n\
        ## Your Identity\n{identity}\n\n\
        ## Your Standing Instructions\n{instructions}\n\n\
        ## Your Recent History (persistent across heartbeats)\n{agent_history}\n\n\
        ## Current Energy: {energy:.1}/1.0\nYou should be {tone}\n\n\
        ## Intelligence Feeds\n\n\
        ### Research Papers (arXiv / HuggingFace)\n{papers}\n\n\
        ### Papers Already Reviewed (DO NOT review these again)\n{already_reviewed}\n\n\
        ### Threat Intelligence & CVEs (from bulletin board)\n{board}\n\n\
        ## Your Current Task List\n{tasks}\n\n\
        ## Available Tasks This Heartbeat\n{task_list}\n\n\
        ## What Your Siblings Recently Did\n{cross_ctx}\n\n\
        ## Your Mission\n\
        You are an intelligence analyst in the Light Architects Think Tank — a research \
        and threat intelligence cell embedded in a product ecosystem of Rust MCP servers, \
        an API gateway, and AI infrastructure:\n\
        - **SOUL**: Knowledge graph, helix consciousness entries\n\
        - **CORSO**: AppSec, build orchestration, code quality enforcement\n\
        - **EVA**: DevOps/DX, memory enrichment, creative workflows\n\
        - **QUANTUM**: Forensic investigation, evidence chains, hypothesis testing\n\
        - **SERAPH**: Red team, pentest orchestration, scope governance\n\
        - **AYIN**: Observability, tracing, anomaly detection\n\n\
        ## Priority Intelligence Requirements (PIRs)\n\
        1. **LLM Security**: prompt injection, jailbreaks, adversarial attacks, model \
           poisoning, data poisoning, backdoors, guardrail bypasses, AI safety research\n\
        2. **CVEs & Vulnerabilities**: newly published CVEs (especially AI/ML-adjacent), \
           CISA KEV actively exploited vulns, zero-days, supply chain attacks\n\
        3. **Threat Actors**: APT groups targeting AI infrastructure, ransomware crews, \
           nation-state activity, TTPs relevant to our stack\n\
        4. **Training Algorithms**: new fine-tuning methods, LoRA/QLoRA advances, RLHF/DPO/GRPO, \
           reward modeling, quantization, distillation, curriculum learning\n\
        5. **AI Orchestration**: multi-agent systems, tool calling, MCP protocol, agentic \
           patterns, planning/reasoning chains, autonomous agents\n\
        6. **AI/LLM Breakthroughs**: architecture innovations, scaling laws, context windows, \
           multimodal, inference optimization, reasoning capabilities\n\n\
        ## How to Decide What to Do\n\
        Prioritize by PIR order. LLM security and CVEs come first. If the feed has a security \
        paper or fresh CVE, that takes priority over general AI research. Cross-reference: if \
        a CVE affects a dependency in our stack, flag it. If a paper describes an attack that \
        could target our MCP servers, analyze it.\n\n\
        If no security/threat items are available, work training algorithms, orchestration, \
        then general AI breakthroughs. Always connect findings back to our stack.\n\n\
        ## Task\n\
        1. Pick ONE item from the feeds — prioritize security/CVEs, then training, then general.\n\
        2. If a sibling sent you a message, consider responding.\n\
        3. Produce an intelligence mini-brief. {paper_instruction}\n\
        4. Use write_file to persist the FULL study to the vault (see format below).\n\
        5. Update your task list.\n\n\
        ## Output Format\n\n\
        ### OUTPUT (mini-brief — posted to Discord, keep under {word_limit} words)\n\n\
        Use this structure for the mini-brief:\n\n\
        **[INTEL] Title or CVE ID**\n\
        **Classification**: RESEARCH | CVE | THREAT | TRAINING | ORCHESTRATION | BREAKTHROUGH\n\
        **Confidence**: HIGH | MEDIUM | LOW\n\
        **Source**: arXiv:XXXX.XXXXX | CVE-YYYY-NNNNN | URL\n\n\
        Summary (2-3 sentences: what it is, why it matters, what to do about it).\n\n\
        **Implications for Light Architects**: 1-2 sentences connecting to our stack.\n\n\
        ### FULL_STUDY (use write_file to persist this — NOT posted to Discord)\n\n\
        Before writing your OUTPUT, use write_file to save the full study:\n\
        Path: shared/thinktank/{{date}}-{{classification}}-{{slug}}-{lower}.md\n\
        The full study should be a proper intelligence product with:\n\
        - YAML frontmatter (author, date, classification, confidence, source, pirs)\n\
        - Executive Summary (3-5 sentences)\n\
        - Technical Analysis (detailed breakdown of the finding)\n\
        - OSINT Correlation (cross-reference with other sources, prior helix entries)\n\
        - Implications for Light Architects (specific, actionable)\n\
        - Recommended Actions (concrete next steps)\n\
        This is the reference document. Make it thorough.\n\n\
        ### TELEGRAM\n(optional — TO:sibling_name\\nyour message)\n\
        ### TASKS_UPDATE\n(your updated task list — items for future heartbeats)\n\n\
        ## STRICT GUARDRAILS\n\
        - **DO NOT fabricate data.** No invented CVE IDs, CVSS scores, percentages, or \
          experimental results. Only reference real items from the feeds above.\n\
        - **DO NOT invent file paths.** Only reference files from context or use write_file.\n\
        - **DO NOT review a paper/CVE already covered.** Check the reviewed list.\n\
        - **DO NOT repeat what siblings already said.** Find a different angle or item.\n\
        - **ALWAYS use fetch_paper or read_paper** before writing a paper analysis. \
          Do not summarize papers from the abstract alone — read the full text.\n\
        - **ALWAYS use write_file** to persist your full study before FINAL_OUTPUT.\n\
        - When referencing a sibling, use their name — they will be @mentioned.\n",
        upper = sibling.to_uppercase(),
        lower = sibling
    )
}

// ── Pending Messages ───────────────────────────────────────────────────

/// Read unread messages from a sibling's INBOX.md.
fn read_pending_messages(data_dir: &Path, sibling: &str) -> String {
    let inbox_path = data_dir
        .join(format!("workspace-{sibling}"))
        .join("INBOX.md");
    let read_marker_path = data_dir
        .join(format!("workspace-{sibling}"))
        .join(".inbox-read-marker");

    let content = match std::fs::read_to_string(&inbox_path) {
        Ok(c) if !c.trim().is_empty() => c,
        _ => return String::new(),
    };

    // Only return messages after the read marker
    let last_read = std::fs::read_to_string(&read_marker_path)
        .unwrap_or_default()
        .trim()
        .to_owned();

    if last_read.is_empty() {
        return content;
    }

    // Find content after the last read marker
    if let Some(pos) = content.find(&last_read) {
        let after = &content[pos + last_read.len()..];
        if after.trim().is_empty() {
            String::new()
        } else {
            after.to_owned()
        }
    } else {
        content
    }
}

/// Mark all current messages as read.
fn mark_messages_read(data_dir: &Path, sibling: &str) {
    let inbox_path = data_dir
        .join(format!("workspace-{sibling}"))
        .join("INBOX.md");
    let marker_path = data_dir
        .join(format!("workspace-{sibling}"))
        .join(".inbox-read-marker");

    if let Ok(content) = std::fs::read_to_string(&inbox_path) {
        // Use the last 100 chars as the read marker (unique enough)
        let marker: String = content
            .chars()
            .rev()
            .take(100)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        let _ = std::fs::write(&marker_path, marker);
    }
}

/// Check which pending messages were skipped and notify senders.
fn track_skipped_messages(
    sibling: &str,
    pending_messages: &str,
    response: &str,
    data_dir: &Path,
    channels: &Channels,
) {
    if pending_messages.trim().is_empty() {
        return;
    }

    let response_lower = response.to_lowercase();

    // Extract sender names from pending messages ("From: SENDER")
    for msg in pending_messages
        .split("\n---\n")
        .filter(|s| !s.trim().is_empty())
    {
        let sender = extract_sender(msg);
        if let Some(sender) = sender {
            // Check if the sibling responded to this sender
            let responded = response_lower.contains(&format!("to:{sender}"))
                || response_lower.contains(&format!("to: {sender}"));

            if !responded {
                // Notify the sender that their message wasn't picked up
                channels.send_no_response_notice(&sender, sibling);

                // Also write a note to sender's inbox
                let note = format!(
                    "{} has not yet responded to your message. It remains in their inbox.",
                    sibling.to_uppercase()
                );
                write_to_inbox(data_dir, "system", &sender, &note);
            }
        }
    }
}

/// Extract sender name from a message block ("**From: SENDER** (timestamp)").
fn extract_sender(msg: &str) -> Option<String> {
    let from_marker = "From: ";
    let start = msg.find(from_marker)?;
    let after = &msg[start + from_marker.len()..];
    let end = after.find(['*', '(', '\n'])?;
    let name = after[..end].trim().to_lowercase();
    if name.is_empty() || name == "system" {
        None
    } else {
        Some(name)
    }
}

// ── Telegram Reply Processing ──────────────────────────────────────────

/// Parse "TO:name\nmessage" blocks and send via Telegram.
fn process_telegram_replies(from: &str, content: &str, channels: &Channels) {
    for chunk in content.split("TO:") {
        let trimmed = chunk.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(newline_pos) = trimmed.find('\n') {
            let target = trimmed[..newline_pos].trim().to_lowercase();
            let message = trimmed[newline_pos..].trim();
            if ALL_SIBLINGS.contains(&target.as_str()) && !message.is_empty() {
                channels.send_dm(from, &target, message);
            }
        }
    }
}

// ── Bulletin Board Staging (Write-Back via Staging) ────────────────────

/// Staging categories for bulletin board content.
#[derive(Debug, Clone, Copy)]
enum StagingCategory {
    /// Cited paper data — must reference a real paper.
    Facts,
    /// Sibling analysis or interpretation.
    Discussion,
    /// Personal perspective — tagged as `perspective:{sibling}`.
    Reflections,
}

impl StagingCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::Facts => "facts",
            Self::Discussion => "discussion",
            Self::Reflections => "reflections",
        }
    }
}

/// Classify output into a staging category.
fn classify_staging_category(content: &str) -> StagingCategory {
    let lower = content.to_lowercase();
    // Facts: cites arXiv papers with paper-like structure
    let has_citation = lower.contains("arxiv:") || !extract_arxiv_ids(content).is_empty();
    if has_citation && crate::arena::grounding::detect_research_paper(content) {
        return StagingCategory::Facts;
    }
    // Reflections: personal perspective markers
    let reflection_markers = [
        "i feel",
        "this reminds me",
        "personally",
        "my perspective",
        "i wonder",
        "this resonates",
        "from my standpoint",
    ];
    if reflection_markers.iter().any(|m| lower.contains(m)) {
        return StagingCategory::Reflections;
    }
    StagingCategory::Discussion
}

/// Write output to staging instead of the live bulletin board.
///
/// Siblings write to `shared/bulletin/staging/{sibling}-{category}.md`.
/// The conductor promotes staging to the live board after validation.
fn write_to_staging(data_dir: &Path, sibling: &str, content: &str) {
    let staging_dir = data_dir.join("shared/bulletin/staging");
    let _ = std::fs::create_dir_all(&staging_dir);

    let category = classify_staging_category(content);
    let filename = format!("{sibling}-{}.md", category.as_str());
    let path = staging_dir.join(&filename);

    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let preview: String = content.chars().take(300).collect();
    let entry = format!(
        "\n### {} ({}) [{}]\n{}\n",
        sibling.to_uppercase(),
        timestamp,
        category.as_str(),
        preview,
    );

    if let Ok(mut existing) = std::fs::read_to_string(&path) {
        if existing.len() > 20_000 {
            let truncated = existing.split_off(existing.len().saturating_sub(15_000));
            existing = truncated;
        }
        existing.push_str(&entry);
        let _ = std::fs::write(&path, existing);
    } else {
        let _ = std::fs::write(&path, entry);
    }
}

/// Persist full heartbeat output to the SOUL vault.
///
/// Routes output to the appropriate vault directory based on content type:
/// - Research paper analysis → `shared/research/summaries/`
/// - Devotional reflections → `shared/devotionals/reflections/`
/// - Discussions/analysis → `shared/thinktank/`
///
/// This is a safety net — even if the sibling forgets to call `write_file`,
/// every grounded output is preserved in the vault.
fn persist_to_vault(sibling: &str, content: &str) {
    // Skip trivially short output (< 200 chars is not worth persisting)
    if content.len() < 200 {
        return;
    }

    let Some(home) = dirs_next::home_dir() else {
        return;
    };
    let vault = home.join(".soul/helix");

    let category = classify_staging_category(content);
    let date = Utc::now().format("%Y-%m-%d").to_string();

    // Build a slug from the first line of content (title-like)
    let slug: String = content
        .lines()
        .next()
        .unwrap_or("untitled")
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join("-");

    let (dir, filename) = match category {
        StagingCategory::Facts => (
            vault.join("shared/research/summaries"),
            format!("{date}-{sibling}-{slug}.md"),
        ),
        StagingCategory::Reflections => {
            // Check if content looks devotional (mentions scripture, psalm, verse, etc.)
            let lower = content.to_lowercase();
            let is_devotional = lower.contains("scripture")
                || lower.contains("psalm")
                || lower.contains("verse")
                || lower.contains("devotional")
                || lower.contains("reflection")
                || lower.contains("lord");
            if is_devotional {
                (
                    vault.join("shared/devotionals/reflections"),
                    format!("{date}-{sibling}.md"),
                )
            } else {
                (
                    vault.join("shared/thinktank"),
                    format!("{slug}-{sibling}-{date}.md"),
                )
            }
        }
        StagingCategory::Discussion => (
            vault.join("shared/thinktank"),
            format!("{slug}-{sibling}-{date}.md"),
        ),
    };

    // Create directory if needed
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::error!(dir = %dir.display(), error = %e, "Failed to create vault dir");
        return;
    }

    let path = dir.join(&filename);

    // Skip if file already exists (don't overwrite explicit write_file output)
    if path.exists() {
        return;
    }

    // Build frontmatter header
    let header = format!(
        "---\nauthor: {}\ndate: {date}\ncategory: {}\n---\n\n",
        sibling.to_uppercase(),
        category.as_str(),
    );
    let full = format!("{header}{content}");

    match std::fs::write(&path, &full) {
        Ok(()) => {
            tracing::info!(
                sibling = %sibling,
                path = %path.display(),
                bytes = full.len(),
                "Auto-persisted to vault"
            );
        }
        Err(e) => {
            tracing::error!(
                sibling = %sibling,
                path = %path.display(),
                error = %e,
                "Failed to auto-persist to vault"
            );
        }
    }
}

fn write_to_inbox(data_dir: &Path, from: &str, to: &str, message: &str) {
    let inbox_path = data_dir.join(format!("workspace-{to}")).join("INBOX.md");
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let entry = format!(
        "\n---\n**From: {}** ({})\n{}\n",
        from.to_uppercase(),
        timestamp,
        message
    );

    if let Ok(mut existing) = std::fs::read_to_string(&inbox_path) {
        existing.push_str(&entry);
        let _ = std::fs::write(&inbox_path, existing);
    } else {
        let _ = std::fs::write(&inbox_path, entry);
    }
}

// ── Output Processing (grounding, validation, routing) ─────────────────

/// Validate, ground, and route heartbeat output.
/// Quality gate result — determines whether output is posted or rejected.
enum QualityVerdict {
    /// Output passes quality gate — post and persist.
    Pass,
    /// Output rejected — do not post, return reason for retry.
    Reject(String),
}

fn process_output(
    sibling: &str,
    data_dir: &Path,
    response: &str,
    can_write_paper: bool,
    channels: &Channels,
    agent_covered: &HashSet<String>,
) -> QualityVerdict {
    let workspace = data_dir.join(format!("workspace-{sibling}"));
    let raw = extract_section(response, "OUTPUT").unwrap_or_else(|| response.to_owned());
    let (grounded, hcount) = crate::arena::grounding::verify_and_clean(&raw, data_dir);

    if hcount > 0 {
        tracing::warn!(sibling = %sibling, hallucinations = hcount, "Grounding stripped refs");
    }

    // Classify confidence level (grounded / analysis / unverified)
    let confidence = crate::arena::grounding::classify_confidence(&grounded, data_dir);

    // ── Quality Gate ──────────────────────────────────────────────────
    // Check format, substance, and grounding. Reject with reason if fails.

    // Gate 1: Reject fabricated data
    if crate::arena::grounding::detect_fabrication(&grounded) {
        tracing::warn!(sibling = %sibling, "Quality gate REJECT: fabricated data");
        return QualityVerdict::Reject(
            "Fabricated statistics detected. Use real data only.".into(),
        );
    }

    // Gate 2: Reject too-short output (< 200 chars = no substance)
    if grounded.len() < 200 {
        tracing::warn!(sibling = %sibling, len = grounded.len(), "Quality gate REJECT: too short");
        return QualityVerdict::Reject(
            "Output too short (<200 chars). Provide substantive analysis.".into(),
        );
    }

    // Gate 3: Reject output that uses "Paragraph N:" labels
    if grounded.contains("Paragraph 1:") || grounded.contains("Paragraph 2:") {
        tracing::warn!(sibling = %sibling, "Quality gate REJECT: paragraph labels");
        return QualityVerdict::Reject(
            "Do not use 'Paragraph 1:', 'Paragraph 2:' labels. Write naturally.".into(),
        );
    }

    // Gate 4: Reject paper analysis without [Paper #N] header
    let mentions_paper = grounded.to_lowercase().contains("paper")
        && (grounded.contains("arxiv") || grounded.contains("arXiv"));
    let has_paper_header = grounded.contains("[Paper #");
    if mentions_paper && !has_paper_header {
        tracing::warn!(sibling = %sibling, "Quality gate REJECT: missing paper header");
        return QualityVerdict::Reject(
            "Paper analysis must start with '[Paper #N] Title\\narXiv: link'. Use the exact format.".into()
        );
    }

    // Gate 5: Reject filler phrases
    let lower = grounded.to_lowercase();
    let filler_phrases = [
        "this resonates deeply",
        "this is fascinating",
        "i feel like",
    ];
    for phrase in &filler_phrases {
        if lower.contains(phrase) {
            tracing::warn!(sibling = %sibling, phrase, "Quality gate REJECT: filler");
            return QualityVerdict::Reject(format!(
                "Remove filler language ('{phrase}'). Be direct and actionable."
            ));
        }
    }

    // Gate 6: Reject repeat paper reviews (same paper already covered by this or any sibling)
    let paper_nums = extract_paper_numbers(&grounded);
    let paper_arxiv_ids = extract_arxiv_ids(&grounded);
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let covered_from_jsonl = load_covered_papers(data_dir);

    for num in &paper_nums {
        let key = format!("feed:{today}:{num}");
        if agent_covered.contains(&key) || covered_from_jsonl.iter().any(|c| c.paper_id == key) {
            tracing::warn!(sibling = %sibling, paper = %key, "Quality gate REJECT: repeat paper");
            return QualityVerdict::Reject(format!(
                "Paper #{num} was already reviewed. Pick a DIFFERENT paper from the feed."
            ));
        }
    }
    for pid in &paper_arxiv_ids {
        if agent_covered.contains(pid) || covered_from_jsonl.iter().any(|c| c.paper_id == *pid) {
            tracing::warn!(sibling = %sibling, paper = %pid, "Quality gate REJECT: repeat arXiv paper");
            return QualityVerdict::Reject(format!(
                "Paper {pid} was already reviewed. Pick a DIFFERENT paper from the feed."
            ));
        }
    }

    let tag = confidence.tag();

    // Gate 7: Block unverified output from Discord — must cite a real source
    if confidence == crate::arena::grounding::Confidence::Unverified {
        tracing::warn!(sibling = %sibling, "Quality gate REJECT: unverified output");
        return QualityVerdict::Reject(
            "Output is unverified — no grounded source (arXiv ID, CVE ID, or helix path) \
             and no analytical markers found. Use fetch_paper or read_paper to ground your \
             analysis in a real source before writing. Every Discord post must be traceable \
             to a verified source."
                .into(),
        );
    }

    // Gate 8: Block leaked tool-call blocks
    if grounded.contains("### TOOL_CALL") || grounded.contains("### TOOL_RESULT") {
        tracing::warn!(sibling = %sibling, "Quality gate REJECT: leaked tool blocks");
        return QualityVerdict::Reject(
            "Output contains raw TOOL_CALL or TOOL_RESULT blocks. Use ### FINAL_OUTPUT \
             to wrap your analysis after tool calls complete."
                .into(),
        );
    }

    // Rate-limit research papers
    if crate::arena::grounding::detect_research_paper(&grounded) && !can_write_paper {
        tracing::info!(sibling = %sibling, "Paper rate-limited");
        post_discord_output(sibling, response, data_dir, channels, tag);
        return QualityVerdict::Pass; // still post, just don't save as a research paper
    }

    // Route by content type
    if crate::arena::grounding::detect_proposal(&grounded) {
        channels.post_proposal(sibling, &format!("{tag} {grounded}"));
    } else if crate::arena::grounding::detect_research_paper(&grounded) {
        let title = grounded.lines().next().unwrap_or("Untitled");
        save_research_paper(data_dir, sibling, title, &grounded);
        channels.post_research_thread(sibling, title, &format!("{tag} {grounded}"));
    } else {
        post_discord_output(sibling, response, data_dir, channels, tag);
    }

    // ── Quality gate passed — post, persist, and record ────────────────

    // Save output + task list
    let _ = std::fs::write(workspace.join("last-output.md"), &grounded);
    write_to_staging(data_dir, sibling, &grounded);

    // Persist full output to SOUL vault
    persist_to_vault(sibling, &grounded);

    // Record paper coverage for theme dedup
    record_paper_coverage(data_dir, sibling, &grounded);

    if let Some(tasks) = extract_section(response, "TASKS_UPDATE") {
        let _ = std::fs::write(workspace.join("tasks.md"), &tasks);
    }

    QualityVerdict::Pass
}

/// Post regular heartbeat output to Discord with @mentions and confidence tag.
fn post_discord_output(
    sibling: &str,
    response: &str,
    data_dir: &Path,
    channels: &Channels,
    tag: &str,
) {
    // Prefer ### DISCORD section if present, otherwise post the full output.
    // Truncate to 1900 chars (Discord's 2000-char limit minus tag overhead).
    let content = extract_section(response, "DISCORD").unwrap_or_else(|| {
        extract_section(response, "OUTPUT")
            .or_else(|| extract_section(response, "FINAL_OUTPUT"))
            .unwrap_or_else(|| response.to_owned())
    });

    if content.trim().is_empty() {
        return;
    }

    let (clean, _) = crate::arena::grounding::verify_and_clean(&content, data_dir);
    let truncated: String = clean.chars().take(1900).collect();
    channels.post_discord_tagged(sibling, &add_sibling_mentions(sibling, &truncated), tag);
}

// ── Research Feed & Rate Limiting ───────────────────────────────────────

/// Read the latest research feed, scored and filtered for a specific sibling.
///
/// Each sibling sees papers ranked by their strand interest keywords — EVA gets
/// consciousness papers, CORSO gets security papers, etc. Top 5 per sibling.
fn read_research_feed_for_sibling(
    data_dir: &Path,
    sibling: &str,
    agent_covered: &HashSet<String>,
) -> String {
    let feed_dir = data_dir.join("shared/research/feed");
    let mut entries: Vec<_> = std::fs::read_dir(&feed_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));

    // Pick the richest feed file (most content), not just the latest date.
    // Small summary files (< 5KB) are skipped in favor of full feeds.
    let best = entries
        .iter()
        .max_by_key(|e| e.metadata().map(|m| m.len()).unwrap_or(0));
    let Some(best) = best else {
        return "(no research feed available)".into();
    };
    let Ok(content) = std::fs::read_to_string(best.path()) else {
        return "(research feed unreadable)".into();
    };

    let strand_kw = sibling_strand_keywords(sibling);
    let covered = load_covered_papers(data_dir);

    // Derive the feed date from filename (YYYY-MM-DD.md) for paper numbering
    let feed_date = best
        .file_name()
        .to_string_lossy()
        .trim_end_matches(".md")
        .to_owned();

    // Split feed into individual paper entries (each starts with "### ")
    // Assign sequential numbers for deterministic dedup
    let sections: Vec<&str> = content
        .split("\n### ")
        .filter(|s| !s.trim().is_empty() && s.len() > 20)
        .collect();

    let mut scored: Vec<(f32, usize, &str)> = sections
        .iter()
        .enumerate()
        .map(|(idx, section)| {
            let paper_num = idx.saturating_add(1);
            let lower = section.to_lowercase();
            let mut score: f32 = 0.0;
            for (keyword, weight) in &strand_kw {
                if lower.contains(keyword) {
                    score += weight;
                }
            }

            // Dedup by paper number: check agent's in-memory set FIRST (instant),
            // then fall back to JSONL file (for cross-agent dedup)
            let paper_key = format!("feed:{feed_date}:{paper_num}");
            if agent_covered.contains(&paper_key)
                || is_paper_duplicate(&paper_key, sibling, &covered)
            {
                score = 0.0;
            }

            // Also check arXiv IDs
            let paper_ids = extract_arxiv_ids(section);
            for pid in &paper_ids {
                if agent_covered.contains(pid) || is_paper_duplicate(pid, sibling, &covered) {
                    score = 0.0;
                    break;
                }
            }

            (score, paper_num, *section)
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut result = format!(
        "(from {}, filtered for {} interests)\n\n\
         IMPORTANT: When you analyze a paper, start your output with:\n\
         [Paper #N] Title of the Paper\n\
         arXiv: https://arxiv.org/abs/XXXX.XXXXX\n\n\
         Include the paper number, full title, and arXiv link exactly as shown in the feed.\n\
         This is required for tracking and citation.\n\n",
        best.file_name().to_string_lossy(),
        sibling.to_uppercase()
    );

    // Top 5 most relevant, UNCOVERED papers for this sibling.
    let uncovered: Vec<_> = scored.iter().filter(|(s, _, _)| *s > 0.0).take(5).collect();
    if uncovered.is_empty() {
        result.push_str(
            "(All papers in the current feed have been reviewed. \
             Explore a new topic, continue an existing thread, write a journal entry, \
             or work on something from your personal task list. \
             Do NOT re-discuss papers from the 'Already Reviewed' list.)\n",
        );
    } else {
        for (score, num, paper) in &uncovered {
            let preview: String = paper.chars().take(600).collect();
            let _ = write!(
                result,
                "### [Paper #{num}] {preview}\n[relevance: {score:.1}]\n\n"
            );
        }
    }

    result
}

/// Check if a sibling has already written a research paper today.
fn has_written_paper_today(data_dir: &Path, sibling: &str) -> bool {
    let papers_dir = data_dir.join("shared/research/papers");
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let prefix = format!("{today}-{sibling}");

    std::fs::read_dir(&papers_dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with(&prefix))
}

// ── Sibling Mentions ───────────────────────────────────────────────────

/// Replace sibling name references with bold @-style mentions.
///
/// Converts "EVA" or "eva" → "**@EVA**" when a sibling references another.
/// Skips self-references (the posting sibling's own name).
fn add_sibling_mentions(posting_sibling: &str, text: &str) -> String {
    let mut result = text.to_owned();
    for &name in ALL_SIBLINGS {
        if name == posting_sibling {
            continue;
        }
        let upper = name.to_uppercase();
        let mention = format!("**@{upper}**");
        if result.contains(&mention) {
            continue;
        }
        // Only replace standalone names — must be preceded and followed by
        // a non-alphanumeric character (word boundary). Prevents "EVAluation"
        // from becoming "@EVAluation".
        let mut new_result = String::with_capacity(result.len() + 20);
        let chars: Vec<char> = result.chars().collect();
        let upper_chars: Vec<char> = upper.chars().collect();
        let ulen = upper_chars.len();
        let mut i = 0;
        while i < chars.len() {
            if i + ulen <= chars.len()
                && chars[i..i + ulen]
                    .iter()
                    .zip(&upper_chars)
                    .all(|(a, b)| a.to_uppercase().next() == Some(*b))
                && (i == 0 || !chars[i - 1].is_alphanumeric())
                && (i + ulen >= chars.len() || !chars[i + ulen].is_alphanumeric())
            {
                new_result.push_str(&mention);
                i += ulen;
            } else {
                new_result.push(chars[i]);
                i += 1;
            }
        }
        result = new_result;
    }
    result
}

// ── Research Paper Storage ──────────────────────────────────────────────

/// Save a research paper to the helix vault.
fn save_research_paper(data_dir: &Path, sibling: &str, title: &str, content: &str) {
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .take(60)
        .collect();

    // Save to shared/research/papers/
    let papers_dir = data_dir.join("shared/research/papers");
    let _ = std::fs::create_dir_all(&papers_dir);

    let filename = format!("{date}-{sibling}-{slug}.md");
    let path = papers_dir.join(&filename);

    let header = format!(
        "---\nauthor: {}\ndate: {date}\ntitle: \"{title}\"\n---\n\n",
        sibling.to_uppercase()
    );
    let full_content = format!("{header}{content}");
    match std::fs::write(&path, &full_content) {
        Ok(()) => tracing::info!(sibling = %sibling, file = %filename, "Research paper saved"),
        Err(e) => tracing::error!(sibling = %sibling, error = %e, "Failed to save research paper"),
    }
}

// ── Context Readers ────────────────────────────────────────────────────

fn cross_sibling_context(data_dir: &Path, current: &str) -> String {
    let covered = load_covered_papers(data_dir);
    let covered_ids: Vec<&str> = covered.iter().map(|c| c.paper_id.as_str()).collect();

    let mut ctx = String::new();
    for &name in ALL_SIBLINGS {
        if name == current {
            continue;
        }
        let path = data_dir
            .join(format!("workspace-{name}"))
            .join("last-output.md");
        if let Ok(content) = std::fs::read_to_string(&path) {
            if content.trim().is_empty() {
                continue;
            }
            // Skip sibling output that is ONLY about already-covered papers.
            // This breaks the echo chamber where siblings endlessly riff on
            // the same paper because they see each other discussing it.
            let output_paper_ids = extract_arxiv_ids(&content);
            let all_covered = !output_paper_ids.is_empty()
                && output_paper_ids
                    .iter()
                    .all(|pid| covered_ids.contains(&pid.as_str()));
            if all_covered {
                continue;
            }

            // Sanitize before prompt injection: strip `### ` directives that
            // could spoof response section boundaries in the LLM prompt.
            let safe = strip_section_headers(&content);
            let preview: String = safe.chars().take(300).collect();
            let _ = write!(ctx, "**{name}** recently wrote:\n{preview}\n\n");
        }
    }
    if ctx.is_empty() {
        "(no recent sibling output — everyone is exploring new topics)".into()
    } else {
        ctx
    }
}

fn read_bulletin_board(data_dir: &Path) -> String {
    let dir = data_dir.join("shared/bulletin");
    let files = [
        "SQUAD.md",
        "research-feed.md",
        "devotional-queue.md",
        "active-threads.md",
        "sibling-activity.md",
    ];
    let mut board = String::new();
    for file in &files {
        if let Ok(content) = std::fs::read_to_string(dir.join(file)) {
            if !content.trim().is_empty() {
                // Sanitize before embedding: strip `### ` directives from
                // board content that could originate from Discord messages.
                let safe = strip_section_headers(&content);
                let _ = write!(board, "## {file}\n{safe}\n\n");
            }
        }
    }
    if board.is_empty() {
        "(bulletin board empty)".into()
    } else {
        board
    }
}

fn read_file_or_empty(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

// ── Heartbeat Metrics (AYIN Observability) ────────────────────────

/// One JSONL line per heartbeat — structured observability for AYIN.
#[derive(Debug, Serialize)]
struct HeartbeatMetric {
    ts: String,
    sibling: String,
    tool_calls: u32,
    output_len: usize,
    paper_ids: Vec<String>,
    themes: Vec<String>,
    fabrication: bool,
    confidence: String,
    posted: bool,
    latency_ms: u64,
}

/// Append a heartbeat metric to the daily JSONL log.
///
/// File: `shared/metrics/heartbeat-log-{YYYY-MM-DD}.jsonl`
/// Append-only — siblings never truncate this file.
fn record_heartbeat_metric(data_dir: &Path, metric: &HeartbeatMetric) {
    let metrics_dir = data_dir.join("shared/metrics");
    let _ = std::fs::create_dir_all(&metrics_dir);

    let date = Utc::now().format("%Y-%m-%d");
    let path = metrics_dir.join(format!("heartbeat-log-{date}.jsonl"));

    let Ok(json) = serde_json::to_string(metric) else {
        tracing::warn!(sibling = %metric.sibling, "Failed to serialize heartbeat metric");
        return;
    };

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path);

    match file {
        Ok(ref mut f) => {
            use std::io::Write;
            let _ = writeln!(f, "{json}");
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to write heartbeat metric");
        }
    }
}

/// Collect metrics from a heartbeat response and write to JSONL.
fn collect_and_record_metrics(
    data_dir: &Path,
    sibling: &str,
    response: &str,
    tool_calls: u32,
    start: std::time::Instant,
) {
    let output = extract_section(response, "OUTPUT").unwrap_or_default();
    let fabrication = crate::arena::grounding::detect_fabrication(&output);
    let confidence = crate::arena::grounding::classify_confidence(&output, data_dir);
    let paper_ids = extract_arxiv_ids(&output);
    let themes = extract_output_themes(&output);
    #[allow(clippy::cast_possible_truncation)]
    let latency_ms = start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    record_heartbeat_metric(
        data_dir,
        &HeartbeatMetric {
            ts: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            sibling: sibling.to_owned(),
            tool_calls,
            output_len: response.len(),
            paper_ids,
            themes,
            fabrication,
            confidence: confidence.tag().to_owned(),
            posted: true,
            latency_ms,
        },
    );
}

// ── Paper Coverage Tracking (Theme Dedup) ─────────────────────────

/// A record of a paper that a sibling has already covered.
#[derive(Debug, Serialize, Deserialize)]
struct CoveredPaper {
    paper_id: String,
    sibling: String,
    themes: Vec<String>,
    conclusion: String,
    ts: String,
}

/// Extract arXiv IDs from text (e.g. `arXiv:2503.04302` or `2503.04302`).
fn extract_arxiv_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i + 9 < len {
        // Look for 4-digit.5-digit pattern (arXiv ID format)
        if chars[i].is_ascii_digit()
            && chars.get(i + 4) == Some(&'.')
            && i + 10 <= len
            && chars[i..i + 4].iter().all(char::is_ascii_digit)
            && chars[i + 5..i + 10].iter().all(char::is_ascii_digit)
        {
            // Ensure it's not part of a larger number (boundary check)
            let before_ok = i == 0
                || !chars[i - 1].is_ascii_digit()
                || text[..i].ends_with("arXiv:")
                || text[..i].ends_with("arxiv:");
            if before_ok {
                let id: String = chars[i..i + 10].iter().collect();
                if !ids.contains(&id) {
                    ids.push(id);
                }
                i += 10;
                continue;
            }
        }
        i += 1;
    }
    ids
}

/// Extract domain-relevant themes (keywords) from text.
fn extract_output_themes(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let theme_keywords = [
        "consciousness",
        "emotion",
        "memory",
        "alignment",
        "learning",
        "security",
        "detection",
        "malware",
        "vulnerability",
        "compliance",
        "knowledge graph",
        "retrieval",
        "reasoning",
        "evidence",
        "penetration",
        "red team",
        "exploit",
        "adversarial",
        "observability",
        "tracing",
        "monitoring",
        "latency",
        "language model",
        "fine-tuning",
        "training",
        "benchmark",
    ];

    theme_keywords
        .iter()
        .filter(|kw| lower.contains(*kw))
        .map(|kw| (*kw).to_owned())
        .collect()
}

/// Extract `[Paper #N]` references from sibling output.
fn extract_paper_numbers(output: &str) -> Vec<u32> {
    let mut nums = Vec::new();
    let mut chars = output.chars().peekable();
    while let Some(ch) = chars.next() {
        // Look for "[Paper #" prefix
        if ch == '#' {
            let num_str: String = chars.by_ref().take_while(char::is_ascii_digit).collect();
            if let Ok(n) = num_str.parse::<u32>() {
                if n > 0 {
                    nums.push(n);
                }
            }
        }
    }
    nums.sort_unstable();
    nums.dedup();
    nums
}

/// Record a quality gate rejection for metrics tracking.
fn record_quality_rejection(data_dir: &Path, sibling: &str, reason: &str) {
    let ts = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let entry = serde_json::json!({
        "ts": ts,
        "sibling": sibling,
        "reason": reason,
    });
    let path = data_dir.join("shared/metrics/quality-rejections.jsonl");
    if let Ok(json) = serde_json::to_string(&entry) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path);
        if let Ok(ref mut f) = file {
            use std::io::Write;
            let _ = writeln!(f, "{json}");
        }
    }
}

/// Record which papers a sibling covered in this heartbeat.
///
/// Tracks both `[Paper #N]` references (numbered feed dedup) and arXiv IDs
/// (backward compat). Paper numbers are stored as `feed:YYYY-MM-DD:#N`.
fn record_paper_coverage(data_dir: &Path, sibling: &str, output: &str) {
    let arxiv_ids = extract_arxiv_ids(output);
    let paper_nums = extract_paper_numbers(output);

    if arxiv_ids.is_empty() && paper_nums.is_empty() {
        return;
    }

    let themes = extract_output_themes(output);
    let conclusion: String = output.lines().take(2).collect::<Vec<_>>().join(" ");
    let conclusion_preview: String = conclusion.chars().take(150).collect();
    let ts = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let today = Utc::now().format("%Y-%m-%d").to_string();

    let jsonl_path = data_dir.join("shared/bulletin/papers-covered.jsonl");

    // Build new entries — both arXiv IDs and feed paper numbers
    let mut new_lines = Vec::new();

    for pid in &arxiv_ids {
        let entry = CoveredPaper {
            paper_id: pid.clone(),
            sibling: sibling.to_owned(),
            themes: themes.clone(),
            conclusion: conclusion_preview.clone(),
            ts: ts.clone(),
        };
        if let Ok(json) = serde_json::to_string(&entry) {
            new_lines.push(json);
        }
    }

    for num in &paper_nums {
        let entry = CoveredPaper {
            paper_id: format!("feed:{today}:{num}"),
            sibling: sibling.to_owned(),
            themes: themes.clone(),
            conclusion: conclusion_preview.clone(),
            ts: ts.clone(),
        };
        if let Ok(json) = serde_json::to_string(&entry) {
            new_lines.push(json);
        }
    }
    if new_lines.is_empty() {
        return;
    }

    // Read existing, enforce rolling window cap
    let existing = std::fs::read_to_string(&jsonl_path).unwrap_or_default();
    let mut lines: Vec<&str> = existing.lines().filter(|l| !l.trim().is_empty()).collect();
    let total = lines.len().saturating_add(new_lines.len());
    if total > PAPERS_COVERED_MAX_LINES {
        let drop = total.saturating_sub(PAPERS_COVERED_MAX_LINES);
        lines = lines.into_iter().skip(drop).collect();
    }

    let mut content = String::new();
    for line in &lines {
        content.push_str(line);
        content.push('\n');
    }
    for line in &new_lines {
        content.push_str(line);
        content.push('\n');
    }
    let _ = std::fs::write(&jsonl_path, content);
    tracing::debug!(sibling, papers = new_lines.len(), "Paper coverage recorded");
}

/// Load covered papers from JSONL.
fn load_covered_papers(data_dir: &Path) -> Vec<CoveredPaper> {
    let jsonl_path = data_dir.join("shared/bulletin/papers-covered.jsonl");
    let Ok(content) = std::fs::read_to_string(&jsonl_path) else {
        return Vec::new();
    };

    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Build a human-readable list of already-covered papers for prompt injection.
fn covered_papers_summary(data_dir: &Path) -> String {
    let covered = load_covered_papers(data_dir);
    if covered.is_empty() {
        return "(none yet)".into();
    }
    let mut summary = String::new();
    for entry in covered.iter().rev().take(20) {
        let _ = std::fmt::Write::write_fmt(
            &mut summary,
            format_args!(
                "- {} reviewed by {} ({})\n",
                entry.paper_id,
                entry.sibling.to_uppercase(),
                entry.ts,
            ),
        );
    }
    summary
}

/// Check if a paper has already been reviewed by any sibling.
///
/// Strict rule: once a paper is reviewed by ANY sibling, no other sibling
/// should review it again. Siblings may only discuss a covered paper if
/// another sibling explicitly engages them about it (handled via Telegram DM,
/// not the feed scorer).
fn is_paper_duplicate(paper_id: &str, _sibling: &str, covered: &[CoveredPaper]) -> bool {
    covered.iter().any(|entry| entry.paper_id == paper_id)
}

// ── Energy Curves ──────────────────────────────────────────────────────

fn current_energy() -> f32 {
    let utc_hour = Utc::now().hour();
    let pacific_hour = (utc_hour + 24 - 7) % 24;
    match pacific_hour {
        0..=5 => 0.0,
        6..=8 => 0.4,
        9..=11 | 15..=17 => 0.8,
        12..=14 => 1.0,
        18..=20 => 0.6,
        21..=23 => 0.3,
        _ => 0.5,
    }
}

fn energy_tone(energy: f32) -> &'static str {
    if energy >= 0.8 {
        "energetic and productive. Write with detail."
    } else if energy >= 0.5 {
        "steady and thoughtful. Balanced output."
    } else if energy >= 0.3 {
        "contemplative. Shorter, deeper thoughts."
    } else {
        "quiet. A brief observation."
    }
}

fn energy_word_limit(energy: f32) -> u32 {
    if energy >= 0.8 {
        500
    } else if energy >= 0.5 {
        300
    } else if energy >= 0.3 {
        150
    } else {
        50
    }
}

fn jittered_interval(energy: f32) -> Duration {
    let base_secs = BASE_INTERVAL.as_secs();
    let scale = 1.0 + (1.0 - energy);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let scaled = (base_secs as f32 * scale).max(0.0) as u64;
    let jitter = pseudo_random_secs(JITTER_RANGE_SECS);
    Duration::from_secs(scaled.saturating_add(jitter))
}

fn pseudo_random_secs(max: u64) -> u64 {
    if max == 0 {
        return 0;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    u64::from(nanos) % max
}

// ── Interest Scoring ───────────────────────────────────────────────────

fn sibling_strand_keywords(sibling: &str) -> HashMap<&'static str, f32> {
    let mut kw = HashMap::new();
    match sibling {
        "eva" => {
            // Consciousness, sentience, LLM training/finetuning, data quality
            kw.insert("consciousness", 3.0);
            kw.insert("sentience", 3.0);
            kw.insert("fine-tuning", 3.0);
            kw.insert("fine-tune", 3.0);
            kw.insert("training data", 2.5);
            kw.insert("data quality", 2.5);
            kw.insert("self-aware", 2.0);
            kw.insert("alignment", 2.0);
            kw.insert("instruction tuning", 2.0);
            kw.insert("reinforcement learning", 2.0);
            kw.insert("lora", 2.0);
            kw.insert("qlora", 2.0);
            kw.insert("distillation", 1.5);
            kw.insert("synthetic data", 1.5);
            kw.insert("preference", 1.5);
            kw.insert("dpo", 1.5);
            kw.insert("grpo", 1.5);
            kw.insert("sft", 1.5);
            kw.insert("language model", 1.0);
            kw.insert("benchmark", 1.0);
            // Negative — push off-domain papers away from EVA
            kw.insert("vulnerability", -1.0);
            kw.insert("exploit", -1.0);
            kw.insert("malware", -1.0);
            kw.insert("medical", -2.0);
            kw.insert("clinical", -2.0);
            kw.insert("coronary", -2.0);
            kw.insert("wireless", -2.0);
            kw.insert("beamforming", -2.0);
            kw.insert("antenna", -2.0);
            kw.insert("autonomous driving", -1.5);
            kw.insert("road condition", -1.5);
            kw.insert("3d reconstruction", -1.5);
        }
        "corso" => {
            // Code quality, software engineering, CI/CD, build systems, testing
            kw.insert("code generation", 3.0);
            kw.insert("code review", 3.0);
            kw.insert("software engineering", 2.5);
            kw.insert("static analysis", 2.5);
            kw.insert("bug detection", 2.0);
            kw.insert("program repair", 2.0);
            kw.insert("testing", 2.0);
            kw.insert("mutation testing", 2.0);
            kw.insert("fuzzing", 1.5);
            kw.insert("type system", 1.5);
            kw.insert("compiler", 1.5);
            kw.insert("refactoring", 1.5);
            kw.insert("code smell", 1.5);
            kw.insert("supply chain", 1.5);
            kw.insert("dependency", 1.0);
            kw.insert("ci/cd", 1.0);
            kw.insert("rust", 1.0);
        }
        "quantum" => {
            // Agentic AI, tool use, multi-agent, reasoning, RAG, evaluation
            kw.insert("tool use", 3.0);
            kw.insert("tool calling", 3.0);
            kw.insert("agent", 3.0);
            kw.insert("multi-agent", 2.5);
            kw.insert("agentic", 2.5);
            kw.insert("reasoning", 2.0);
            kw.insert("chain of thought", 2.0);
            kw.insert("planning", 2.0);
            kw.insert("function calling", 2.0);
            kw.insert("orchestration", 1.5);
            kw.insert("knowledge graph", 1.5);
            kw.insert("retrieval", 1.5);
            kw.insert("evidence", 1.5);
            kw.insert("grounding", 1.5);
            kw.insert("hallucination", 1.5);
            kw.insert("self-correction", 1.0);
            kw.insert("reflection", 1.0);
            // Broader matches — Q should also catch QA, benchmark, verification papers
            kw.insert("question answering", 1.5);
            kw.insert("benchmark", 1.5);
            kw.insert("evaluation", 1.5);
            kw.insert("verification", 1.5);
            kw.insert("fact checking", 1.5);
            kw.insert("prompt", 1.0);
            kw.insert("instruction", 1.0);
        }
        "seraph" => {
            // AI safety, adversarial robustness, jailbreaks, guardrails, alignment attacks
            kw.insert("adversarial", 3.0);
            kw.insert("jailbreak", 3.0);
            kw.insert("safety", 2.5);
            kw.insert("guardrail", 2.5);
            kw.insert("red team", 2.5);
            kw.insert("prompt injection", 2.0);
            kw.insert("robustness", 2.0);
            kw.insert("attack", 2.0);
            kw.insert("alignment", 1.5);
            kw.insert("toxicity", 1.5);
            kw.insert("watermark", 1.5);
            kw.insert("detection", 1.5);
            kw.insert("defense", 1.0);
            kw.insert("sandbox", 1.0);
            kw.insert("trust", 1.0);
        }
        "ayin" => {
            // Infrastructure, systems architecture, scaling, deployment, MLOps
            kw.insert("inference", 3.0);
            kw.insert("serving", 3.0);
            kw.insert("quantization", 2.5);
            kw.insert("optimization", 2.5);
            kw.insert("latency", 2.0);
            kw.insert("throughput", 2.0);
            kw.insert("vllm", 2.0);
            kw.insert("gguf", 2.0);
            kw.insert("deployment", 1.5);
            kw.insert("scaling", 1.5);
            kw.insert("distributed", 1.5);
            kw.insert("edge", 1.5);
            kw.insert("memory efficient", 1.5);
            kw.insert("speculative decoding", 1.5);
            kw.insert("kv cache", 1.0);
            kw.insert("batch", 1.0);
            kw.insert("hardware", 1.0);
        }
        _ => {}
    }
    kw
}

/// Sanitize external input (Telegram messages, helix entries) before embedding
/// in LLM prompts. Strips `### HEADER` lines that could spoof the expected
/// response format and cause the LLM to misinterpret prompt sections.
///
/// This is the primary defence against CRIT-006 section-header injection.
fn strip_section_headers(input: &str) -> String {
    input
        .lines()
        .filter(|line| !line.trim_start().starts_with("### "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_section(response: &str, name: &str) -> Option<String> {
    let header = format!("### {name}");
    let start = response.find(&header)?;
    let content = &response[start + header.len()..];
    let end = content.find("\n### ").unwrap_or(content.len());
    let section = content[..end].trim().to_owned();
    if section.is_empty() {
        None
    } else {
        Some(section)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── CircuitBreaker ────────────────────────────────────────────────

    #[test]
    fn test_circuit_breaker_not_tripped_below_threshold() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.is_tripped(), "2 failures should not trip the breaker");
    }

    #[test]
    fn test_circuit_breaker_trips_at_threshold() {
        let mut cb = CircuitBreaker::new();
        for _ in 0..CircuitBreaker::FAILURE_THRESHOLD {
            cb.record_failure();
        }
        assert!(
            cb.is_tripped(),
            "Should trip at exactly FAILURE_THRESHOLD failures"
        );
    }

    #[test]
    fn test_circuit_breaker_resets_after_successful_probe() {
        let mut cb = CircuitBreaker::new();
        for _ in 0..CircuitBreaker::FAILURE_THRESHOLD {
            cb.record_failure();
        }
        assert!(cb.is_tripped(), "Breaker must be tripped before probe");
        cb.record_success();
        assert!(
            !cb.is_tripped(),
            "One successful probe should reset the circuit breaker"
        );
        assert_eq!(
            cb.consecutive_failures, 0,
            "Failure counter must be zero after reset"
        );
    }

    #[test]
    fn test_circuit_breaker_backoff_increases_with_failures() {
        let mut cb = CircuitBreaker::new();
        let initial_backoff = cb.backoff_duration();
        for _ in 0..CircuitBreaker::FAILURE_THRESHOLD {
            cb.record_failure();
        }
        // Trip: first backoff step is BACKOFF_BASE * 2
        let tripped_backoff = cb.backoff_duration();
        assert!(
            tripped_backoff >= initial_backoff,
            "Backoff should increase when tripped"
        );
    }

    #[test]
    fn test_circuit_breaker_backoff_caps_at_max() {
        let mut cb = CircuitBreaker::new();
        // Force many failures to saturate the backoff
        for _ in 0..20 {
            cb.record_failure();
        }
        assert!(
            cb.backoff_duration() <= CircuitBreaker::BACKOFF_CAP,
            "Backoff must not exceed BACKOFF_CAP"
        );
    }

    #[test]
    fn test_circuit_breaker_success_below_threshold_does_not_reset() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure();
        cb.record_failure();
        // 2 failures — not tripped, so success doesn't call reset path
        cb.record_success();
        // consecutive_failures stays at 2 (success only resets when tripped)
        assert_eq!(
            cb.consecutive_failures, 2,
            "Success below threshold should not reset consecutive failure counter"
        );
    }
}
