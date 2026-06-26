//! Phase 4 acceptance tests: librarian / explorer integration.
//!
//! Covers the four acceptance criteria:
//! 1. A project with flat-file knowledge yields a knowledge packet (KP-).
//! 2. `run_explorer_readonly` writes an evidence packet (EP-) without a durable
//!    evidence-ledger write.
//! 3. `propose_openviking_memory` writes a curation candidate (CC-) under run
//!    artifacts only.
//! 4. No tool lets the orchestrator write durable OpenViking memory directly;
//!    the proposal is a candidate (`status: proposed`), not a durable write.

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

/// Create an active run via the orchestrator so the run pointer is set.
fn create_active_run(dir: &Path) {
    fuzzy(dir)
        .args([
            "chat",
            "--backend",
            "mock",
            "--one-shot",
            "create_run for the librarian phase",
        ])
        .assert()
        .success();
    assert!(
        dir.join(".fuzzy/current_run").exists(),
        "active run pointer written"
    );
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

/// Criterion 1: ask_librarian produces a knowledge packet under the run.
#[test]
fn ask_librarian_writes_knowledge_packet() {
    let dir = init_project();
    create_active_run(dir.path());

    // Seed flat-file knowledge so the librarian has something to read.
    let knowledge = dir.path().join(".fuzzy/knowledge");
    std::fs::create_dir_all(&knowledge).unwrap();
    std::fs::write(
        knowledge.join("importer.md"),
        "The importer retries flaky uploads three times before failing.",
    )
    .unwrap();

    fuzzy(dir.path())
        .args([
            "chat",
            "--backend",
            "mock",
            "--one-shot",
            "ask_librarian about the flaky importer",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[tool] ask_librarian"));

    let packets = run_dir(dir.path()).join("librarian/knowledge_packets");
    let has_packet = std::fs::read_dir(&packets)
        .expect("packets dir")
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().starts_with("KP-"));
    assert!(has_packet, "a KP- knowledge packet was written");

    // The librarian.query event is recorded.
    let events = std::fs::read_to_string(run_dir(dir.path()).join("events.jsonl")).unwrap();
    assert!(
        events.contains("librarian.query"),
        "librarian.query event recorded: {events}"
    );
}

/// Criterion 2: run_explorer_readonly writes an EP report and does NOT write a
/// durable evidence-ledger entry.
#[test]
fn explorer_readonly_writes_evidence_packet_only() {
    let dir = init_project();
    create_active_run(dir.path());

    fuzzy(dir.path())
        .args([
            "chat",
            "--backend",
            "mock",
            "--one-shot",
            "run_explorer_readonly to inspect the repo",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[tool] run_explorer_readonly"));

    let packets = run_dir(dir.path()).join("explorer/evidence_packets");
    let has_ep = std::fs::read_dir(&packets)
        .expect("evidence packets dir")
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().starts_with("EP-"));
    assert!(has_ep, "an EP- evidence packet was written");

    // Read-only: the durable evidence ledger has no recorded evidence.
    let ledger = run_dir(dir.path()).join("evidence_ledger.yaml");
    if ledger.exists() {
        let body = std::fs::read_to_string(&ledger).unwrap();
        assert!(
            !body.contains("EV-001"),
            "no durable evidence entry written: {body}"
        );
    }
}

/// Criteria 3 & 4: propose_openviking_memory writes a CC- candidate under run
/// artifacts with status `proposed` — a candidate, never a durable OV write.
#[test]
fn propose_openviking_writes_candidate_only() {
    let dir = init_project();
    create_active_run(dir.path());

    fuzzy(dir.path())
        .args([
            "chat",
            "--backend",
            "mock",
            "--one-shot",
            "propose_openviking_memory for what we learned",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("[tool] propose_openviking_memory"));

    let candidates = run_dir(dir.path()).join("librarian/curation_candidates");
    let candidate = std::fs::read_dir(&candidates)
        .expect("curation candidates dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().starts_with("CC-"))
                .unwrap_or(false)
        })
        .expect("a CC- candidate was written");

    let body = std::fs::read_to_string(&candidate).unwrap();
    assert!(
        body.contains("\"status\": \"proposed\""),
        "candidate is a proposal: {body}"
    );
    assert!(
        body.contains("\"destination\": \"openviking\""),
        "candidate targets openviking: {body}"
    );

    // The proposal is recorded as an event, not a durable OV write.
    let events = std::fs::read_to_string(run_dir(dir.path()).join("events.jsonl")).unwrap();
    assert!(
        events.contains("openviking.proposed"),
        "openviking.proposed event recorded: {events}"
    );
}
