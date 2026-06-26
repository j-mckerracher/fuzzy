//! Phase 7 acceptance tests: polish (resume + redaction reaching transcript).
//!
//! Covers the acceptance criteria:
//! 1. Resume works: with no `--run`, chat binds to the project's active run and
//!    surfaces it (visible via `--print-context`).
//! 2. Secrets never reach the transcript: a credential embedded in a one-shot
//!    turn is stored as `[REDACTED]` in the persisted transcript.

use assert_cmd::Command;
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

/// One chat turn against the mock backend (auto-approve on).
fn turn(dir: &Path, text: &str) -> assert_cmd::assert::Assert {
    fuzzy(dir)
        .args(["chat", "--backend", "mock", "--one-shot", text])
        .assert()
        .success()
}

/// The single active run directory's id (its folder name).
fn active_run_id(dir: &Path) -> String {
    let runs = dir.join("fuzzy-runs");
    std::fs::read_dir(&runs)
        .expect("runs dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .expect("one run dir")
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

/// Concatenate every chat session's transcript on disk.
fn all_transcripts(dir: &Path) -> String {
    let chats = dir.join(".fuzzy").join("chats");
    let mut out = String::new();
    for entry in std::fs::read_dir(&chats).expect("chats dir").flatten() {
        let path = entry.path().join("transcript.jsonl");
        if let Ok(text) = std::fs::read_to_string(&path) {
            out.push_str(&text);
        }
    }
    out
}

#[test]
fn print_context_resumes_active_run() {
    let dir = init_project();
    turn(dir.path(), "create_run for the resume check");
    let run_id = active_run_id(dir.path());

    // No --run flag: chat must resolve and display the active run.
    let assert = fuzzy(dir.path())
        .args([
            "chat",
            "--backend",
            "mock",
            "--no-banner",
            "--print-context",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains(&format!("run: {run_id}")),
        "print-context should resume the active run; got:\n{stdout}"
    );
}

#[test]
fn secrets_never_reach_transcript() {
    let dir = init_project();
    let secret_token = "ghp_ABCDEFGHIJKLMNOPQRST1234";
    let secret_assign = "SUPERSECRETVALUE1234";
    turn(
        dir.path(),
        &format!("store token {secret_token} and api_key={secret_assign} please"),
    );

    let transcript = all_transcripts(dir.path());
    assert!(
        !transcript.contains(secret_token),
        "bare credential leaked into transcript:\n{transcript}"
    );
    assert!(
        !transcript.contains(secret_assign),
        "assigned secret leaked into transcript:\n{transcript}"
    );
    assert!(
        transcript.contains("[REDACTED]"),
        "transcript should mark redactions:\n{transcript}"
    );
}
