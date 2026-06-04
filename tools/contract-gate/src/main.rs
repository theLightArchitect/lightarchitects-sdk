//! `contract-gate` — Rust port of `standards/canon/contracts/validate.sh`.
//!
//! Validates every YAML contract in `--contracts-dir` against `--schema`,
//! then runs the symmetric-edge sweep on `mcp.capability ↔ wire.mcp` pairs.
//! Exit 0 on clean, 1 on any failure, 2 on internal error.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use contract_gate::{ContractGate, Report};

#[derive(Parser, Debug)]
#[command(
    name = "contract-gate",
    about = "JSON Schema 2020-12 + symmetric-edge sweep over standards/canon/contracts/"
)]
struct Args {
    /// Path to `la-contracts.schema.json`.
    #[arg(long, default_value = "standards/canon/la-contracts.schema.json")]
    schema: PathBuf,

    /// Root of the contracts tree (recursively scanned for `*.yaml`).
    #[arg(long, default_value = "standards/canon/contracts")]
    contracts_dir: PathBuf,

    /// Cap the number of failure messages shown per pass.
    #[arg(long, default_value_t = 20)]
    max_shown: usize,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let gate = match ContractGate::new(&args.schema, &args.contracts_dir) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("ERROR: {e}");
            return ExitCode::from(2);
        }
    };
    let report = match gate.validate() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ERROR: {e}");
            return ExitCode::from(2);
        }
    };
    print_report(&report, args.max_shown);
    if report.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn print_report(report: &Report, max_shown: usize) {
    println!();
    let pct = if report.total_contracts == 0 {
        100
    } else {
        report.schema_pass_count * 100 / report.total_contracts
    };
    println!(
        "{}/{} contracts validate ({pct}%)",
        report.schema_pass_count, report.total_contracts
    );

    if !report.schema_failures.is_empty() {
        print_schema_failures(report, max_shown);
        return;
    }

    print_edge_sweep_header(report);

    if report.edge_violations.is_empty() {
        println!("  ✓ all edges reciprocated");
        println!();
    } else {
        print_edge_violations(report, max_shown);
    }
}

fn print_schema_failures(report: &Report, max_shown: usize) {
    println!();
    println!("{} schema failures by class:", report.schema_failures.len());
    for (cls, n) in report.schema_class_counts() {
        println!("  {n:3} × {}", cls.as_str());
    }
    println!();
    for f in report.schema_failures.iter().take(max_shown) {
        println!("FAIL {}", f.file.display());
        for m in &f.messages {
            println!("  → {m}");
        }
    }
    if report.schema_failures.len() > max_shown {
        println!(
            "  ({} more failures suppressed)",
            report.schema_failures.len() - max_shown
        );
    }
}

fn print_edge_sweep_header(report: &Report) {
    println!();
    println!("Symmetric-edge sweep (mcp.capability ↔ wire.mcp, LÆX 2026-06-03):");
    println!(
        "  mcp.capability contracts:    {}",
        report.mcp_capability_count
    );
    println!("  wire.mcp contracts:          {}", report.wire_mcp_count);
    println!("  forward edges declared:      {}", report.forward_edges);
    println!("  backward edges declared:     {}", report.backward_edges);
}

fn print_edge_violations(report: &Report, max_shown: usize) {
    println!();
    println!(
        "{} symmetric-edge violations:",
        report.edge_violations.len()
    );
    for (cls, n) in report.edge_class_counts() {
        println!("  {n:3} × {}", cls.as_str());
    }
    println!();
    for ev in report.edge_violations.iter().take(max_shown) {
        println!("FAIL [{}] {} ↛ {}", ev.class.as_str(), ev.from, ev.to);
        println!("  → {}", ev.detail);
    }
    if report.edge_violations.len() > max_shown {
        println!(
            "  ({} more violations suppressed)",
            report.edge_violations.len() - max_shown
        );
    }
}
