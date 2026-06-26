//! Phase 1 interactive-shell acceptance tests.
//!
//! Covers banner gating, the deterministic mock backend one-shot turn, and the
//! guarantee that deterministic subcommands never print the owl.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;
use tempfile::TempDir;

/// A distinctive glyph that only appears in the owl banner art.
const OWL_GLYPH: &str = "⣿";

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

#[test]
fn dry_start_banner_always_prints_owl() {
    let dir = TempDir::new().unwrap();
    fuzzy(dir.path())
        .args(["chat", "--dry-start", "--banner", "always"])
        .assert()
        .success()
        .stdout(predicate::str::contains(OWL_GLYPH))
        .stdout(predicate::str::contains("[dry-start]"));
}

#[test]
fn dry_start_no_banner_suppresses_owl() {
    let dir = TempDir::new().unwrap();
    fuzzy(dir.path())
        .args(["chat", "--dry-start", "--banner", "always", "--no-banner"])
        .assert()
        .success()
        .stdout(predicate::str::contains(OWL_GLYPH).not())
        .stdout(predicate::str::contains("[dry-start]"));
}

#[test]
fn dry_start_auto_no_owl_when_piped() {
    // assert_cmd pipes stdout (not a TTY), so Auto must not print the banner.
    let dir = TempDir::new().unwrap();
    fuzzy(dir.path())
        .args(["chat", "--dry-start"])
        .assert()
        .success()
        .stdout(predicate::str::contains(OWL_GLYPH).not());
}

#[test]
fn gate_json_never_prints_owl() {
    let dir = init_project();
    fuzzy(dir.path())
        .args(["start", "--mode", "troubleshoot", "imports", "fail"])
        .assert()
        .success();
    fuzzy(dir.path())
        .args(["gate", "--json"])
        .assert()
        .stdout(predicate::str::contains(OWL_GLYPH).not());
}

#[test]
fn one_shot_mock_runs_one_turn() {
    let dir = init_project();
    fuzzy(dir.path())
        .args(["chat", "--backend", "mock", "--one-shot", "hello there"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[mock] received: hello there"));

    // A chat session directory with a transcript should now exist.
    let chats = dir.path().join(".fuzzy/chats");
    assert!(chats.exists(), "chats dir created");
    let session = std::fs::read_dir(&chats)
        .unwrap()
        .filter_map(|e| e.ok())
        .next()
        .expect("one chat session dir");
    assert!(
        session.path().join("transcript.jsonl").exists(),
        "transcript written"
    );
}

#[test]
fn one_shot_none_backend_errors() {
    let dir = init_project();
    fuzzy(dir.path())
        .args(["chat", "--backend", "none", "--one-shot", "hi"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no agent backend configured"));
}

#[test]
fn bare_fuzzy_shows_help() {
    Command::cargo_bin("fuzzy")
        .unwrap()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}
