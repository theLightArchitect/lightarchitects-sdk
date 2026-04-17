//! `lightarchitects builds` — display the build portfolio from the SOUL vault.
//!
//! Reads `~/.soul/helix/corso/builds/active.yaml` and prints a tiered
//! summary of all tracked projects. Supports `--output-format json` for
//! machine-readable output.

use serde::{Deserialize, Serialize};

use crate::cli::output::{OutputMode, print_value};
use crate::error::GatewayError;

/// Build tracking subcommands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildsCommand {
    /// List all builds grouped by tier.
    List,
    /// Show details for a specific build by name.
    Show {
        /// Build name (case-insensitive match).
        name: String,
    },
}

/// Top-level schema for `active.yaml`.
#[derive(Debug, Serialize, Deserialize)]
struct BuildsDocument {
    #[serde(default)]
    schema: Option<String>,
    #[serde(default)]
    builds: Vec<BuildEntry>,
}

/// A single build entry from `active.yaml`.
#[derive(Debug, Serialize, Deserialize)]
struct BuildEntry {
    name: String,
    #[serde(default)]
    codename: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    tier: Option<u32>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    binary: Option<String>,
    #[serde(default)]
    deploy: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    siblings: Option<Vec<String>>,
    #[serde(default)]
    last_commit: Option<LastCommit>,
}

/// Nested `last_commit` struct.
#[derive(Debug, Serialize, Deserialize)]
struct LastCommit {
    #[serde(default)]
    sha: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    date: Option<String>,
}

/// Tier display metadata.
struct TierInfo {
    label: &'static str,
    color: &'static str,
}

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";

fn tier_info(tier: u32) -> TierInfo {
    match tier {
        1 => TierInfo {
            label: "Production",
            color: "\x1b[32m",
        },
        2 => TierInfo {
            label: "Active Development",
            color: "\x1b[34m",
        },
        3 => TierInfo {
            label: "Experimental",
            color: "\x1b[33m",
        },
        4 => TierInfo {
            label: "Prototype / Brand",
            color: "\x1b[35m",
        },
        _ => TierInfo {
            label: "Unknown",
            color: "\x1b[37m",
        },
    }
}

/// Execute a builds subcommand.
///
/// # Errors
///
/// Returns [`GatewayError`] if the vault path cannot be resolved or
/// `active.yaml` cannot be read/parsed.
pub fn execute(args: &[String], mode: OutputMode) -> Result<(), GatewayError> {
    let cmd = match args.first().map(String::as_str) {
        Some("list" | "ls") | None => BuildsCommand::List,
        Some("show") => {
            let name = args
                .get(1)
                .ok_or(GatewayError::MissingParam("build name"))?
                .clone();
            BuildsCommand::Show { name }
        }
        Some(other) => {
            eprintln!("Unknown builds subcommand: {other}");
            eprintln!("Available: list (ls), show <name>");
            return Err(GatewayError::UnknownTool(other.to_owned()));
        }
    };

    let doc = load_builds()?;

    match cmd {
        BuildsCommand::List => {
            list_builds(&doc, mode);
            Ok(())
        }
        BuildsCommand::Show { name } => show_build(&doc, &name, mode),
    }
}

/// Load and parse `active.yaml` from the SOUL vault.
fn load_builds() -> Result<BuildsDocument, GatewayError> {
    let home =
        std::env::var_os("HOME").ok_or(GatewayError::Config(crate::error::ConfigError::NoHome))?;
    let helix_root = std::path::PathBuf::from(&home).join(".soul").join("helix");

    let path = helix_root.join("corso").join("builds").join("active.yaml");

    let content = std::fs::read_to_string(&path)
        .map_err(|e| GatewayError::Internal(format!("failed to read {}: {e}", path.display())))?;

    serde_yaml::from_str(&content)
        .map_err(|e| GatewayError::Internal(format!("failed to parse {}: {e}", path.display())))
}

/// Print all builds grouped by tier.
fn list_builds(doc: &BuildsDocument, mode: OutputMode) {
    if doc.builds.is_empty() {
        println!("No builds found in vault.");
        return;
    }

    match mode {
        OutputMode::Json => {
            print_value(mode, &serde_json::to_value(doc).unwrap_or_default());
        }
        OutputMode::Human => {
            let mut tiers: Vec<(u32, Vec<&BuildEntry>)> = Vec::new();

            for build in &doc.builds {
                let tier = build.tier.unwrap_or(0);
                if let Some(entry) = tiers.iter_mut().find(|(t, _)| *t == tier) {
                    entry.1.push(build);
                } else {
                    tiers.push((tier, vec![build]));
                }
            }
            tiers.sort_by_key(|(t, _)| *t);

            for (tier_num, builds) in &tiers {
                let info = tier_info(*tier_num);
                println!("{}Tier {} — {}{}", info.color, tier_num, info.label, RESET);

                for build in builds {
                    let status_marker = match build.status.as_deref() {
                        Some("production") => "\x1b[32m●\x1b[0m",
                        Some("active") => "\x1b[34m●\x1b[0m",
                        Some("experimental") => "\x1b[33m●\x1b[0m",
                        Some("prototype") => "\x1b[35m●\x1b[0m",
                        _ => "\x1b[37m●\x1b[0m",
                    };

                    let version = build.version.as_deref().unwrap_or("—");
                    let language = build.language.as_deref().unwrap_or("—");
                    let sha_short: &str = build
                        .last_commit
                        .as_ref()
                        .and_then(|c| c.sha.as_deref())
                        .map_or("—", |s| &s[..s.len().min(7)]);

                    println!(
                        "  {status_marker} {:<20} v{:<8} {:<14} {sha_short}",
                        build.name, version, language
                    );
                }
                println!();
            }

            println!("{DIM}Source: ~/.soul/helix/corso/builds/active.yaml{RESET}");
        }
    }
}

/// Show details for a single build by name (case-insensitive).
fn show_build(doc: &BuildsDocument, name: &str, mode: OutputMode) -> Result<(), GatewayError> {
    let build = doc
        .builds
        .iter()
        .find(|b| b.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| {
            GatewayError::Internal(format!(
                "build '{name}' not found. Available: {}",
                doc.builds
                    .iter()
                    .map(|b| b.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;

    match mode {
        OutputMode::Json => {
            print_value(mode, &serde_json::to_value(build).unwrap_or_default());
        }
        OutputMode::Human => {
            let tier = build.tier.unwrap_or(0);
            let info = tier_info(tier);

            println!(
                "{}{} ({}){}",
                info.color,
                build.name,
                build.codename.as_deref().unwrap_or("—"),
                RESET
            );
            println!("  Tier:     {} — {}", tier, info.label);
            println!("  Version:  v{}", build.version.as_deref().unwrap_or("—"));
            println!("  Status:   {}", build.status.as_deref().unwrap_or("—"));
            println!("  Language: {}", build.language.as_deref().unwrap_or("—"));
            println!("  Path:     {}", build.path.as_deref().unwrap_or("—"));
            println!("  Binary:   {}", build.binary.as_deref().unwrap_or("—"));
            println!("  Deploy:   {}", build.deploy.as_deref().unwrap_or("—"));

            if let Some(ref commit) = build.last_commit {
                println!(
                    "  Commit:   {} — {}",
                    commit.sha.as_deref().unwrap_or("—"),
                    commit.message.as_deref().unwrap_or("—")
                );
                println!("  Date:     {}", commit.date.as_deref().unwrap_or("—"));
            }

            if let Some(ref siblings) = build.siblings {
                println!("  Siblings: {}", siblings.join(", "));
            }

            println!(
                "\n  {}",
                build.description.as_deref().unwrap_or("No description.")
            );
        }
    }

    Ok(())
}
