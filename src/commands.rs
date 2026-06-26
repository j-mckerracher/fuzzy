//! Subcommand handlers. Each delegates state/artifact work to `crate::ops` and
//! is responsible only for printing CLI output and invoking external processes.

use crate::adapters::OpenVikingAdapter;
use crate::models::{Confidence, ExitType, GateReport, HypothesisStatus, WorkMode};
use crate::ops;
use crate::render;
use crate::store::{write_json_pretty, Store};
use crate::util::{ensure_dir, relative_to};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub fn cmd_doctor(store: Option<&Store>) -> Result<()> {
    println!("fuzzy doctor");
    match ops::doctor(store)? {
        Some(p) => {
            println!("project root: {}", p.root.display());
            println!("runs dir: {}", p.runs_dir.display());
            println!("config: {}", p.config_path.display());
            println!(
                "ov binary `{}`: {}",
                p.ov.name,
                if p.ov.found { "found" } else { "not found" }
            );
            println!(
                "pi binary `{}`: {}",
                p.pi.name,
                if p.pi.found { "found" } else { "not found" }
            );
            println!(
                "autoctx binary `{}`: {}",
                p.autoctx.name,
                if p.autoctx.found {
                    "found"
                } else {
                    "not found"
                }
            );
            println!("knowledge mode probe: {:?}", p.knowledge_mode);
            for anomaly in p.anomalies {
                println!("- {}: {}", anomaly.tool, anomaly.anomaly);
            }
            println!(
                "ollama: model `{}` via {} ({})",
                p.ollama.model,
                if p.ollama.direct_cloud_api {
                    "https://ollama.com".to_string()
                } else {
                    p.ollama.base_url.clone()
                },
                if p.ollama.direct_cloud_api {
                    "direct cloud"
                } else {
                    "local"
                }
            );
            if p.ollama.direct_cloud_api {
                println!(
                    "ollama api key (${}): {}",
                    p.ollama.api_key_env,
                    if p.ollama.api_key_present {
                        "present"
                    } else {
                        "MISSING"
                    }
                );
            }
        }
        None => {
            println!("no fuzzy project detected; run `fuzzy init`");
        }
    }
    Ok(())
}

pub fn cmd_config_show(store: &Store) -> Result<()> {
    println!("{}", ops::config_toml(store)?);
    Ok(())
}

pub fn cmd_config_set(store: Store, key: String, value: String) -> Result<()> {
    let path = ops::config_set(store, key, value)?;
    println!("updated config: {}", path.display());
    Ok(())
}

pub fn cmd_start(
    store: &Store,
    mode: WorkMode,
    title: Option<String>,
    request: Vec<String>,
) -> Result<()> {
    let (run, dir) = ops::start(store, mode, title, request)?;
    println!("created run: {}", run.id);
    println!("path: {}", dir.display());
    println!(
        "next: fuzzy librarian ask --run {} \"what do we already know?\"",
        run.id
    );
    Ok(())
}

pub fn cmd_list(store: &Store) -> Result<()> {
    let runs = ops::list(store)?;
    if runs.is_empty() {
        println!("no runs yet");
        return Ok(());
    }
    for run in runs {
        println!(
            "{}\t{:?}\t{:?}\t{}",
            run.id, run.mode, run.status, run.title
        );
    }
    Ok(())
}

pub fn cmd_status(store: &Store, run: Option<String>) -> Result<()> {
    let b = ops::status(store, run)?;
    render::print_run_summary(
        &b.run,
        &b.questions,
        &b.hypotheses,
        &b.evidence,
        &b.decisions,
    );
    Ok(())
}

pub fn cmd_open_path(store: &Store, run: Option<String>) -> Result<()> {
    println!("{}", ops::run_path(store, run)?.display());
    Ok(())
}

pub fn cmd_question_add(
    store: &Store,
    run: Option<String>,
    text: Vec<String>,
    blocking: bool,
    owner: Option<String>,
) -> Result<()> {
    let id = ops::question_add(store, run, text, blocking, owner)?;
    println!("added question {id}");
    Ok(())
}

pub fn cmd_question_list(store: &Store, run: Option<String>) -> Result<()> {
    let doc = ops::question_list(store, run)?;
    if doc.questions.is_empty() {
        println!("no questions recorded");
        return Ok(());
    }
    for q in doc.questions {
        println!(
            "{}\t{:?}\t{}\t{}",
            q.id,
            q.status,
            if q.blocking {
                "blocking"
            } else {
                "nonblocking"
            },
            q.question
        );
    }
    Ok(())
}

pub fn cmd_question_resolve(
    store: &Store,
    run: Option<String>,
    id: String,
    resolution: Vec<String>,
    evidence: Vec<String>,
) -> Result<()> {
    let id = ops::question_resolve(store, run, id, resolution, evidence)?;
    println!("resolved question {id}");
    Ok(())
}

pub fn cmd_hypothesis_add(
    store: &Store,
    run: Option<String>,
    claim: Vec<String>,
    confidence: f32,
    falsification: Option<String>,
) -> Result<()> {
    let id = ops::hypothesis_add(store, run, claim, confidence, falsification)?;
    println!("added hypothesis {id}");
    Ok(())
}

pub fn cmd_hypothesis_update(
    store: &Store,
    run: Option<String>,
    id: String,
    status: Option<HypothesisStatus>,
    confidence: Option<f32>,
    evidence: Vec<String>,
) -> Result<()> {
    let id = ops::hypothesis_update(store, run, id, status, confidence, evidence)?;
    println!("updated hypothesis {id}");
    Ok(())
}

pub fn cmd_hypothesis_list(store: &Store, run: Option<String>) -> Result<()> {
    let doc = ops::hypothesis_list(store, run)?;
    if doc.hypotheses.is_empty() {
        println!("no hypotheses recorded");
        return Ok(());
    }
    for h in doc.hypotheses {
        println!("{}\t{:?}\t{:.2}\t{}", h.id, h.status, h.confidence, h.claim);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_evidence_add(
    store: &Store,
    run: Option<String>,
    claim: Vec<String>,
    source_type: String,
    source: Option<String>,
    confidence: Confidence,
    excerpt: Option<String>,
    file: Option<PathBuf>,
    used_by: Vec<String>,
) -> Result<()> {
    let id = ops::evidence_add(
        store,
        run,
        claim,
        source_type,
        source,
        confidence,
        excerpt,
        file,
        used_by,
    )?;
    println!("added evidence {id}");
    Ok(())
}

pub fn cmd_evidence_list(store: &Store, run: Option<String>) -> Result<()> {
    let doc = ops::evidence_list(store, run)?;
    if doc.evidence.is_empty() {
        println!("no evidence recorded");
        return Ok(());
    }
    for e in doc.evidence {
        println!(
            "{}\t{:?}\t{}\t{}",
            e.id, e.confidence, e.source_type, e.claim
        );
    }
    Ok(())
}

pub fn cmd_decision_add(
    store: &Store,
    run: Option<String>,
    decision: Vec<String>,
    rationale: Option<String>,
    evidence: Vec<String>,
) -> Result<()> {
    let id = ops::decision_add(store, run, decision, rationale, evidence)?;
    println!("added decision {id}");
    Ok(())
}

pub fn cmd_decision_list(store: &Store, run: Option<String>) -> Result<()> {
    let doc = ops::decision_list(store, run)?;
    if doc.decisions.is_empty() {
        println!("no decisions recorded");
        return Ok(());
    }
    for d in doc.decisions {
        println!("{}\t{}", d.id, d.decision);
    }
    Ok(())
}

pub fn cmd_librarian_ask(
    store: &Store,
    run: Option<String>,
    query: Vec<String>,
    force_explorer: bool,
    no_explorer: bool,
) -> Result<()> {
    let r = ops::librarian_ask(store, run, query, force_explorer, no_explorer)?;
    println!("knowledge packet: {}", r.packet_path.display());
    println!("confidence: {:?}", r.packet.confidence);
    println!("summary: {}", r.packet.answer_summary);
    if let Some(path) = r.exploration_report_path {
        println!("explorer report: {}", path.display());
    }
    if r.packet.confidence == Confidence::None {
        println!(
            "next: fuzzy question add --run {} --blocking \"{}\"",
            r.run_id,
            r.packet.query.replace('"', "'")
        );
    }
    Ok(())
}

pub fn cmd_explorer_run(
    store: &Store,
    run: Option<String>,
    query: Vec<String>,
    scope: Option<PathBuf>,
    record_evidence: bool,
) -> Result<()> {
    let r = ops::explorer_run(store, run, query, scope, record_evidence)?;
    println!("explorer report: {}", r.report_path.display());
    println!("confidence: {:?}", r.report.confidence);
    println!("summary: {}", r.report.answer_summary);
    Ok(())
}

pub fn cmd_gate(store: &Store, run: Option<String>) -> Result<GateReport> {
    let outcome = ops::gate(store, run)?;
    let report = outcome.report;
    println!(
        "gate: {} (score {:.2})",
        if report.pass { "PASS" } else { "NEEDS-WORK" },
        report.score
    );
    for check in &report.checks {
        let status = if check.pass {
            "ok"
        } else {
            check.severity.as_str()
        };
        println!("- [{}] {}: {}", status, check.name, check.message);
    }
    println!("report: {}", outcome.path.display());
    Ok(report)
}

pub fn cmd_report(store: &Store, run: Option<String>, out: Option<PathBuf>) -> Result<()> {
    let path = ops::report(store, run, out)?;
    println!("report: {}", path.display());
    Ok(())
}

pub fn cmd_exit(
    store: &Store,
    run: Option<String>,
    exit_type: ExitType,
    note: Vec<String>,
) -> Result<()> {
    let run_id = store.resolve_run_id(run)?;
    let gate = cmd_gate(store, Some(run_id.clone()))?;
    let path = ops::record_exit(store, &run_id, exit_type, note, &gate)?;
    println!("exit recorded: {:?}", exit_type);
    println!("handoff: {}", path.display());
    Ok(())
}

pub fn cmd_promote_workbench(
    store: &Store,
    run: Option<String>,
    story: Option<PathBuf>,
    exec: bool,
) -> Result<()> {
    let run_id = store.resolve_run_id(run)?;
    let story_path = story.unwrap_or_else(|| store.run_dir(&run_id).join("outputs/story.json"));
    if !story_path.exists() {
        bail!("story file not found: {}", story_path.display());
    }
    let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&story_path)?)
        .with_context(|| format!("story file is not valid JSON: {}", story_path.display()))?;
    let dest = store.run_dir(&run_id).join("handoff/workbench/story.json");
    ensure_dir(dest.parent().unwrap())?;
    fs::copy(&story_path, &dest)?;
    write_json_pretty(
        &store
            .run_dir(&run_id)
            .join("handoff/workbench/validation_report.json"),
        &json!({
            "valid_json": true,
            "story_file": relative_to(&dest, &store.run_dir(&run_id)),
            "top_level_type": value_type(&value),
            "validated_at": Utc::now(),
        }),
    )?;
    store.append_event(
        Some(&run_id),
        "promote.workbench",
        json!({"story": relative_to(&dest, &store.run_dir(&run_id)), "exec": exec}),
    )?;
    println!("workbench story copied: {}", dest.display());
    if exec {
        let wb = store.config.workbench_path.clone().ok_or_else(|| {
            anyhow!("set workbench_path with `fuzzy config set workbench_path <path>`")
        })?;
        let run_py = PathBuf::from(wb).join("run.py");
        let status = Command::new("python")
            .arg(run_py)
            .arg("--manual-story-file")
            .arg(&dest)
            .status()?;
        if !status.success() {
            bail!("Agent Workbench returned non-zero status {status}");
        }
    } else {
        println!("dry promotion only. add --exec to invoke configured Agent Workbench.");
    }
    Ok(())
}

pub fn cmd_promote_openviking(
    store: &Store,
    run: Option<String>,
    exec: bool,
    wait: bool,
) -> Result<()> {
    let run_id = store.resolve_run_id(run)?;
    let report = store.run_dir(&run_id).join("outputs/final_report.md");
    if !report.exists() {
        bail!(
            "final report not found; run `fuzzy report --run {}` first",
            run_id
        );
    }
    if exec {
        let ov = OpenVikingAdapter::new(&store.config.ov_binary);
        let ok = ov.add_resource(&report, wait)?;
        if !ok {
            bail!("OpenViking add-resource failed");
        }
        println!("promoted report to OpenViking: {}", report.display());
    } else {
        println!(
            "dry promotion. would run: {} add-resource {}{}",
            store.config.ov_binary,
            report.display(),
            if wait { " --wait" } else { "" }
        );
    }
    store.append_event(
        Some(&run_id),
        "promote.openviking",
        json!({"exec": exec, "report": relative_to(&report, &store.run_dir(&run_id))}),
    )?;
    Ok(())
}

pub fn cmd_promote_autocontext(store: &Store, run: Option<String>, exec: bool) -> Result<()> {
    let run_id = store.resolve_run_id(run)?;
    let payload_path = store
        .run_dir(&run_id)
        .join("handoff/autocontext_payload.json");
    let payload = json!({
        "run_id": run_id,
        "run_dir": store.run_dir(&run_id),
        "final_report": "outputs/final_report.md",
        "events": "events.jsonl",
        "created_at": Utc::now(),
    });
    write_json_pretty(&payload_path, &payload)?;
    if exec {
        let status = Command::new(&store.config.autoctx_binary)
            .arg("ingest")
            .arg("--json")
            .arg(&payload_path)
            .status()
            .with_context(|| format!("running {}", store.config.autoctx_binary))?;
        if !status.success() {
            bail!("AutoContext command returned non-zero status {status}");
        }
    } else {
        println!(
            "dry promotion. AutoContext payload: {}",
            payload_path.display()
        );
    }
    store.append_event(
        Some(&run_id),
        "promote.autocontext",
        json!({"exec": exec, "payload": relative_to(&payload_path, &store.run_dir(&run_id))}),
    )?;
    Ok(())
}

fn value_type(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

pub fn write_example_story(store: &Store, run: Option<String>) -> Result<()> {
    let path = ops::write_example_story(store, run)?;
    println!("wrote example story: {}", path.display());
    Ok(())
}

pub fn cmd_export_run(store: &Store, run: Option<String>, output: Option<PathBuf>) -> Result<()> {
    let run_id = store.resolve_run_id(run)?;
    let out = output.unwrap_or_else(|| PathBuf::from(format!("{}.tar.gz", run_id)));
    let parent = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    ensure_dir(parent)?;
    let status = Command::new("tar")
        .arg("-czf")
        .arg(&out)
        .arg("-C")
        .arg(store.runs_root())
        .arg(&run_id)
        .status()
        .context("running tar")?;
    if !status.success() {
        bail!("tar failed with status {status}");
    }
    println!("exported run: {}", out.display());
    Ok(())
}

pub fn cmd_new_template(store: &Store, run: Option<String>, artifact: String) -> Result<()> {
    let path = ops::new_template(store, run, artifact)?;
    println!("created artifact template: {}", path.display());
    Ok(())
}
