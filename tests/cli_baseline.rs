//! Phase 0A baseline regression tests.
//!
//! These tests lock the current deterministic CLI behavior BEFORE any refactor,
//! interactive-shell, or backend work. They intentionally assert on stable
//! stdout substrings and on-disk artifacts only, so later internal refactors
//! (Phase 0B+) can be validated against them without churn.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;
use tempfile::TempDir;

/// Build a `fuzzy` command scoped to `root` via the global `--root` flag.
fn fuzzy(root: &Path) -> Command {
    let mut cmd = Command::cargo_bin("fuzzy").expect("fuzzy binary builds");
    cmd.arg("--root").arg(root);
    cmd
}

/// Initialize a project in a fresh temp dir and return it.
fn init_project() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    fuzzy(dir.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("initialized fuzzy harness at"));
    assert!(
        dir.path().join(".fuzzy/config.toml").exists(),
        "config.toml created"
    );
    dir
}

/// Initialize a project and start one troubleshoot run.
fn project_with_run() -> TempDir {
    let dir = init_project();
    fuzzy(dir.path())
        .args([
            "start",
            "--mode",
            "troubleshoot",
            "imports",
            "randomly",
            "fail",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("created run:"));
    dir
}

#[test]
fn init_creates_config() {
    let dir = init_project();
    // Re-init without --force should not blow away anything unexpectedly; the
    // important invariant is the config file remains present.
    assert!(dir.path().join(".fuzzy/config.toml").exists());
}

#[test]
fn init_force_succeeds_on_existing_project() {
    let dir = init_project();
    fuzzy(dir.path())
        .args(["init", "--force"])
        .assert()
        .success();
    assert!(dir.path().join(".fuzzy/config.toml").exists());
}

#[test]
fn config_show_lists_known_keys() {
    let dir = init_project();
    fuzzy(dir.path())
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("default_agent_backend"))
        .stdout(predicate::str::contains("default_agent_backend = \"none\""))
        .stdout(predicate::str::contains("knowledge_backend"))
        .stdout(predicate::str::contains("ov_binary"));
}

#[test]
fn config_set_updates_value() {
    let dir = init_project();
    fuzzy(dir.path())
        .args(["config", "set", "runs_dir", "custom-runs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated config:"));
    let toml = std::fs::read_to_string(dir.path().join(".fuzzy/config.toml")).unwrap();
    assert!(toml.contains("custom-runs"), "new runs_dir persisted");
}

#[test]
fn config_set_unknown_key_fails() {
    let dir = init_project();
    fuzzy(dir.path())
        .args(["config", "set", "not_a_real_key", "x"])
        .assert()
        .failure();
}

#[test]
fn start_creates_run_artifacts() {
    let dir = project_with_run();
    let runs_dir = dir.path().join("fuzzy-runs");
    assert!(runs_dir.exists(), "fuzzy-runs created");
    let run = std::fs::read_dir(&runs_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .next()
        .expect("one run dir");
    let run_path = run.path();
    assert!(run_path.join("run.yaml").exists(), "run.yaml");
    assert!(
        run_path.join("open_questions.yaml").exists(),
        "open_questions.yaml"
    );
    assert!(
        run_path.join("hypothesis_ledger.yaml").exists(),
        "hypothesis_ledger.yaml"
    );
    assert!(
        run_path.join("evidence_ledger.yaml").exists(),
        "evidence_ledger.yaml"
    );
}

#[test]
fn list_shows_started_run() {
    let dir = project_with_run();
    fuzzy(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Troubleshoot"));
}

#[test]
fn list_empty_when_no_runs() {
    let dir = init_project();
    fuzzy(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("no runs yet"));
}

#[test]
fn status_reports_run_summary() {
    let dir = project_with_run();
    fuzzy(dir.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Mode: Troubleshoot"))
        .stdout(predicate::str::contains("Open questions:"))
        .stdout(predicate::str::contains("Evidence: 0"));
}

#[test]
fn question_add_list_resolve_cycle() {
    let dir = project_with_run();
    fuzzy(dir.path())
        .args([
            "question",
            "add",
            "--blocking",
            "when",
            "did",
            "this",
            "start",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("added question Q-001"));
    fuzzy(dir.path())
        .args(["question", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Q-001"))
        .stdout(predicate::str::contains("blocking"));
    fuzzy(dir.path())
        .args([
            "question", "resolve", "Q-001", "happened", "after", "deploy",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("resolved question Q-001"));
}

#[test]
fn question_list_empty() {
    let dir = project_with_run();
    fuzzy(dir.path())
        .args(["question", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no questions recorded"));
}

#[test]
fn hypothesis_add_and_list() {
    let dir = project_with_run();
    fuzzy(dir.path())
        .args(["hypothesis", "add", "stale", "import", "cache"])
        .assert()
        .success()
        .stdout(predicate::str::contains("added hypothesis H-001"));
    fuzzy(dir.path())
        .args(["hypothesis", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("H-001"));
}

#[test]
fn evidence_add_and_list() {
    let dir = project_with_run();
    fuzzy(dir.path())
        .args(["evidence", "add", "deploy", "log", "shows", "cache", "miss"])
        .assert()
        .success()
        .stdout(predicate::str::contains("added evidence E-001"));
    fuzzy(dir.path())
        .args(["evidence", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("E-001"));
}

#[test]
fn decision_add_and_list() {
    let dir = project_with_run();
    fuzzy(dir.path())
        .args(["decision", "add", "roll", "back", "deploy"])
        .assert()
        .success()
        .stdout(predicate::str::contains("added decision D-001"));
    fuzzy(dir.path())
        .args(["decision", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("D-001"));
}

#[test]
fn gate_blocks_on_open_blocking_question() {
    let dir = project_with_run();
    fuzzy(dir.path())
        .args(["question", "add", "--blocking", "unknown", "root", "cause"])
        .assert()
        .success();
    fuzzy(dir.path())
        .arg("gate")
        .assert()
        .success()
        .stdout(predicate::str::contains("NEEDS-WORK"))
        .stdout(predicate::str::contains("blocking questions"));
    let runs_dir = dir.path().join("fuzzy-runs");
    let run = std::fs::read_dir(&runs_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .next()
        .unwrap();
    assert!(
        run.path().join("outputs/gate_report.json").exists(),
        "gate report written"
    );
}

#[test]
fn report_writes_markdown() {
    let dir = project_with_run();
    fuzzy(dir.path())
        .arg("report")
        .assert()
        .success()
        .stdout(predicate::str::contains("report:"));
    let runs_dir = dir.path().join("fuzzy-runs");
    let run = std::fs::read_dir(&runs_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .next()
        .unwrap();
    let report = run.path().join("outputs/final_report.md");
    assert!(report.exists(), "final_report.md written");
    let body = std::fs::read_to_string(report).unwrap();
    assert!(body.contains("# Fuzzy Run Report"), "report header present");
}

#[test]
fn doctor_without_project_reports_no_project() {
    let dir = TempDir::new().unwrap();
    // No --root project here; point at an empty dir so no .fuzzy is found.
    Command::cargo_bin("fuzzy")
        .unwrap()
        .current_dir(dir.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("no fuzzy project detected"));
}

#[test]
fn doctor_with_project_reports_root() {
    let dir = init_project();
    fuzzy(dir.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("project root:"));
}
