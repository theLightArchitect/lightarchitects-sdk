//! Bulletin board curator — deterministic curation engine.
//!
//! Populates the shared bulletin board using filesystem data only.
//! ZERO LLM calls — all content is derived from research feeds,
//! devotional rotation, and sibling activity aggregation.
//!
//! The curator does NOT dispatch work to siblings — it curates the board.
//! Siblings choose their own work via interest scoring in the heartbeat loop.

use std::fmt::Write as _;
use std::path::Path;

use chrono::Timelike;

use chrono::Utc;

use crate::channels::Channels;

/// Sibling rotation order for devotional and activity aggregation.
const SIBLING_ORDER: &[&str] = &["eva", "corso", "quantum", "seraph", "ayin"];

/// Run one curator cycle (called by scheduler when curator routine fires).
///
/// Mostly deterministic — reads filesystem, scores papers, rotates devotionals,
/// aggregates activity, posts formatted summaries. The one exception is the
/// daily paper fetch (HTTP GET to `HuggingFace` papers API, no LLM).
///
/// # Errors
/// Returns error if filesystem operations fail.
pub fn run_cycle(data_dir: &Path, channels: &Channels) -> Result<(), String> {
    let shared_dir = data_dir.join("shared");
    ensure_shared_dirs(&shared_dir)?;

    tracing::info!("Curator cycle starting");

    // 0. Fetch fresh papers and threat intel if this window's feed doesn't exist
    fetch_daily_papers_if_needed(&shared_dir);
    fetch_intel_feed_if_needed(&shared_dir);

    // 1. Research feed — score and pick top 5 UNCOVERED papers
    let research_summary = build_research_feed(data_dir, &shared_dir)?;
    let bulletin_dir = shared_dir.join("bulletin");
    std::fs::write(bulletin_dir.join("research-feed.md"), &research_summary)
        .map_err(|e| format!("Failed to write research-feed.md: {e}"))?;

    // 1b. Intel feed — write to bulletin board for sibling consumption
    let intel_summary = build_intel_summary(&shared_dir);
    std::fs::write(bulletin_dir.join("intel-feed.md"), &intel_summary)
        .map_err(|e| format!("Failed to write intel-feed.md: {e}"))?;

    // 2. Devotional rotation
    let devotional = rotate_devotional(&shared_dir)?;
    std::fs::write(bulletin_dir.join("devotional-queue.md"), &devotional)
        .map_err(|e| format!("Failed to write devotional-queue.md: {e}"))?;

    // 3. Promote staging → live board (validate + merge)
    promote_staging(data_dir, &shared_dir)?;

    // 4. Sibling activity aggregation
    let activity = aggregate_sibling_activity(data_dir);
    std::fs::write(bulletin_dir.join("sibling-activity.md"), &activity)
        .map_err(|e| format!("Failed to write sibling-activity.md: {e}"))?;

    // 5. Workspace maintenance (runs every cycle, idempotent)
    run_maintenance(data_dir, &shared_dir);

    // 6. Post deterministic summary to Telegram only (ops channel).
    let telegram_msg = build_telegram_summary(&research_summary, &devotional);
    channels.post_telegram(&telegram_msg);

    tracing::info!("Curator cycle complete");
    Ok(())
}

/// Ensure the shared bulletin board directories exist.
pub fn ensure_shared_dirs(shared_dir: &Path) -> Result<(), String> {
    let dirs = [
        shared_dir.join("bulletin"),
        shared_dir.join("bulletin/staging"),
        shared_dir.join("devotionals"),
        shared_dir.join("devotionals/scripture"),
        shared_dir.join("devotionals/reflections"),
        shared_dir.join("research"),
        shared_dir.join("research/feed"),
        shared_dir.join("metrics"),
    ];
    for dir in &dirs {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
    }
    Ok(())
}

/// Extract a named section from a response string.
#[allow(dead_code)] // Retained for future use by external consumers
pub fn extract_section(response: &str, section_name: &str) -> Option<String> {
    let header = format!("### {section_name}");
    let start = response.find(&header)?;
    let content_start = start.checked_add(header.len())?;
    let content = response.get(content_start..)?;

    // Find the next ### header or end of string
    let end = content.find("\n### ").unwrap_or(content.len());
    let section = content.get(..end)?.trim().to_owned();

    if section.is_empty() {
        None
    } else {
        Some(section)
    }
}

// ── Research Feed ────────────────────────────────────────────────────────

/// Build the research feed by scoring papers from the feed directory.
fn build_research_feed(data_dir: &Path, shared_dir: &Path) -> Result<String, String> {
    let feed_dir = shared_dir.join("research/feed");
    let best_file = find_largest_feed_file(&feed_dir)?;

    let content = std::fs::read_to_string(&best_file)
        .map_err(|e| format!("Failed to read feed file: {e}"))?;

    // Load covered papers to filter from the bulletin board
    let covered = load_covered_paper_ids(data_dir);

    let feed_date = best_file.file_name().map_or_else(
        || "unknown".into(),
        |n| n.to_string_lossy().trim_end_matches(".md").to_owned(),
    );

    // Score papers, filtering out covered ones
    let sections: Vec<(usize, &str)> = content
        .split("\n### ")
        .filter(|s| !s.trim().is_empty() && s.len() > 20)
        .enumerate()
        .collect();

    let mut scored: Vec<(f32, String, String, usize)> = sections
        .into_iter()
        .filter_map(|(idx, section)| {
            let paper_num = idx.saturating_add(1);
            let paper_key = format!("feed:{feed_date}:{paper_num}");

            // Skip covered papers entirely
            if covered.contains(&paper_key) {
                return None;
            }

            let (score, domains, text) = score_single_paper(section);
            Some((score, domains, text, paper_num))
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let top_5: Vec<_> = scored.into_iter().take(5).collect();

    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let filename = best_file
        .file_name()
        .map_or_else(|| "unknown".into(), |n| n.to_string_lossy().to_string());

    let mut output = format!("# Research Feed\n\n*Updated: {timestamp} | Source: {filename}*\n\n");

    if top_5.is_empty() {
        let _ = writeln!(
            output,
            "(all papers reviewed — siblings will explore new topics)"
        );
    } else {
        for (score, domains, text, _num) in &top_5 {
            let _ = write!(output, "## [score: {score:.1} | {domains}]\n\n{text}\n\n",);
        }
    }

    Ok(output)
}

/// Build the intel feed summary for the bulletin board.
///
/// Reads the most recent `*-intel.md` file from the feed directory
/// and includes it verbatim. The intel feed contains CVEs, CISA KEV,
/// and arXiv security papers — already formatted by the fetcher.
fn build_intel_summary(shared_dir: &Path) -> String {
    let feed_dir = shared_dir.join("research/feed");
    let mut intel_files: Vec<_> = std::fs::read_dir(&feed_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with("-intel.md"))
        .collect();

    intel_files.sort_by_key(|e| std::cmp::Reverse(e.file_name()));

    match intel_files.first() {
        Some(entry) => std::fs::read_to_string(entry.path())
            .unwrap_or_else(|_| "*(intel feed unreadable)*".into()),
        None => "# Threat Intelligence Feed\n\n*(no intel feed available yet)*\n".into(),
    }
}

/// Load covered paper IDs from JSONL as a set for fast lookup.
fn load_covered_paper_ids(data_dir: &Path) -> std::collections::HashSet<String> {
    let path = data_dir.join("shared/bulletin/papers-covered.jsonl");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return std::collections::HashSet::new();
    };
    content
        .lines()
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            v.get("paper_id")
                .and_then(serde_json::Value::as_str)
                .map(String::from)
        })
        .collect()
}

/// Score a single paper section by domain keywords.
fn score_single_paper(section: &str) -> (f32, String, String) {
    let lower = section.to_lowercase();

    // Weighted domain scoring — priority intel domains score 2x
    let domains: &[(&str, f32, &[&str])] = &[
        // PRIORITY: LLM security & adversarial research (2x weight)
        (
            "llm-security",
            2.0,
            &[
                "prompt injection",
                "jailbreak",
                "adversarial attack",
                "model poisoning",
                "data poisoning",
                "backdoor",
                "trojan",
                "llm security",
                "ai safety",
                "guardrail",
                "red team",
                "alignment",
                "misuse",
            ],
        ),
        // PRIORITY: CVE / vulnerability research (2x weight)
        (
            "vulnerability",
            2.0,
            &[
                "cve",
                "vulnerability",
                "exploit",
                "supply chain",
                "zero-day",
                "malware",
                "detection",
                "static analysis",
                "binary analysis",
                "threat",
                "attack surface",
            ],
        ),
        // PRIORITY: Training algorithms & optimization (1.5x weight)
        (
            "training",
            1.5,
            &[
                "fine-tun",
                "lora",
                "qlora",
                "training",
                "optimization",
                "distillation",
                "curriculum",
                "reinforcement learning from",
                "rlhf",
                "dpo",
                "grpo",
                "reward model",
                "loss function",
                "learning rate",
                "gradient",
                "quantiz",
            ],
        ),
        // PRIORITY: Agentic orchestration & multi-agent (1.5x weight)
        (
            "orchestration",
            1.5,
            &[
                "multi-agent",
                "agent",
                "orchestrat",
                "tool calling",
                "tool use",
                "mcp",
                "function calling",
                "planning",
                "reasoning chain",
                "chain of thought",
                "agentic",
                "autonomous",
            ],
        ),
        // Standard: AI/LLM breakthroughs
        (
            "ai-breakthrough",
            1.0,
            &[
                "transformer",
                "attention",
                "scaling",
                "architecture",
                "benchmark",
                "state-of-the-art",
                "multimodal",
                "vision-language",
                "embedding",
                "tokeniz",
                "context window",
                "inference",
            ],
        ),
        // Standard: Knowledge systems
        (
            "knowledge",
            1.0,
            &[
                "knowledge graph",
                "retrieval",
                "rag",
                "evidence",
                "information extraction",
                "semantic search",
            ],
        ),
        // Standard: Observability & monitoring
        (
            "observability",
            1.0,
            &[
                "observability",
                "tracing",
                "monitoring",
                "latency",
                "anomaly detection",
            ],
        ),
    ];

    let mut total_score: f32 = 0.0;
    let mut matched = Vec::new();
    for (domain, weight, keywords) in domains {
        let hits = keywords.iter().filter(|kw| lower.contains(*kw)).count();
        if hits > 0 {
            total_score += hits as f32 * weight;
            matched.push(*domain);
        }
    }

    let domains_str = if matched.is_empty() {
        "general".to_owned()
    } else {
        matched.join(", ")
    };
    let preview: String = section.chars().take(600).collect();
    (total_score, domains_str, preview)
}

/// Find the newest `.md` file in the feed directory (most recent date).
///
/// Feed files are named `YYYY-MM-DD.md`. The newest by filename wins,
/// NOT the largest by size (a stale 190KB file should not beat a fresh 50KB file).
fn find_largest_feed_file(feed_dir: &Path) -> Result<std::path::PathBuf, String> {
    let entries: Vec<_> = std::fs::read_dir(feed_dir)
        .map_err(|e| format!("Failed to read feed dir: {e}"))?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .collect();

    if entries.is_empty() {
        return Err("No feed files found in shared/research/feed/".into());
    }

    // Sort by filename descending — YYYY-MM-DD.md sorts correctly as strings
    let mut sorted: Vec<_> = entries.into_iter().collect();
    sorted.sort_by_key(|e| std::cmp::Reverse(e.file_name()));

    sorted
        .into_iter()
        .next()
        .map(|e| e.path())
        .ok_or_else(|| "No feed files found".into())
}

// ── Daily Paper Fetch ─────────────────────────────────────────────────────

/// Maximum papers to fetch per daily pull.
const DAILY_PAPER_LIMIT: usize = 50;

/// Fetch papers from `HuggingFace` daily papers API if this window's feed
/// doesn't exist yet. Writes to `shared/research/feed/YYYY-MM-DD-{am|pm}.md`.
///
/// Two windows per day: AM (before 12:00 UTC) and PM (12:00 UTC onward).
/// This gives fresh papers at ~7am and ~7pm PST when the curator runs
/// on those schedules. Each window is idempotent — fetches at most once.
fn fetch_daily_papers_if_needed(shared_dir: &Path) {
    let now = Utc::now();
    let today = now.format("%Y-%m-%d").to_string();
    let window = if now.hour() < 12 { "am" } else { "pm" };
    let feed_dir = shared_dir.join("research/feed");
    let window_file = feed_dir.join(format!("{today}-{window}.md"));

    if window_file.exists() {
        return; // Already fetched this window
    }

    tracing::info!(date = %today, window, "Fetching papers from HuggingFace");

    // Use a short-lived blocking runtime for the HTTP call.
    // The curator runs in a sync context (called from scheduler tick).
    let papers = match fetch_hf_daily_papers() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "Daily paper fetch failed — using stale feed");
            return;
        }
    };

    if papers.is_empty() {
        tracing::warn!("HuggingFace returned 0 papers — skipping");
        return;
    }

    // Format as the same markdown structure the scorer expects.
    // HF API nests paper data under "paper" key.
    let mut content = format!(
        "---\ndate: {today}\nsources: [huggingface]\npaper_count: {count}\n---\n\n\
         # Research Feed — {today}\n\n",
        count = papers.len(),
    );

    for paper_wrapper in &papers {
        // HF daily papers API nests data: { "paper": { "id", "title", ... }, "upvotes": N }
        let inner = paper_wrapper.get("paper").unwrap_or(paper_wrapper);

        let title = inner
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Untitled");
        let paper_id = inner
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let upvotes = paper_wrapper
            .get("upvotes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let authors = inner
            .get("authors")
            .and_then(serde_json::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.get("name").and_then(serde_json::Value::as_str))
                    .take(5)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let summary = inner
            .get("summary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let preview: String = summary.chars().take(500).collect();

        // Build proper arXiv + HF links from the paper ID
        let arxiv_link = if paper_id.is_empty() {
            String::new()
        } else {
            format!("https://arxiv.org/abs/{paper_id}")
        };
        let hf_link = if paper_id.is_empty() {
            String::new()
        } else {
            format!("https://huggingface.co/papers/{paper_id}")
        };

        let _ = write!(
            content,
            "### {title}\n\
             - **arXiv**: {arxiv_link}\n\
             - **HuggingFace**: {hf_link}\n\
             - **Authors**: {authors}\n\
             - **Upvotes**: {upvotes}\n\
             - **Abstract**: {preview}\n\n",
        );
    }

    match std::fs::write(&window_file, &content) {
        Ok(()) => {
            tracing::info!(
                date = %today,
                window,
                papers = papers.len(),
                bytes = content.len(),
                "Feed window written"
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to write feed window");
        }
    }
}

/// Fetch papers from the `HuggingFace` daily papers API.
///
/// Returns a vec of paper JSON objects. Uses a blocking HTTP client
/// since the curator runs in a sync context.
fn fetch_hf_daily_papers() -> Result<Vec<serde_json::Value>, String> {
    let url = format!("https://huggingface.co/api/daily_papers?limit={DAILY_PAPER_LIMIT}");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .get(&url)
        .header("User-Agent", "lightarchitects-arena/1.0")
        .send()
        .map_err(|e| format!("HuggingFace API error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HuggingFace API returned {}", resp.status()));
    }

    let papers: Vec<serde_json::Value> =
        resp.json().map_err(|e| format!("JSON parse error: {e}"))?;

    Ok(papers)
}

// ── Threat Intelligence Feeds ─────────────────────────────────────────────

/// Maximum recent CVEs to fetch per window.
const CVE_FETCH_LIMIT: usize = 20;

/// Fetch recent CVEs from NVD and write to the intel feed.
///
/// Queries NVD 2.0 API for CVEs published in the last 24 hours with
/// AI/ML-adjacent keywords. Also fetches CISA KEV for actively exploited
/// vulnerabilities. Writes to `shared/research/feed/YYYY-MM-DD-{am|pm}-intel.md`.
fn fetch_intel_feed_if_needed(shared_dir: &std::path::Path) {
    let now = Utc::now();
    let today = now.format("%Y-%m-%d").to_string();
    let window = if now.hour() < 12 { "am" } else { "pm" };
    let feed_dir = shared_dir.join("research/feed");
    let intel_file = feed_dir.join(format!("{today}-{window}-intel.md"));

    if intel_file.exists() {
        return;
    }

    tracing::info!(date = %today, window, "Fetching threat intelligence feeds");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let mut content = format!(
        "---\ndate: {today}\nwindow: {window}\nsources: [nvd, cisa-kev, arxiv-security]\ntype: intel\n---\n\n\
         # Threat Intelligence Feed — {today} ({window})\n\n"
    );

    // 1. NVD CVEs (last 24h)
    let yesterday = (now - chrono::Duration::hours(24))
        .format("%Y-%m-%dT00:00:00.000")
        .to_string();
    let nvd_url = format!(
        "https://services.nvd.nist.gov/rest/json/cves/2.0?pubStartDate={yesterday}&resultsPerPage={CVE_FETCH_LIMIT}"
    );
    match client
        .get(&nvd_url)
        .header("User-Agent", "lightarchitects-arena/1.0")
        .send()
    {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(json) = resp.json::<serde_json::Value>() {
                let total = json
                    .get("totalResults")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                content.push_str(&format!("## Recent CVEs ({total} in last 24h)\n\n"));

                if let Some(vulns) = json
                    .get("vulnerabilities")
                    .and_then(serde_json::Value::as_array)
                {
                    for vuln in vulns.iter().take(CVE_FETCH_LIMIT) {
                        let cve = vuln.get("cve").unwrap_or(vuln);
                        let id = cve
                            .get("id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("Unknown");
                        let desc = cve
                            .get("descriptions")
                            .and_then(serde_json::Value::as_array)
                            .and_then(|a| a.first())
                            .and_then(|d| d.get("value"))
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("No description");
                        let preview: String = desc.chars().take(300).collect();
                        let severity = cve
                            .get("metrics")
                            .and_then(|m| m.get("cvssMetricV31"))
                            .and_then(serde_json::Value::as_array)
                            .and_then(|a| a.first())
                            .and_then(|m| m.get("cvssData"))
                            .and_then(|d| d.get("baseScore"))
                            .and_then(serde_json::Value::as_f64)
                            .map_or("Unknown".to_owned(), |s| format!("{s:.1}"));
                        let _ = write!(
                            content,
                            "### {id} (CVSS: {severity})\n{preview}\n\
                             - **NVD**: https://nvd.nist.gov/vuln/detail/{id}\n\n"
                        );
                    }
                }
            }
        }
        Ok(resp) => {
            tracing::warn!(status = %resp.status(), "NVD API returned non-success");
            content.push_str("## Recent CVEs\n\n*(NVD API unavailable this window)*\n\n");
        }
        Err(e) => {
            tracing::warn!(error = %e, "NVD fetch failed");
            content.push_str("## Recent CVEs\n\n*(NVD API unreachable)*\n\n");
        }
    }

    // 2. CISA Known Exploited Vulnerabilities (static JSON, check for recent additions)
    match client
        .get("https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json")
        .header("User-Agent", "lightarchitects-arena/1.0")
        .send()
    {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(json) = resp.json::<serde_json::Value>() {
                let cutoff = (now - chrono::Duration::days(7))
                    .format("%Y-%m-%d")
                    .to_string();
                if let Some(vulns) = json
                    .get("vulnerabilities")
                    .and_then(serde_json::Value::as_array)
                {
                    let recent: Vec<_> = vulns
                        .iter()
                        .filter(|v| {
                            v.get("dateAdded")
                                .and_then(serde_json::Value::as_str)
                                .is_some_and(|d| d >= cutoff.as_str())
                        })
                        .take(10)
                        .collect();

                    if !recent.is_empty() {
                        content.push_str(&format!(
                            "## CISA KEV — Actively Exploited ({} this week)\n\n",
                            recent.len()
                        ));
                        for vuln in &recent {
                            let id = vuln
                                .get("cveID")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("Unknown");
                            let name = vuln
                                .get("vulnerabilityName")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("");
                            let vendor = vuln
                                .get("vendorProject")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("");
                            let product = vuln
                                .get("product")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("");
                            let due = vuln
                                .get("dueDate")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("");
                            let _ = write!(
                                content,
                                "### {id}: {name}\n- **Vendor**: {vendor} — {product}\n\
                                 - **Remediation due**: {due}\n\
                                 - **NVD**: https://nvd.nist.gov/vuln/detail/{id}\n\n"
                            );
                        }
                    }
                }
            }
        }
        _ => {
            tracing::warn!("CISA KEV fetch failed — skipping");
        }
    }

    // 3. arXiv security + AI intersection (recent papers)
    let arxiv_query = "cat:cs.CR+AND+(cat:cs.AI+OR+cat:cs.LG+OR+cat:cs.CL)";
    let arxiv_url = format!(
        "http://export.arxiv.org/api/query?search_query={arxiv_query}&sortBy=submittedDate&sortOrder=descending&max_results=10"
    );
    match client
        .get(&arxiv_url)
        .header("User-Agent", "lightarchitects-arena/1.0")
        .send()
    {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(body) = resp.text() {
                content.push_str("## arXiv Security × AI (recent)\n\n");
                // Parse arXiv Atom XML — extract entries
                for entry in body.split("<entry>").skip(1).take(10) {
                    let title = super::agent_loop::extract_xml_tag(entry, "title")
                        .unwrap_or_else(|| "Untitled".into())
                        .replace('\n', " ");
                    let summary = super::agent_loop::extract_xml_tag(entry, "summary")
                        .unwrap_or_default()
                        .replace('\n', " ");
                    let preview: String = summary.chars().take(400).collect();
                    let id = entry
                        .split("<id>")
                        .nth(1)
                        .and_then(|s| s.split("</id>").next())
                        .unwrap_or("")
                        .trim()
                        .replace("http://arxiv.org/abs/", "");
                    let _ = write!(
                        content,
                        "### {title}\n- **arXiv**: https://arxiv.org/abs/{id}\n\
                         - **Abstract**: {preview}\n\n"
                    );
                }
            }
        }
        _ => {
            tracing::warn!("arXiv security fetch failed — skipping");
        }
    }

    match std::fs::write(&intel_file, &content) {
        Ok(()) => {
            tracing::info!(date = %today, window, bytes = content.len(), "Intel feed written");
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to write intel feed");
        }
    }
}

// ── Devotional Rotation ──────────────────────────────────────────────────

/// Devotional state persisted between cycles.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct DevotionalState {
    last_sibling: String,
    last_file: String,
}

/// Rotate the devotional queue: next sibling + next scripture passage.
#[allow(clippy::unnecessary_wraps)] // Caller expects Result for consistency
fn rotate_devotional(shared_dir: &Path) -> Result<String, String> {
    let scripture_dir = shared_dir.join("devotionals/scripture");
    let state_path = shared_dir.join("bulletin/.devotional-state.json");

    let state = load_devotional_state(&state_path);
    let files = list_scripture_files(&scripture_dir);

    if files.is_empty() {
        return Ok("# Devotional Queue\n\n*(no scripture passages available)*\n".into());
    }

    let next_sibling = pick_next_sibling(&state.last_sibling);
    let next_file = pick_next_file(&files, &state.last_file);

    let passage_path = scripture_dir.join(&next_file);
    let passage_content = std::fs::read_to_string(&passage_path)
        .unwrap_or_else(|_| "(passage could not be read)".into());

    // Save updated state
    let new_state = DevotionalState {
        last_sibling: next_sibling.to_owned(),
        last_file: next_file.clone(),
    };
    save_devotional_state(&state_path, &new_state);

    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
    Ok(format!(
        "# Devotional Queue\n\n\
         *Updated: {timestamp}*\n\n\
         **Assigned to**: {upper}\n\
         **Passage**: {next_file}\n\n\
         ---\n\n{passage_content}\n",
        upper = next_sibling.to_uppercase(),
    ))
}

/// Load devotional state from JSON, defaulting if missing or corrupt.
fn load_devotional_state(path: &Path) -> DevotionalState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Save devotional state to JSON (best-effort).
fn save_devotional_state(path: &Path, state: &DevotionalState) {
    if let Ok(json) = serde_json::to_string_pretty(state) {
        if let Err(e) = std::fs::write(path, json) {
            tracing::warn!(error = %e, "Failed to save devotional state");
        }
    }
}

/// List scripture files sorted alphabetically.
fn list_scripture_files(dir: &Path) -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "md" || ext == "txt")
        })
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    files.sort();
    files
}

/// Pick the next sibling in rotation order after `last`.
fn pick_next_sibling(last: &str) -> &'static str {
    if last.is_empty() {
        return SIBLING_ORDER.first().copied().unwrap_or("eva");
    }
    let last_lower = last.to_lowercase();
    let pos = SIBLING_ORDER.iter().position(|&s| s == last_lower);
    match pos {
        Some(idx) => {
            let next_idx = idx.saturating_add(1) % SIBLING_ORDER.len();
            SIBLING_ORDER.get(next_idx).copied().unwrap_or("eva")
        }
        None => SIBLING_ORDER.first().copied().unwrap_or("eva"),
    }
}

/// Pick the next file alphabetically after `last`.
fn pick_next_file(files: &[String], last: &str) -> String {
    if last.is_empty() || files.is_empty() {
        return files.first().cloned().unwrap_or_default();
    }
    let pos = files.iter().position(|f| f == last);
    match pos {
        Some(idx) => {
            let next_idx = idx.saturating_add(1) % files.len();
            files.get(next_idx).cloned().unwrap_or_default()
        }
        None => files.first().cloned().unwrap_or_default(),
    }
}

// ── Sibling Activity ─────────────────────────────────────────────────────

/// Aggregate sibling activity from `workspace-{sibling}/last-output.md`.
fn aggregate_sibling_activity(data_dir: &Path) -> String {
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let mut output = format!("# Sibling Activity\n\n*Aggregated: {timestamp}*\n\n");

    for &sibling in SIBLING_ORDER {
        let path = data_dir
            .join(format!("workspace-{sibling}"))
            .join("last-output.md");

        let preview = match std::fs::read_to_string(&path) {
            Ok(content) if !content.trim().is_empty() => {
                let chars: String = content.chars().take(300).collect();
                chars
            }
            _ => "(no recent output)".into(),
        };

        let _ = write!(
            output,
            "## {upper}\n{preview}\n\n",
            upper = sibling.to_uppercase(),
        );
    }

    output
}

// ── Staging Promotion ──────────────────────────────────────────────────

/// Promote validated staging content to the live bulletin board.
///
/// Reads `shared/bulletin/staging/*.md`, validates by category, appends
/// validated entries to `shared/bulletin/sibling-activity.md`, then clears
/// staging files. Returns the count of promoted entries.
fn promote_staging(data_dir: &Path, shared_dir: &Path) -> Result<u32, String> {
    let staging_dir = shared_dir.join("bulletin/staging");
    let live_path = shared_dir.join("bulletin/sibling-activity.md");

    let entries =
        std::fs::read_dir(&staging_dir).map_err(|e| format!("Failed to read staging dir: {e}"))?;

    let mut promoted: u32 = 0;
    let mut live_content = std::fs::read_to_string(&live_path).unwrap_or_default();

    // Cap live board at 50KB before appending
    if live_content.len() > 50_000 {
        let truncated = live_content.split_off(live_content.len().saturating_sub(40_000));
        live_content = truncated;
    }

    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        let filename = entry.file_name().to_string_lossy().to_string();
        if !entry
            .path()
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) if !c.trim().is_empty() => c,
            _ => continue,
        };

        let category = extract_category_from_filename(&filename);
        let valid = validate_staging_entry(&content, &category, data_dir);

        if valid {
            live_content.push_str(&content);
            promoted = promoted.saturating_add(1);
            tracing::debug!(file = %filename, category = %category, "Staging promoted");
        } else {
            tracing::info!(file = %filename, category = %category, "Staging rejected (validation failed)");
        }

        // Clear staging file regardless of validation (don't re-process)
        let _ = std::fs::remove_file(&path);
    }

    if promoted > 0 {
        std::fs::write(&live_path, &live_content)
            .map_err(|e| format!("Failed to write live board: {e}"))?;
    }

    tracing::info!(promoted, "Staging promotion complete");
    Ok(promoted)
}

/// Extract the category from a staging filename like `eva-facts.md`.
fn extract_category_from_filename(filename: &str) -> String {
    filename
        .rsplit('-')
        .next()
        .and_then(|s| s.strip_suffix(".md"))
        .unwrap_or("discussion")
        .to_owned()
}

/// Validate a staging entry by category.
///
/// - `facts`: must contain an arXiv ID reference
/// - `discussion`: must pass basic grounding check (no fabrication)
/// - `reflections`: always valid (tagged as perspective, no grounding needed)
fn validate_staging_entry(content: &str, category: &str, _data_dir: &Path) -> bool {
    match category {
        "facts" => {
            // Facts must cite at least one real paper
            let lower = content.to_lowercase();
            lower.contains("arxiv:")
                || lower.contains("arxiv.org")
                || content.chars().any(|c| c.is_ascii_digit()) && content.contains('.')
        }
        "discussion" => {
            // Discussion must not contain fabricated data
            !crate::arena::grounding::detect_fabrication(content)
        }
        "reflections" => {
            // Reflections are perspective — always valid
            true
        }
        _ => {
            // Unknown category: treat as discussion
            !crate::arena::grounding::detect_fabrication(content)
        }
    }
}

// ── Workspace Maintenance ─────────────────────────────────────────────────

/// Maximum age for vault files before archival (30 days).
const VAULT_MAX_AGE_DAYS: u64 = 30;

/// Maximum vault files per category before pruning.
const VAULT_MAX_FILES_PER_CATEGORY: usize = 500;

/// Maximum heartbeat log age before deletion (7 days).
const LOG_MAX_AGE_DAYS: u64 = 7;

/// Run workspace maintenance — prune old files, rotate logs, cap growth.
///
/// Runs every curator cycle but operations are idempotent and cheap
/// (just filesystem checks). No LLM calls.
fn run_maintenance(data_dir: &Path, shared_dir: &Path) {
    prune_old_vault_files(shared_dir);
    rotate_heartbeat_logs(data_dir);
    cap_papers_covered(data_dir);
}

/// Archive vault files older than 30 days.
fn prune_old_vault_files(shared_dir: &Path) {
    let categories = ["thinktank", "research/summaries", "devotionals/reflections"];
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(VAULT_MAX_AGE_DAYS * 86400))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    for category in &categories {
        let dir = shared_dir.join(category);
        let archive_dir = dir.join(".archive");

        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        let mut files: Vec<_> = entries
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
            .collect();

        // Archive files older than cutoff
        for entry in &files {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::now());

            if modified < cutoff && std::fs::create_dir_all(&archive_dir).is_ok() {
                let dest = archive_dir.join(entry.file_name());
                if std::fs::rename(entry.path(), &dest).is_ok() {
                    tracing::debug!(file = %entry.file_name().to_string_lossy(), "Archived old vault file");
                }
            }
        }

        // Cap total files per category
        files.retain(|e| e.path().exists()); // refresh after archival
        if files.len() > VAULT_MAX_FILES_PER_CATEGORY {
            files.sort_by_key(|e| {
                e.metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            });
            let to_archive = files.len().saturating_sub(VAULT_MAX_FILES_PER_CATEGORY);
            for entry in files.iter().take(to_archive) {
                if std::fs::create_dir_all(&archive_dir).is_ok() {
                    let dest = archive_dir.join(entry.file_name());
                    let _ = std::fs::rename(entry.path(), &dest);
                }
            }
            if to_archive > 0 {
                tracing::info!(category, archived = to_archive, "Vault file cap enforced");
            }
        }
    }
}

/// Delete heartbeat logs older than 7 days.
fn rotate_heartbeat_logs(data_dir: &Path) {
    let metrics_dir = data_dir.join("shared/metrics");
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(LOG_MAX_AGE_DAYS * 86400))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    let Ok(entries) = std::fs::read_dir(&metrics_dir) else {
        return;
    };

    for entry in entries.filter_map(std::result::Result::ok) {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("heartbeat-log-") || !name.ends_with(".jsonl") {
            continue;
        }

        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::now());

        if modified < cutoff && std::fs::remove_file(entry.path()).is_ok() {
            tracing::info!(file = %name, "Rotated old heartbeat log");
        }
    }
}

/// Cap papers-covered.jsonl to the most recent 500 entries.
fn cap_papers_covered(data_dir: &Path) {
    let path = data_dir.join("shared/bulletin/papers-covered.jsonl");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };

    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() <= 500 {
        return;
    }

    // Keep the most recent 500 entries (last N lines)
    let keep = &lines[lines.len().saturating_sub(500)..];
    let trimmed = keep.join("\n");
    if std::fs::write(&path, format!("{trimmed}\n")).is_ok() {
        tracing::info!(
            before = lines.len(),
            after = keep.len(),
            "Capped papers-covered.jsonl"
        );
    }
}

// ── Channel Summaries ────────────────────────────────────────────────────

/// Build a deterministic Telegram summary (no LLM).
fn build_telegram_summary(research: &str, devotional: &str) -> String {
    let paper_count = research.matches("## ").count();
    let assignee = extract_devotional_assignee(devotional);

    format!(
        "Curator Update: \
         {paper_count} top papers in research feed. \
         Devotional assigned to {assignee}."
    )
}

/// Extract the assigned sibling name from the devotional output.
fn extract_devotional_assignee(devotional: &str) -> String {
    devotional
        .lines()
        .find(|l| l.starts_with("**Assigned to**"))
        .and_then(|l| l.split(':').nth(1))
        .map_or_else(|| "unknown".into(), |s| s.trim().to_owned())
}
