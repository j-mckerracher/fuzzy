//! Phase 6 acceptance tests: promotions (Workbench / OpenViking / AutoContext).
//!
//! Covers the acceptance criteria:
//! 1. Workbench promotion fails without a valid story.json.
//! 2. OpenViking promotion requires approval: denied under --require-approval
//!    (piped EOF), granted under auto-approve.
//! 3. AutoContext review writes a proposal artifact, and decision-point tool
//!    calls auto-invoke a review proposal.

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

/// Elevate the global default permission so handoff-tier tools are allowed.
fn set_handoff(dir: &Path) {
    fuzzy(dir)
        .args(["config", "set", "default_permission_level", "handoff"])
        .assert()
        .success();
}

/// One chat turn against the mock backend (auto-approve on).
fn turn(dir: &Path, text: &str) -> assert_cmd::assert::Assert {
    fuzzy(dir)
        .args(["chat", "--backend", "mock", "--one-shot", text])
        .assert()
        .success()
}

/// One chat turn that requires confirmation (stdin is piped EOF -> denied).
fn turn_require_approval(dir: &Path, text: &str) -> assert_cmd::assert::Assert {
    fuzzy(dir)
        .args([
            "chat",
            "--backend",
            "mock",
            "--require-approval",
            "--one-shot",
            text,
        ])
        .assert()
        .success()
}

fn run_dir(dir: &Path) -> PathBuf {
    let runs = dir.join("fuzzy-runs");
    std::fs::read_dir(&runs)
        .expect("runs dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .expect("one run dir")
}

fn events(dir: &Path) -> String {
    std::fs::read_to_string(run_dir(dir).join("events.jsonl")).unwrap_or_default()
}

/// Criterion 1: Workbench promotion fails without a valid story.json.
#[test]
fn workbench_promotion_fails_without_story() {
    let dir = init_project();
    set_handoff(dir.path());
    turn(dir.path(), "create_run for the workbench handoff");

    turn(dir.path(), "promote_workbench the converged scope")
        .stdout(predicate::str::contains("[tool-error] promote_workbench"))
        .stdout(predicate::str::contains("story file not found"));

    assert!(
        !run_dir(dir.path())
            .join("handoff/workbench/story.json")
            .exists(),
        "no workbench story staged when source story.json is missing"
    );
}

/// Criterion 2a: OpenViking promotion is denied under --require-approval.
#[test]
fn openviking_promotion_denied_without_approval() {
    let dir = init_project();
    set_handoff(dir.path());
    turn(dir.path(), "create_run for the openviking handoff");

    turn_require_approval(dir.path(), "promote_openviking the final report")
        .stdout(predicate::str::contains("[blocked] promote_openviking"))
        .stdout(predicate::str::contains("confirmation denied"));

    assert!(
        !run_dir(dir.path())
            .join("handoff/openviking/knowledge_candidate.json")
            .exists(),
        "no OpenViking candidate staged when confirmation is denied"
    );
    assert!(
        events(dir.path()).contains("approval.denied"),
        "approval.denied event recorded"
    );
}

/// Criterion 2b: OpenViking promotion succeeds under auto-approve.
#[test]
fn openviking_promotion_granted_with_auto_approve() {
    let dir = init_project();
    set_handoff(dir.path());
    turn(dir.path(), "create_run for the openviking handoff");
    turn(dir.path(), "write_report the investigation summary");

    turn(dir.path(), "promote_openviking the final report")
        .stdout(predicate::str::contains("[tool] promote_openviking"));

    assert!(
        run_dir(dir.path())
            .join("handoff/openviking/knowledge_candidate.json")
            .exists(),
        "OpenViking candidate staged"
    );
    let ev = events(dir.path());
    assert!(ev.contains("approval.granted"), "approval.granted recorded");
    assert!(ev.contains("promote.openviking"), "promote event recorded");
}

/// Criterion 3a: run_autocontext_review writes a proposal artifact.
#[test]
fn autocontext_review_writes_artifact() {
    let dir = init_project();
    turn(dir.path(), "create_run for the autocontext review");

    turn(dir.path(), "run_autocontext_review of this run")
        .stdout(predicate::str::contains("[tool] run_autocontext_review"));

    assert!(
        run_dir(dir.path())
            .join("handoff/autocontext_review.json")
            .exists(),
        "autocontext review proposal written"
    );
    assert!(
        events(dir.path()).contains("autocontext.review"),
        "autocontext.review event recorded"
    );
}

/// Criterion 3b: decision-point tool calls auto-invoke an AutoContext review.
#[test]
fn validate_gate_auto_invokes_autocontext_review() {
    let dir = init_project();
    turn(dir.path(), "create_run for the auto review hook");

    turn(dir.path(), "validate_gate the current state");

    assert!(
        run_dir(dir.path())
            .join("handoff/autocontext_review.json")
            .exists(),
        "auto review proposal written after validate_gate"
    );
    assert!(
        events(dir.path()).contains("autocontext.review.auto"),
        "autocontext.review.auto event recorded"
    );
}
