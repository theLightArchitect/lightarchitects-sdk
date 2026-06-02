//! `lightarchitects loop <kind> <goal>` — run a strategy loop from the CLI.
//!
//! Dispatches one of the five agentic strategies (react, ach, itt, cove, reflexion)
//! using the configured sibling clients and streams progress to stdout.
//!
//! # Usage
//!
//! ```text
//! lightarchitects loop react "investigate the auth bug in soul handler"
//! lightarchitects loop ach "why does the SSE stream drop after 30 s?"
//! lightarchitects loop itt "trace the helix write path"
//! lightarchitects loop cove "verify HMAC comparison is constant-time"    --executor seraph
//! lightarchitects loop reflexion "document the AYIN span schema"        --max-turns 8
//! lightarchitects loop react "..." --ndjson        # machine-readable NDJSON output
//! ```
//!
//! # Flags
//!
//! | Flag | Default | Description |
//! |------|---------|-------------|
//! | `--executor <sibling>` | per-strategy default | Override executor sibling |
//! | `--max-turns <n>` | 20 | Maximum strategy steps |
//! | `--budget <usd>` | unlimited | USD ceiling |
//! | `--role <role>` | none | Domain role for profile resolution (`engineer`, `security`, …) |
//! | `--phase <phase>` | none | LASDLC phase context forwarded to AYIN dispatch span |
//! | `--ndjson` | off | Emit NDJSON events instead of human text |

use crate::agent_stream::strategy::{ExecutorHint, StrategyKind, StrategyRequest};
use crate::agent_stream::{NdjsonTransport, TtyTransport};
use crate::config::GatewayConfig;
use crate::error::GatewayError;

/// Execute a strategy loop and stream results to stdout.
///
/// Parses CLI args of the form `<kind> <goal...> [flags]`.
///
/// # Errors
///
/// Returns [`GatewayError::MissingParam`] if `kind` or `goal` are absent,
/// [`GatewayError::UnknownTool`] for an unrecognised strategy kind, or
/// [`GatewayError::Internal`] if the strategy executor fails.
pub async fn execute(config: &GatewayConfig, args: &[String]) -> Result<(), GatewayError> {
    // Parse kind from first positional arg (skip flags)
    let kind_str = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .ok_or(GatewayError::MissingParam("strategy kind"))?
        .as_str();

    let strategy = parse_kind(kind_str).ok_or_else(|| {
        GatewayError::UnknownTool(format!(
            "Unknown strategy '{kind_str}'. Valid: react, ach, itt, cove, reflexion"
        ))
    })?;

    // Goal = all positional args after kind, joined by space (skip flags)
    let goal: String = args
        .iter()
        .skip_while(|a| a.as_str() != kind_str)
        .skip(1)
        .filter(|a| !a.starts_with('-') && !is_flag_value(args, a))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");

    if goal.is_empty() {
        return Err(GatewayError::MissingParam("goal"));
    }

    let max_turns = find_flag_u32(args, "--max-turns");
    let max_budget_usd = find_flag_f64(args, "--budget");
    let executor = find_flag_str(args, "--executor").and_then(parse_executor_hint);
    let role = find_flag_str(args, "--role");
    let phase = find_flag_str(args, "--phase");
    let ndjson = args.iter().any(|a| a == "--ndjson" || a == "--json");

    let req = StrategyRequest {
        strategy,
        goal,
        executor,
        max_turns,
        max_budget_usd,
        session_id: find_flag_str(args, "--session-id"),
        role,
        phase,
    };

    if ndjson {
        let mut transport = NdjsonTransport::new(tokio::io::stdout());
        crate::agent_stream::strategy::run_strategy(req, config, &mut transport)
            .await
            .map_err(|e| GatewayError::Internal(e.to_string()))
    } else {
        let mut transport = TtyTransport::new(tokio::io::stdout());
        crate::agent_stream::strategy::run_strategy(req, config, &mut transport)
            .await
            .map_err(|e| GatewayError::Internal(e.to_string()))
    }
}

// ── Arg helpers ───────────────────────────────────────────────────────────────

fn parse_kind(s: &str) -> Option<StrategyKind> {
    match s.to_lowercase().as_str() {
        "react" => Some(StrategyKind::React),
        "ach" => Some(StrategyKind::Ach),
        "itt" => Some(StrategyKind::Itt),
        "cove" => Some(StrategyKind::CoVe),
        "reflexion" => Some(StrategyKind::Reflexion),
        _ => None,
    }
}

fn parse_executor_hint(s: String) -> Option<ExecutorHint> {
    match s.to_lowercase().as_str() {
        "quantum" | "q" => Some(ExecutorHint::Quantum),
        "seraph" | "s" => Some(ExecutorHint::Seraph),
        "corso" | "c" => Some(ExecutorHint::Corso),
        "soul" | "k" => Some(ExecutorHint::Soul),
        "eva" | "e" => Some(ExecutorHint::Eva),
        _ => None,
    }
}

fn find_flag_str(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn find_flag_u32(args: &[String], flag: &str) -> Option<u32> {
    find_flag_str(args, flag).and_then(|s| s.parse().ok())
}

fn find_flag_f64(args: &[String], flag: &str) -> Option<f64> {
    find_flag_str(args, flag).and_then(|s| s.parse().ok())
}

/// Returns `true` if `candidate` is the value immediately following a known flag.
///
/// Used to exclude flag values from the goal positional collection.
fn is_flag_value(args: &[String], candidate: &str) -> bool {
    const FLAGS: &[&str] = &[
        "--executor",
        "--max-turns",
        "--budget",
        "--session-id",
        "--role",
        "--phase",
    ];
    args.windows(2)
        .any(|w| FLAGS.contains(&w[0].as_str()) && w[1] == candidate)
}
