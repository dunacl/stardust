//! Runs the `sg-cli` binary end to end.

use std::path::PathBuf;
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sg_cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sg-cli"))
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("sg-cli runs")
}

/// M1 acceptance: the Rust Classic tier reproduces every golden system of
/// the C reference. Requires the canonical platform's arithmetic for the
/// per-seed part; elsewhere run `compare-golden --statistical-only`.
#[test]
fn classic_matches_the_golden_reference() {
    let per_seed_platform = cfg!(all(target_os = "macos", target_arch = "aarch64"));
    let mut args = vec!["compare-golden"];
    if !per_seed_platform {
        args.push("--statistical-only");
    }
    let output = sg_cli(&args);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("statistical agreement: PASS"), "{stdout}");
}

#[test]
fn simulate_prints_a_summary_and_json() {
    let output = sg_cli(&["simulate", "--seed", "42", "--fidelity", "classic"]);
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Star: G2 1.00 Msun"), "{text}");
    assert!(text.contains("16 bodies"), "{text}");

    let output = sg_cli(&["simulate", "--seed", "42", "--json"]);
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(json["snapshot"]["bodies"].as_array().unwrap().len(), 16);
    assert_eq!(json["diagnostics"]["bit_deterministic"], true);
    assert!(json["timeline"]["events"].as_array().unwrap().len() > 16);
}

#[test]
fn hash_is_reproducible_and_seed_dependent() {
    let hash = |seed: &str| {
        let output = sg_cli(&["hash", "--seed", seed]);
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    assert_eq!(hash("42"), hash("42"));
    assert_ne!(hash("42"), hash("43"));
    assert_eq!(hash("42").len(), 16);
}

#[test]
fn batch_writes_one_snapshot_per_line() {
    let out = std::env::temp_dir().join(format!("sg-cli-batch-{}.jsonl", std::process::id()));
    let output = sg_cli(&[
        "batch",
        "--first-seed",
        "1",
        "--count",
        "5",
        "--mass",
        "0.8",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = std::fs::read_to_string(&out).unwrap();
    std::fs::remove_file(&out).ok();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 5);
    for line in lines {
        let snapshot: sg_schema::SystemSnapshot = serde_json::from_str(line).unwrap();
        assert!(!snapshot.bodies.is_empty());
    }
}

#[test]
fn errors_are_reported_with_a_failing_exit_code() {
    let output = sg_cli(&["simulate", "--seed", "0"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("seed 0 is reserved"));

    let output = sg_cli(&["simulate", "--fidelity", "standard"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("M4"));
}
