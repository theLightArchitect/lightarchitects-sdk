//! CLI headed integration tests — spawn the real `lightarchitects` binary
//! and assert on stdout/stderr behaviour.
//!
//! These tests require a compiled binary. Cargo resolves the path via
//! `env!("CARGO_BIN_EXE_lightarchitects")`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::uninlined_format_args,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::items_after_statements
)]

use std::process::{Command, Stdio};

/// Path to the compiled gateway binary, resolved by Cargo at compile time.
const GATEWAY_BIN: &str = env!("CARGO_BIN_EXE_lightarchitects");

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Run a one-shot command with the given args and return (stdout, stderr, exit).
fn run(args: &[&str]) -> (String, String, std::process::ExitStatus) {
    let output = Command::new(GATEWAY_BIN)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn gateway binary");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stdout, stderr, output.status)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// `--help` is treated as unknown subcommand; stderr contains usage text.
#[test]
fn help_flag_prints_usage_to_stderr() {
    let (stdout, stderr, _status) = run(&["--help"]);
    assert!(
        stderr.contains("Usage:") || stderr.contains("lightarchitects"),
        "--help stderr should contain usage; got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// `lightarchitects --version` exits 0 and emits a version string.
#[test]
fn version_flag_exits_zero() {
    let (stdout, _stderr, status) = run(&["--version"]);
    assert!(status.success(), "--version must exit 0; got {status:?}");
    assert!(
        stdout.contains('.'),
        "version output should contain a dot (semver); got: {stdout}"
    );
}

/// `lightarchitects status` returns sibling binary availability.
#[test]
fn status_command_lists_agents() {
    let (stdout, stderr, _status) = run(&["status"]);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("corso")
            || combined.contains("eva")
            || combined.contains("soul")
            || combined.contains("quantum")
            || combined.contains("seraph")
            || combined.contains("ayin"),
        "status output should mention at least one sibling; got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// `lightarchitects routes` lists enabled agents.
#[test]
fn routes_command_prints_routes() {
    let (stdout, stderr, _status) = run(&["routes"]);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("lightarchitects")
            || combined.contains("agent")
            || combined.contains("route"),
        "routes output should mention routes or agents; got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// `lightarchitects chat <prompt>` (non-interactive) produces output and a plan suggestion.
#[test]
fn chat_noninteractive_produces_plan_suggestion() {
    let (stdout, stderr, status) = run(&["chat", "how do I refactor auth middleware?"]);
    assert!(
        status.success(),
        "chat <prompt> must exit 0; got {status:?}\nstderr: {stderr}"
    );
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("plan") || combined.contains("LASDLC"),
        "chat non-interactive should suggest a LASDLC plan; got:\n{combined}"
    );
}

/// `lightarchitects chat` with no args enters REPL and exits on "quit".
/// We feed "quit" via stdin and assert the process terminates cleanly.
#[test]
fn chat_repl_exits_on_quit() {
    let mut child = Command::new(GATEWAY_BIN)
        .arg("chat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn chat REPL");

    {
        let mut stdin = child.stdin.take().expect("take stdin");
        use std::io::Write;
        writeln!(stdin, "quit").expect("write quit");
        stdin.flush().expect("flush stdin");
    }

    let output = child
        .wait_with_output()
        .expect("wait for REPL output");

    assert!(
        output.status.success(),
        "chat REPL should exit 0 after 'quit'; got {:?}",
        output.status
    );
}

/// `lightarchitects chat` REPL accumulates messages and suggests a plan on "build".
#[test]
fn chat_repl_suggests_plan_on_build_command() {
    let mut child = Command::new(GATEWAY_BIN)
        .arg("chat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn chat REPL");

    {
        let mut stdin = child.stdin.take().expect("take stdin");
        use std::io::Write;
        writeln!(stdin, "I need a login page").expect("write msg1");
        writeln!(stdin, "React + Tailwind").expect("write msg2");
        writeln!(stdin, "build").expect("write build");
        writeln!(stdin, "quit").expect("write quit");
        stdin.flush().expect("flush stdin");
    }

    let output = child
        .wait_with_output()
        .expect("wait for REPL output");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    assert!(
        output.status.success(),
        "chat REPL should exit 0; got {:?}",
        output.status
    );
    assert!(
        stdout.contains("LASDLC Plan") || stdout.contains("Promoted plan"),
        "REPL should print a LASDLC plan after 'build' command; got stdout:\n{stdout}"
    );
}

/// `lightarchitects config` prints resolved configuration.
#[test]
fn config_command_prints_configuration() {
    let (stdout, stderr, _status) = run(&["config"]);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("config") || combined.contains("lightarchitects"),
        "config should print configuration; got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// `lightarchitects builds list` shows the build portfolio header (or empty list).
#[test]
fn builds_list_shows_portfolio_header() {
    let (stdout, stderr, _status) = run(&["builds", "list"]);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("build")
            || combined.contains("portfolio")
            || combined.contains("No builds"),
        "builds list should mention builds or portfolio; got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
