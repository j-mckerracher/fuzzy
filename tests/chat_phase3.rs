//! Phase 3 acceptance tests: Ollama backend wiring, config keys, doctor.
//!
//! These tests never require a live Ollama daemon. The reachability test
//! deliberately points the backend at a closed port so the connection is
//! refused deterministically.

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

#[test]
fn config_set_dotted_keys_update_config() {
    let dir = init_project();
    fuzzy(dir.path())
        .args(["config", "set", "agent.default_backend", "ollama"])
        .assert()
        .success();
    fuzzy(dir.path())
        .args(["config", "set", "agent.ollama.model", "gpt-oss:120b"])
        .assert()
        .success();
    fuzzy(dir.path())
        .args(["config", "set", "agent.ollama.direct_cloud_api", "true"])
        .assert()
        .success();

    fuzzy(dir.path())
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "default_agent_backend = \"ollama\"",
        ))
        .stdout(predicate::str::contains("ollama_model = \"gpt-oss:120b\""))
        .stdout(predicate::str::contains("ollama_direct_cloud_api = true"));
}

#[test]
fn config_set_rejects_non_boolean_direct_cloud() {
    let dir = init_project();
    fuzzy(dir.path())
        .args(["config", "set", "agent.ollama.direct_cloud_api", "yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a boolean"));
}

#[test]
fn doctor_reports_ollama_config() {
    let dir = init_project();
    fuzzy(dir.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("ollama:"))
        .stdout(predicate::str::contains("glm-5.2:cloud"))
        .stdout(predicate::str::contains("localhost:11434"));
}

#[test]
fn ollama_unreachable_local_gives_guidance() {
    let dir = init_project();
    // Port 9 (discard) refuses TCP connections deterministically.
    fuzzy(dir.path())
        .args([
            "config",
            "set",
            "agent.ollama.base_url",
            "http://127.0.0.1:9",
        ])
        .assert()
        .success();
    fuzzy(dir.path())
        .args(["chat", "--backend", "ollama", "--one-shot", "hello"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not reachable"));
}

#[test]
fn ollama_direct_cloud_requires_api_key() {
    let dir = init_project();
    fuzzy(dir.path())
        .args(["config", "set", "agent.ollama.direct_cloud_api", "true"])
        .assert()
        .success();
    fuzzy(dir.path())
        .args([
            "config",
            "set",
            "agent.ollama.api_key_env",
            "FUZZY_TEST_NO_SUCH_KEY",
        ])
        .assert()
        .success();
    fuzzy(dir.path())
        .env_remove("FUZZY_TEST_NO_SUCH_KEY")
        .args(["chat", "--backend", "ollama", "--one-shot", "hello"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("needs an API key"));
}
