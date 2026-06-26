//! Phase 2 acceptance tests: action envelope + tool runtime + redaction.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;
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

/// First-turn behavior: the orchestrator can create a run and add a question;
/// the runtime writes the artifacts.
#[test]
fn mock_create_run_and_add_question_writes_artifacts() {
    let dir = init_project();
    fuzzy(dir.path())
        .args([
            "chat",
            "--backend",
            "mock",
            "--one-shot",
            "please create_run and add_question for the flaky importer",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[tool] create_run"))
        .stdout(predicate::str::contains("[tool] add_question"));

    // A run directory with a run.yaml must now exist.
    let runs = dir.path().join("fuzzy-runs");
    assert!(runs.exists(), "runs dir created");
    let run_entry = std::fs::read_dir(&runs)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir())
        .expect("one run dir");
    assert!(
        run_entry.path().join("run.yaml").exists(),
        "run.yaml written"
    );

    // The open-questions ledger should hold the new question.
    let questions = run_entry.path().join("open_questions.yaml");
    assert!(questions.exists(), "open_questions.yaml written");
    let body = std::fs::read_to_string(&questions).unwrap();
    assert!(body.contains("Q-001"), "question recorded: {body}");
}

/// The active-run pointer is set after create_run.
#[test]
fn create_run_sets_active_run_pointer() {
    let dir = init_project();
    fuzzy(dir.path())
        .args(["chat", "--backend", "mock", "--one-shot", "create_run now"])
        .assert()
        .success();
    assert!(
        dir.path().join(".fuzzy/current_run").exists(),
        "active run pointer written"
    );
}

/// Assistant-only turn: no trigger tokens → no tool calls, just a message.
#[test]
fn assistant_only_turn_runs_no_tools() {
    let dir = init_project();
    fuzzy(dir.path())
        .args([
            "chat",
            "--backend",
            "mock",
            "--one-shot",
            "what does this harness do",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[mock] received:"))
        .stdout(predicate::str::contains("[tool]").not());
}

/// Unknown tool requests are blocked and logged, not executed.
#[test]
fn unknown_tool_is_blocked() {
    let dir = init_project();
    fuzzy(dir.path())
        .args([
            "chat",
            "--backend",
            "mock",
            "--one-shot",
            "FORCE_UNKNOWN_TOOL please",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[blocked]"))
        .stdout(predicate::str::contains("unknown tool"));
}

/// Malformed model output → repair retries fail → safe fallback, no tools run.
#[test]
fn malformed_json_falls_back() {
    let dir = init_project();
    fuzzy(dir.path())
        .args([
            "chat",
            "--backend",
            "mock",
            "--one-shot",
            "FORCE_BAD_JSON here",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "could not parse the model's action envelope",
        ));
}

/// `fuzzy debug envelope` validates a well-formed envelope file.
#[test]
fn debug_envelope_validates_file() {
    let dir = init_project();
    let path = dir.path().join("env.json");
    std::fs::write(
        &path,
        r#"{"assistant_message":"hi","tool_calls":[{"id":"call-001","name":"load_status","arguments":{}}]}"#,
    )
    .unwrap();
    fuzzy(dir.path())
        .args(["debug", "envelope", "--file"])
        .arg(&path)
        .assert()
        .success()
        .stdout(predicate::str::contains("valid envelope"))
        .stdout(predicate::str::contains("load_status"));
}

/// `fuzzy debug context` prints the orchestrator system prompt with tools.
#[test]
fn debug_context_lists_tools() {
    let dir = init_project();
    fuzzy(dir.path())
        .args(["debug", "context"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fuzzy Action Envelope"))
        .stdout(predicate::str::contains("load_status"))
        .stdout(predicate::str::contains("create_run"));
}

/// Lifecycle events are logged with call ids when tools run.
#[test]
fn tool_lifecycle_events_are_logged() {
    let dir = init_project();
    fuzzy(dir.path())
        .args(["chat", "--backend", "mock", "--one-shot", "create_run go"])
        .assert()
        .success();

    let runs = dir.path().join("fuzzy-runs");
    let run_entry = std::fs::read_dir(&runs)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir())
        .expect("one run dir");
    let events = run_entry.path().join("events.jsonl");
    assert!(events.exists(), "events.jsonl written");
    let body = std::fs::read_to_string(&events).unwrap();
    assert!(
        body.contains("tool_call.executed"),
        "executed event: {body}"
    );
    assert!(body.contains("call-"), "call id recorded: {body}");
}
