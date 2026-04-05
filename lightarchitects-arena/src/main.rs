//! `lightarchitects-arena` CLI — plug-and-play training data factory for MCP tool-use LLMs.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use lightarchitects_arena::ayin_exporter::{AyinExportConfig, export_ayin_sft};
use lightarchitects_arena::config::ArenaConfig;
use lightarchitects_arena::discovery;
use lightarchitects_arena::exercises::{self, GeneratorConfig};

/// Light Architects Arena — generate scored training data from real MCP tool interactions.
#[derive(Debug, Parser)]
#[command(name = "lightarchitects-arena", version, about, long_about = None)]
struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    command: Command,
}

/// Available arena commands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Run the full arena pipeline: discover → generate → execute → score → export.
    Run {
        /// Path to the arena configuration YAML file.
        #[arg(short, long)]
        config: PathBuf,
        /// Generate exercises without executing them.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Verbosity level (repeat for more: -v, -vv, -vvv).
        #[arg(short, long, action = clap::ArgAction::Count)]
        verbose: u8,
    },
    /// Discover and list tools from configured MCP servers without running exercises.
    Discover {
        /// Path to the arena configuration YAML file.
        #[arg(short, long)]
        config: PathBuf,
    },
    /// Export AYIN session traces as SFT training data.
    ExportAyin {
        /// Output `.jsonl` path.
        #[arg(short, long, default_value = "ayin_sft.jsonl")]
        output: PathBuf,
        /// Minimum tool calls per conversation example (lower = more examples).
        #[arg(long, default_value_t = 2)]
        min_tools: usize,
        /// Include sibling traces that resulted in errors (skipped by default).
        #[arg(long, default_value_t = false)]
        include_errors: bool,
        /// Suppress pivot reasoning from assistant turns.
        #[arg(long, default_value_t = false)]
        no_pivots: bool,
    },
    /// Train a model using arena-generated training data.
    Train {
        /// Training method to use.
        #[arg(short, long)]
        method: TrainMethod,
        /// Path to the arena output directory containing JSONL files.
        #[arg(short, long)]
        data: PathBuf,
        /// RL algorithm to use (only for rl method).
        #[arg(long, default_value = "grpo")]
        rl_algo: RlAlgo,
        /// Path to a model or `HuggingFace` model ID to fine-tune.
        #[arg(long)]
        model: Option<String>,
        /// `LoRA` rank for parameter-efficient fine-tuning.
        #[arg(long, default_value_t = 16)]
        lora_rank: u32,
    },
}

/// Training methods.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum TrainMethod {
    /// Supervised fine-tuning (behavioral cloning from good trajectories).
    Sft,
    /// Direct preference optimization (chosen vs rejected pairs).
    Dpo,
    /// Reinforcement learning (policy gradient with 8-dim rewards).
    Rl,
}

/// RL algorithm selection.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum RlAlgo {
    /// Group Relative Policy Optimization (sparse rewards).
    Grpo,
    /// REINFORCE with Running-mean Baseline Normalization (dense rewards).
    ReinforceRebn,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Run {
            config,
            dry_run,
            verbose,
        } => run_pipeline(&config, dry_run, verbose),
        Command::Discover { config } => run_discover(&config),
        Command::ExportAyin {
            output,
            min_tools,
            include_errors,
            no_pivots,
        } => run_export_ayin(&output, min_tools, include_errors, no_pivots),
        Command::Train {
            method,
            data,
            rl_algo,
            model,
            lora_rank,
        } => run_train(method, &data, rl_algo, model.as_deref(), lora_rank),
    }
}

fn run_export_ayin(
    output: &Path,
    min_tools: usize,
    include_errors: bool,
    no_pivots: bool,
) -> ExitCode {
    let default = AyinExportConfig::default();
    let config = AyinExportConfig {
        output_path: output.to_path_buf(),
        min_tool_calls: min_tools,
        skip_error_traces: !include_errors,
        include_pivots: !no_pivots,
        ..default
    };

    println!(
        "lightarchitects-arena export-ayin: reading from\n  {}\n  {}",
        config.conversations_dir.display(),
        config.sibling_traces_dir.display()
    );

    match export_ayin_sft(&config) {
        Ok(count) => {
            println!(
                "lightarchitects-arena export-ayin: wrote {count} example(s) to {}",
                output.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("lightarchitects-arena export-ayin: error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_pipeline(config_path: &std::path::Path, dry_run: bool, verbose: u8) -> ExitCode {
    let arena_config = match ArenaConfig::from_file(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("lightarchitects-arena: config error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let server_count = arena_config.mcp_servers.len();
    let exercise_count = arena_config.exercises.count;
    println!(
        "lightarchitects-arena: loaded config — {server_count} server(s), \
         {exercise_count} exercise(s)"
    );

    // Step 1: Discover tools from MCP servers.
    println!("lightarchitects-arena: discovering tools...");
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let registry = match rt.block_on(discovery::discover_all(&arena_config.mcp_servers)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("lightarchitects-arena: discovery failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "lightarchitects-arena: discovered {} tool(s) across {} server(s)",
        registry.tool_count(),
        registry.server_count()
    );

    if verbose > 0 {
        for (server, tool) in registry.all_tools() {
            let desc = tool.description.as_deref().unwrap_or("-");
            println!("  [{server}] {} — {desc}", tool.name);
        }
    }

    // Step 2: Generate exercises.
    println!("lightarchitects-arena: generating exercises...");
    let gen_config = GeneratorConfig {
        types: arena_config.exercises.types.clone(),
        difficulties: arena_config.exercises.difficulty.clone(),
        count: arena_config.exercises.count,
        seed: arena_config.exercises.seed.unwrap_or(42),
    };
    let generated_exercises = match exercises::generate(&registry, &gen_config) {
        Ok(ex) => ex,
        Err(e) => {
            eprintln!("lightarchitects-arena: exercise generation failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "lightarchitects-arena: generated {} exercise(s)",
        generated_exercises.len()
    );

    if dry_run {
        println!("lightarchitects-arena: dry-run — writing exercise manifest only");
        let manifest_path = arena_config.output.path.join("exercise_manifest.json");
        if let Err(e) = std::fs::create_dir_all(&arena_config.output.path) {
            eprintln!("lightarchitects-arena: failed to create output dir: {e}");
            return ExitCode::FAILURE;
        }
        if let Err(e) = exercises::write_manifest(&generated_exercises, &manifest_path) {
            eprintln!("lightarchitects-arena: failed to write manifest: {e}");
            return ExitCode::FAILURE;
        }
        println!(
            "lightarchitects-arena: manifest written to {}",
            manifest_path.display()
        );
        return ExitCode::SUCCESS;
    }

    // Step 3: Execute exercises (requires LLM endpoint).
    // For now, this is a placeholder — full execution wired in Phase 11.
    println!(
        "lightarchitects-arena: execution engine ready — \
         connect to {} for live execution",
        arena_config.model.endpoint
    );
    println!(
        "lightarchitects-arena: note: full execution requires a running LLM endpoint. \
         Use --dry-run for offline exercise generation."
    );

    ExitCode::SUCCESS
}

fn run_discover(config_path: &std::path::Path) -> ExitCode {
    let arena_config = match ArenaConfig::from_file(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("lightarchitects-arena: config error: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "lightarchitects-arena: discovering tools from {} server(s)...",
        arena_config.mcp_servers.len()
    );

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let registry = match rt.block_on(discovery::discover_all(&arena_config.mcp_servers)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("lightarchitects-arena: discovery failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "\nDiscovered {} tool(s) across {} server(s):\n",
        registry.tool_count(),
        registry.server_count()
    );

    for server_name in registry.server_names() {
        println!("[{server_name}]");
        if let Some(tools) = registry.tools_for_server(server_name) {
            for tool in tools {
                let desc = tool.description.as_deref().unwrap_or("(no description)");
                println!("  {} — {desc}", tool.name);
            }
        }
        println!();
    }

    // Cache schemas to the configured output directory (not CWD).
    if let Err(e) = std::fs::create_dir_all(&arena_config.output.path) {
        eprintln!("lightarchitects-arena: warning: failed to create output dir: {e}");
    }
    let cache_path = arena_config.output.path.join("tool_schemas.json");
    if let Err(e) = registry.save_cache(&cache_path) {
        eprintln!("lightarchitects-arena: warning: failed to cache schemas: {e}");
    } else {
        println!("Schemas cached to {}", cache_path.display());
    }

    ExitCode::SUCCESS
}

fn run_train(
    method: TrainMethod,
    data: &std::path::Path,
    rl_algo: RlAlgo,
    model: Option<&str>,
    lora_rank: u32,
) -> ExitCode {
    let method_name = match method {
        TrainMethod::Sft => "SFT",
        TrainMethod::Dpo => "DPO",
        TrainMethod::Rl => match rl_algo {
            RlAlgo::Grpo => "RL (GRPO)",
            RlAlgo::ReinforceRebn => "RL (REINFORCE+ReBN)",
        },
    };

    println!("lightarchitects-arena: training with {method_name}");
    println!("lightarchitects-arena: data directory: {}", data.display());
    if let Some(model_id) = model {
        println!("lightarchitects-arena: base model: {model_id}");
    }
    println!("lightarchitects-arena: LoRA rank: {lora_rank}");

    // Locate the Python training script.
    let script_name = match method {
        TrainMethod::Sft => "sft_train.py",
        TrainMethod::Dpo => "dpo_train.py",
        TrainMethod::Rl => "rl_train.py",
    };

    // Try to find the script relative to the binary.
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf));

    let script_candidates = [
        // In-tree (development).
        std::path::PathBuf::from(format!("training/{script_name}")),
        // Relative to binary.
        exe_dir
            .as_ref()
            .map(|d| d.join(format!("../training/{script_name}")))
            .unwrap_or_default(),
    ];

    let script_path = script_candidates.iter().find(|p| p.exists());

    if let Some(path) = script_path {
        println!(
            "lightarchitects-arena: found training script at {}",
            path.display()
        );
        println!(
            "lightarchitects-arena: run manually:\n  python {} --data {} --model {} --lora-rank {lora_rank}",
            path.display(),
            data.display(),
            model.unwrap_or("<model-path>"),
        );
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "lightarchitects-arena: training script '{script_name}' not found. \
             Expected in ./training/ directory."
        );
        ExitCode::FAILURE
    }
}
