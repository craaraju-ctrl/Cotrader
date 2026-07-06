//! # CoTrader — Build Automation (xtask)
//!
//! Replaces Makefiles with pure Rust automation.
//!
//! ## Usage
//! ```bash
//! cargo xtask build          # Build workspace
//! cargo xtask test           # Run all tests
//! cargo xtask ci             # Run full CI pipeline (fmt + clippy + build + test)
//! cargo xtask fmt            # Format all code
//! cargo xtask lint           # Clippy lint
//! cargo xtask docs           # Build documentation
//! cargo xtask clean          # Clean and rebuild
//! cargo xtask release        # Build release binaries
//! ```

use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let task = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    match task {
        "build" => run("cargo", &["build", "--workspace"]),
        "test" => run("cargo", &["test", "--workspace", "--", "--nocapture"]),
        "ci" => {
            run("cargo", &["fmt", "--all", "--", "--check"]);
            run("cargo", &["clippy", "--workspace", "--all-targets"]);
            run("cargo", &["build", "--workspace"]);
            run("cargo", &["test", "--workspace", "--", "--nocapture"]);
        }
        "fmt" => run("cargo", &["fmt", "--all"]),
        "lint" => run("cargo", &["clippy", "--workspace", "--all-targets"]),
        "docs" => run("cargo", &["doc", "--workspace", "--no-deps"]),
        "clean" => {
            run("cargo", &["clean"]);
            run("cargo", &["build", "--workspace"]);
        }
        "release" => run("cargo", &["build", "--release", "--workspace"]),
        "check" => run("cargo", &["check", "--workspace"]),
        "help" | _ => {
            eprintln!("RAT Agent — xtask automation");
            eprintln!("Usage: cargo xtask <task>");
            eprintln!();
            eprintln!("Tasks:");
            eprintln!("  build    Build workspace (debug)");
            eprintln!("  test     Run all tests");
            eprintln!("  ci       Full CI pipeline (fmt → lint → build → test)");
            eprintln!("  fmt      Format all code");
            eprintln!("  lint     Clippy lint");
            eprintln!("  docs     Build documentation");
            eprintln!("  clean    Clean and rebuild");
            eprintln!("  release  Build release binaries");
            eprintln!("  check    Check compilation (fast)");
            std::process::exit(1);
        }
    }
}

fn run(cmd: &str, args: &[&str]) {
    eprintln!("==> {} {}", cmd, args.join(" "));
    let status = Command::new(cmd)
        .args(args)
        .status()
        .expect("failed to execute command");
    if !status.success() {
        eprintln!("❌ Command failed: {} {}", cmd, args.join(" "));
        std::process::exit(status.code().unwrap_or(1));
    }
}
