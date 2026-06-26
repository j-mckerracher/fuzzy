//! Phase 5 acceptance tests: full tool set, permissions, and gates.
//!
//! Covers the acceptance criteria:
//! 1. A troubleshoot chat drives run -> questions -> evidence -> hypotheses ->
//!    diagnosis report through the orchestrator tools.
//! 2. The gate flags unresolved blocking questions (pass == false).
//! 3. A risky tool call prompts for confirmation: denied without approval
//!    (piped EOF), granted when auto-approve is on.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn fuzzy(root: &Path) -> Command {
    let mut cmd = Command::cargo_bin("fuzzy").expect("fuzzy binary builds");
    cmd.arg("--root").arg(root);
    cmd
}

fn init_project() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    fuzzy(dir.path()).arg("init").assert().success();
    dir
}

/// Run one chat turn against the mock backend (auto-approve on by default).
fn turn(dir: &Path, text: &str) -> assert_cmd::assert::Assert {
    fuzzy(dir)
        .args(["chat", "--backend", "mock", "--one-shot", text])
        .assert()
        .success()
}

/// Resolve the single run directory under fuzzy-runs.
fn run_dir(dir: &Path) -> PathBuf {
    let runs = dir.join("fuzzy-runs");
    std::fs::read_dir(&runs)
        .expect("runs dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .expect("one run dir")
}

/// Criterion 1: full troubleshoot flow writes evidence, hypotheses, and a report.
#[test]
fn troubleshoot_flow_writes_diagnosis_report() {
    let dir = init_project();

    turn(dir.path(), "create_run for the failing importer");
    turn(dir.path(), "add_question is the upload retry path blocking");
    turn(
        dir.path(),
        "resolve_question after inspecting the retry path",
    );
    turn(dir.path(), "add_evidence from the importer log tail");
    turn(dir.path(), "add_hypothesis the retry budget is exhausted");
    turn(dir.path(), "write_report the diagnosis summary");

    let rd = run_dir(dir.path());

    let evidence = std::fs::read_to_string(rd.join("evidence_ledger.yaml")).unwrap();
    assert!(
        evidence.contains("E-001"),
        "evidence entry written: {evidence}"
    );

    let hypotheses = std::fs::read_to_string(rd.join("hypothesis_ledger.yaml")).unwrap();
    assert!(
        hypotheses.contains("H-001"),
        "hypothesis entry written: {hypotheses}"
    );

    assert!(
        rd.join("outputs/final_report.md").exists(),
        "diagnosis report written"
    );

    let events = std::fs::read_to_string(rd.join("events.jsonl")).unwrap();
    assert!(
        events.contains("evidence.added"),
        "evidence event: {events}"
    );
    assert!(
        events.contains("hypothesis.added"),
        "hypothesis event: {events}"
    );
}

/// Criterion 2: the gate flags an unresolved blocking question.
#[test]
fn gate_flags_unresolved_blocking_question() {
    let dir = init_project();

    turn(dir.path(), "create_run for the gate check");
    turn(
        dir.path(),
        "add_question is the cache invalidation blocking",
    );
    turn(dir.path(), "validate_gate now").stdout(predicate::str::contains("[tool] validate_gate"));

    let report = std::fs::read_to_string(run_dir(dir.path()).join("outputs/gate_report.json"))
        .expect("gate report written");
    assert!(
        report.contains("\"pass\": false") || report.contains("\"pass\":false"),
        "gate fails with an open blocking question: {report}"
    );
    assert!(
        report.contains("blocking"),
        "gate report names the blocking-question check: {report}"
    );
}

/// Criterion 3a: a risky tool call is denied when approval is required and
/// stdin is closed (piped EOF -> denied).
#[test]
fn risky_call_denied_without_approval() {
    let dir = init_project();
    turn(dir.path(), "create_run for the approval check");

    fuzzy(dir.path())
        .args([
            "chat",
            "--backend",
            "mock",
            "--require-approval",
            "--one-shot",
            "set_permission_level to branch-write",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[blocked]"))
        .stdout(predicate::str::contains("confirmation denied"));

    let events = std::fs::read_to_string(run_dir(dir.path()).join("events.jsonl")).unwrap();
    assert!(
        events.contains("approval.requested"),
        "approval requested event: {events}"
    );
    assert!(
        events.contains("approval.denied"),
        "approval denied event: {events}"
    );
}

/// Criterion 3b: the same risky call is granted when auto-approve is on.
#[test]
fn risky_call_granted_with_auto_approve() {
    let dir = init_project();
    turn(dir.path(), "create_run for the approval grant");

    turn(dir.path(), "set_permission_level to branch-write")
        .stdout(predicate::str::contains("[tool] set_permission_level"));

    let events = std::fs::read_to_string(run_dir(dir.path()).join("events.jsonl")).unwrap();
    assert!(
        events.contains("approval.granted"),
        "approval granted event: {events}"
    );
    assert!(
        events.contains("permission.set"),
        "permission set event: {events}"
    );
}
