//! Side-effecting command logic, separated from presentation.
//!
//! Each function performs the state/artifact work a subcommand needs and returns
//! structured data. The `cmd_*` handlers in `commands.rs` call these and print.
//! Future tool implementations call these ops directly instead of `cmd_*`.

use crate::adapters::{
    flat_file_knowledge_search, knowledge_packet_from_sources, repository_explore,
    OpenVikingAdapter,
};
use crate::models::*;
use crate::render;
use crate::store::{read_json_if_exists, write_json_pretty, write_toml, write_yaml, Store};
use crate::util::{command_exists, relative_to, run_id, slugify, write_string};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

// ---------- structured results ----------

pub struct BinaryProbe {
    pub name: String,
    pub found: bool,
}

pub struct DoctorProject {
    pub root: PathBuf,
    pub runs_dir: PathBuf,
    pub config_path: PathBuf,
    pub ov: BinaryProbe,
    pub pi: BinaryProbe,
    pub autoctx: BinaryProbe,
    pub knowledge_mode: KnowledgeMode,
    pub anomalies: Vec<ToolAnomaly>,
    pub ollama: OllamaDoctor,
}

pub struct OllamaDoctor {
    pub base_url: String,
    pub model: String,
    pub direct_cloud_api: bool,
    pub api_key_env: String,
    pub api_key_present: bool,
}

pub struct StatusBundle {
    pub run: RunDoc,
    pub questions: OpenQuestionsDoc,
    pub hypotheses: HypothesisLedgerDoc,
    pub evidence: EvidenceLedgerDoc,
    pub decisions: DecisionLogDoc,
}

pub struct LibrarianResult {
    pub run_id: String,
    pub packet: KnowledgePacket,
    pub packet_path: PathBuf,
    pub exploration_report_path: Option<PathBuf>,
}

pub struct ExplorerResult {
    pub report: ExplorationReport,
    pub report_path: PathBuf,
}

pub struct GateOutcome {
    pub report: GateReport,
    pub path: PathBuf,
}

pub struct CurationCandidate {
    pub id: String,
    pub path: PathBuf,
}

// ---------- doctor / config ----------

pub fn doctor(store: Option<&Store>) -> Result<Option<DoctorProject>> {
    let Some(store) = store else {
        return Ok(None);
    };
    let ov = OpenVikingAdapter::new(&store.config.ov_binary);
    let (mode, anomalies) = ov.detect_mode();
    Ok(Some(DoctorProject {
        root: store.root.clone(),
        runs_dir: store.runs_root(),
        config_path: store.config_path(),
        ov: BinaryProbe {
            found: command_exists(&store.config.ov_binary),
            name: store.config.ov_binary.clone(),
        },
        pi: BinaryProbe {
            found: command_exists(&store.config.pi_binary),
            name: store.config.pi_binary.clone(),
        },
        autoctx: BinaryProbe {
            found: command_exists(&store.config.autoctx_binary),
            name: store.config.autoctx_binary.clone(),
        },
        knowledge_mode: mode,
        anomalies,
        ollama: OllamaDoctor {
            base_url: store.config.ollama_base_url.clone(),
            model: store.config.ollama_model.clone(),
            direct_cloud_api: store.config.ollama_direct_cloud_api,
            api_key_env: store.config.ollama_api_key_env.clone(),
            api_key_present: std::env::var(&store.config.ollama_api_key_env)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false),
        },
    }))
}

pub fn config_toml(store: &Store) -> Result<String> {
    Ok(toml::to_string_pretty(&store.config)?)
}

pub fn config_set(mut store: Store, key: String, value: String) -> Result<PathBuf> {
    match key.as_str() {
        "runs_dir" => store.config.runs_dir = value,
        "default_agent_backend" | "agent.default_backend" => {
            store.config.default_agent_backend = value
        }
        "knowledge_backend" | "knowledge.backend" => store.config.knowledge_backend = value,
        "ov_binary" | "knowledge.ov_binary" => store.config.ov_binary = value,
        "pi_binary" | "agent.pi.binary" => store.config.pi_binary = value,
        "autoctx_binary" | "agent.autoctx.binary" => store.config.autoctx_binary = value,
        "ollama_base_url" | "agent.ollama.base_url" => store.config.ollama_base_url = value,
        "ollama_model" | "agent.ollama.model" => store.config.ollama_model = value,
        "ollama_direct_cloud_api" | "agent.ollama.direct_cloud_api" => {
            store.config.ollama_direct_cloud_api = value
                .parse()
                .with_context(|| format!("`{value}` is not a boolean (use true/false)"))?;
        }
        "ollama_api_key_env" | "agent.ollama.api_key_env" => {
            store.config.ollama_api_key_env = value
        }
        "workbench_path" => {
            store.config.workbench_path = if value.trim().is_empty() {
                None
            } else {
                Some(value)
            }
        }
        "default_permission_level" => {
            store.config.default_permission_level = parse_permission(&value)?;
        }
        other => bail!("unknown config key `{other}`"),
    }
    let path = store.config_path();
    write_toml(&path, &store.config)?;
    Ok(path)
}

pub fn parse_permission(value: &str) -> Result<PermissionLevel> {
    match value {
        "observe-only" => Ok(PermissionLevel::ObserveOnly),
        "scratch-only" => Ok(PermissionLevel::ScratchOnly),
        "branch-write" => Ok(PermissionLevel::BranchWrite),
        "repo-write" => Ok(PermissionLevel::RepoWrite),
        "remediate" => Ok(PermissionLevel::Remediate),
        "handoff" => Ok(PermissionLevel::Handoff),
        _ => bail!("unknown permission level `{value}`"),
    }
}

// ---------- run lifecycle ----------

pub fn start(
    store: &Store,
    mode: WorkMode,
    title: Option<String>,
    request: Vec<String>,
) -> Result<(RunDoc, PathBuf)> {
    let raw = request.join(" ").trim().to_string();
    if raw.is_empty() {
        bail!("request text is required");
    }
    let short = title.clone().unwrap_or_else(|| raw.clone());
    let id = format!("{}-{}", run_id(), slugify(&short, 32));
    let now = Utc::now();
    let policies = RunPolicies {
        permission_level: store.config.default_permission_level,
        ..RunPolicies::default()
    };
    let run = RunDoc {
        schema_version: "fuzzy.run.v1".into(),
        id: id.clone(),
        title: title.unwrap_or_else(|| short.chars().take(96).collect()),
        mode,
        status: RunStatus::Active,
        created_at: now,
        updated_at: now,
        request: WorkRequest {
            raw: raw.clone(),
            normalized_goal: format!(
                "Turn this uncertain request into a responsible next state: {raw}"
            ),
        },
        uncertainty: Uncertainty::default(),
        policies,
        outputs: OutputExpectations {
            expected: mode.expected_outputs(),
            optional: vec![
                "story.json".into(),
                "workbench_handoff".into(),
                "openviking_knowledge_candidate".into(),
            ],
        },
        integrations: Integrations {
            knowledge_backend: store.config.knowledge_backend.clone(),
            agent_backend: store.config.default_agent_backend.clone(),
            downstreams: vec![
                "workbench".into(),
                "openviking".into(),
                "autocontext".into(),
            ],
        },
    };
    let dir = store.create_run(&run)?;
    Ok((run, dir))
}

pub fn list(store: &Store) -> Result<Vec<RunDoc>> {
    store.list_runs()
}

pub fn status(store: &Store, run: Option<String>) -> Result<StatusBundle> {
    let run_id = store.resolve_run_id(run)?;
    Ok(StatusBundle {
        run: store.load_run(&run_id)?,
        questions: store.load_questions(&run_id)?,
        hypotheses: store.load_hypotheses(&run_id)?,
        evidence: store.load_evidence(&run_id)?,
        decisions: store.load_decisions(&run_id)?,
    })
}

pub fn run_path(store: &Store, run: Option<String>) -> Result<PathBuf> {
    let run_id = store.resolve_run_id(run)?;
    Ok(store.run_dir(&run_id))
}

// ---------- questions ----------

pub fn question_add(
    store: &Store,
    run: Option<String>,
    text: Vec<String>,
    blocking: bool,
    owner: Option<String>,
) -> Result<String> {
    let run_id = store.resolve_run_id(run)?;
    let question_text = text.join(" ").trim().to_string();
    if question_text.is_empty() {
        bail!("question text is required");
    }
    let mut doc = store.load_questions(&run_id)?;
    let q = Question {
        id: store.next_id(&run_id, "Q", doc.questions.len()),
        question: question_text,
        owner,
        blocking,
        status: QuestionStatus::Open,
        created_at: Utc::now(),
        resolved_at: None,
        hypotheses: Vec::new(),
        evidence: Vec::new(),
        resolution: None,
        notes: Vec::new(),
    };
    let id = q.id.clone();
    doc.questions.push(q);
    store.save_questions(&run_id, &doc)?;
    store.append_event(
        Some(&run_id),
        "question.added",
        json!({"id": id, "blocking": blocking}),
    )?;
    Ok(id)
}

pub fn question_list(store: &Store, run: Option<String>) -> Result<OpenQuestionsDoc> {
    let run_id = store.resolve_run_id(run)?;
    store.load_questions(&run_id)
}

pub fn question_resolve(
    store: &Store,
    run: Option<String>,
    id: String,
    resolution: Vec<String>,
    evidence: Vec<String>,
) -> Result<String> {
    let run_id = store.resolve_run_id(run)?;
    let mut doc = store.load_questions(&run_id)?;
    let q = doc
        .questions
        .iter_mut()
        .find(|q| q.id == id)
        .ok_or_else(|| anyhow!("question `{id}` not found"))?;
    q.status = QuestionStatus::Resolved;
    q.resolved_at = Some(Utc::now());
    let resolution_text = resolution.join(" ").trim().to_string();
    q.resolution = if resolution_text.is_empty() {
        Some("resolved".into())
    } else {
        Some(resolution_text)
    };
    q.evidence.extend(evidence);
    store.save_questions(&run_id, &doc)?;
    store.append_event(Some(&run_id), "question.resolved", json!({"id": id}))?;
    Ok(id)
}

// ---------- hypotheses ----------

pub fn hypothesis_add(
    store: &Store,
    run: Option<String>,
    claim: Vec<String>,
    confidence: f32,
    falsification: Option<String>,
) -> Result<String> {
    let run_id = store.resolve_run_id(run)?;
    let claim = claim.join(" ").trim().to_string();
    if claim.is_empty() {
        bail!("hypothesis claim is required");
    }
    let mut doc = store.load_hypotheses(&run_id)?;
    let id = store.next_id(&run_id, "H", doc.hypotheses.len());
    let now = Utc::now();
    doc.hypotheses.push(Hypothesis {
        id: id.clone(),
        claim,
        status: HypothesisStatus::Open,
        confidence,
        evidence: Vec::new(),
        falsification,
        created_at: now,
        updated_at: now,
    });
    store.save_hypotheses(&run_id, &doc)?;
    store.append_event(Some(&run_id), "hypothesis.added", json!({"id": id}))?;
    Ok(id)
}

pub fn hypothesis_update(
    store: &Store,
    run: Option<String>,
    id: String,
    status: Option<HypothesisStatus>,
    confidence: Option<f32>,
    evidence: Vec<String>,
) -> Result<String> {
    let run_id = store.resolve_run_id(run)?;
    let mut doc = store.load_hypotheses(&run_id)?;
    let h = doc
        .hypotheses
        .iter_mut()
        .find(|h| h.id == id)
        .ok_or_else(|| anyhow!("hypothesis `{id}` not found"))?;
    if let Some(status) = status {
        h.status = status;
    }
    if let Some(confidence) = confidence {
        h.confidence = confidence;
    }
    h.evidence.extend(evidence);
    h.updated_at = Utc::now();
    store.save_hypotheses(&run_id, &doc)?;
    store.append_event(Some(&run_id), "hypothesis.updated", json!({"id": id}))?;
    Ok(id)
}

pub fn hypothesis_list(store: &Store, run: Option<String>) -> Result<HypothesisLedgerDoc> {
    let run_id = store.resolve_run_id(run)?;
    store.load_hypotheses(&run_id)
}

// ---------- evidence ----------

#[allow(clippy::too_many_arguments)]
pub fn evidence_add(
    store: &Store,
    run: Option<String>,
    claim: Vec<String>,
    source_type: String,
    source: Option<String>,
    confidence: Confidence,
    excerpt: Option<String>,
    file: Option<PathBuf>,
    used_by: Vec<String>,
) -> Result<String> {
    let run_id = store.resolve_run_id(run)?;
    let claim = claim.join(" ").trim().to_string();
    if claim.is_empty() {
        bail!("evidence claim is required");
    }
    let mut doc = store.load_evidence(&run_id)?;
    let id = store.next_id(&run_id, "E", doc.evidence.len());
    let mut source_value = source.unwrap_or_else(|| "manual".into());
    let mut file_copy = None;
    if let Some(file) = file {
        let name = file
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("evidence-file");
        let dest = store
            .run_dir(&run_id)
            .join("evidence/files")
            .join(format!("{}_{}", id, name));
        fs::copy(&file, &dest)
            .with_context(|| format!("copying evidence file {}", file.display()))?;
        source_value = file.to_string_lossy().to_string();
        file_copy = Some(relative_to(&dest, &store.run_dir(&run_id)));
    }
    doc.evidence.push(Evidence {
        id: id.clone(),
        claim,
        source_type,
        source: source_value,
        confidence,
        excerpt,
        file_copy,
        used_by,
        created_at: Utc::now(),
    });
    store.save_evidence(&run_id, &doc)?;
    store.append_event(Some(&run_id), "evidence.added", json!({"id": id}))?;
    Ok(id)
}

pub fn evidence_list(store: &Store, run: Option<String>) -> Result<EvidenceLedgerDoc> {
    let run_id = store.resolve_run_id(run)?;
    store.load_evidence(&run_id)
}

// ---------- decisions ----------

pub fn decision_add(
    store: &Store,
    run: Option<String>,
    decision: Vec<String>,
    rationale: Option<String>,
    evidence: Vec<String>,
) -> Result<String> {
    let run_id = store.resolve_run_id(run)?;
    let decision = decision.join(" ").trim().to_string();
    if decision.is_empty() {
        bail!("decision text is required");
    }
    let mut doc = store.load_decisions(&run_id)?;
    let id = store.next_id(&run_id, "D", doc.decisions.len());
    doc.decisions.push(Decision {
        id: id.clone(),
        decision,
        rationale,
        evidence,
        created_at: Utc::now(),
        superseded_by: None,
    });
    store.save_decisions(&run_id, &doc)?;
    store.append_event(Some(&run_id), "decision.added", json!({"id": id}))?;
    Ok(id)
}

pub fn decision_list(store: &Store, run: Option<String>) -> Result<DecisionLogDoc> {
    let run_id = store.resolve_run_id(run)?;
    store.load_decisions(&run_id)
}

// ---------- librarian / explorer ----------

pub fn librarian_ask(
    store: &Store,
    run: Option<String>,
    query: Vec<String>,
    force_explorer: bool,
    no_explorer: bool,
) -> Result<LibrarianResult> {
    let run_id = store.resolve_run_id(run)?;
    let query = query.join(" ").trim().to_string();
    if query.is_empty() {
        bail!("query text is required");
    }

    let kp_count = count_files(
        store.run_dir(&run_id).join("librarian/knowledge_packets"),
        "KP-",
    )?;
    let packet_id = format!("KP-{:03}", kp_count + 1);

    let ov = OpenVikingAdapter::new(&store.config.ov_binary);
    let mut tool_anomalies = Vec::new();
    let mut mode = KnowledgeMode::FlatFile;
    let mut sources = Vec::new();
    if store.config.knowledge_backend == "openviking" {
        let (detected, anomalies) = ov.detect_mode();
        mode = detected;
        tool_anomalies = anomalies;
        if mode == KnowledgeMode::OpenViking {
            sources = ov.find(&query, 5).unwrap_or_default();
        }
    }
    if sources.is_empty() {
        mode = KnowledgeMode::FlatFile;
        sources = flat_file_knowledge_search(&store.root, &query, 5)?;
    }

    let needs_explorer = force_explorer || (!no_explorer && sources.len() < 2);
    let mut exploration_report_path = None;
    if needs_explorer {
        let mut report = repository_explore(&store.root, &query, None, 8)?;
        report.knowledge_mode = mode;
        report
            .metacognitive_context
            .tool_anomalies
            .extend(tool_anomalies.clone());
        let path = store.write_exploration_report(&run_id, &report)?;
        exploration_report_path = Some(path);
        sources.extend(report.evidence.clone());
    }

    let mut packet = knowledge_packet_from_sources(packet_id.clone(), &query, mode, sources);
    if !tool_anomalies.is_empty() {
        packet.next_actions.push(
            "OpenViking probe had anomalies; see explorer metacognitive_context/tool_anomalies."
                .into(),
        );
    }
    let packet_path = store.write_knowledge_packet(&run_id, &packet)?;
    store.append_event(
        Some(&run_id),
        "librarian.query",
        json!({
            "packet_id": packet_id,
            "confidence": packet.confidence,
            "knowledge_mode": packet.knowledge_mode,
            "query": query,
            "explorer_used": exploration_report_path.is_some(),
        }),
    )?;

    Ok(LibrarianResult {
        run_id,
        packet,
        packet_path,
        exploration_report_path,
    })
}

pub fn explorer_run(
    store: &Store,
    run: Option<String>,
    query: Vec<String>,
    scope: Option<PathBuf>,
    record_evidence: bool,
) -> Result<ExplorerResult> {
    let run_id = store.resolve_run_id(run)?;
    let query = query.join(" ").trim().to_string();
    if query.is_empty() {
        bail!("query text is required");
    }
    let report = repository_explore(&store.root, &query, scope, 10)?;
    let report_path = store.write_exploration_report(&run_id, &report)?;
    if record_evidence {
        let mut ledger = store.load_evidence(&run_id)?;
        for source in &report.evidence {
            let id = store.next_id(&run_id, "E", ledger.evidence.len());
            ledger.evidence.push(Evidence {
                id,
                claim: format!("Explorer found relevant evidence for: {query}"),
                source_type: source.source_type.clone(),
                source: source.source.clone(),
                confidence: report.confidence,
                excerpt: Some(source.excerpt.clone()),
                file_copy: None,
                used_by: vec![report.exploration_id.clone()],
                created_at: Utc::now(),
            });
        }
        store.save_evidence(&run_id, &ledger)?;
    }
    store.append_event(
        Some(&run_id),
        "explorer.run",
        json!({"exploration_id": report.exploration_id, "query": query}),
    )?;
    Ok(ExplorerResult {
        report,
        report_path,
    })
}

/// Write an OpenViking curation candidate under the run's artifacts only.
///
/// This never writes to durable OpenViking memory; it records a proposal that a
/// later `promote openviking` / approved tool can act on (spec §19.6).
pub fn propose_openviking_memory(
    store: &Store,
    run: Option<String>,
    title: String,
    body: String,
    source_packet_id: Option<String>,
    sources: Vec<String>,
) -> Result<CurationCandidate> {
    let run_id = store.resolve_run_id(run)?;
    if title.trim().is_empty() || body.trim().is_empty() {
        bail!("title and body are required");
    }
    let dir = store.run_dir(&run_id).join("librarian/curation_candidates");
    let id = format!("CC-{:03}", count_files(dir.clone(), "CC-")? + 1);
    let doc = json!({
        "id": id,
        "run_id": run_id,
        "title": title,
        "body": body,
        "source_packet_id": source_packet_id,
        "sources": sources,
        "status": "proposed",
        "destination": "openviking",
        "created_at": Utc::now(),
    });
    let path = dir.join(format!("{id}.json"));
    write_json_pretty(&path, &doc)?;
    store.append_event(
        Some(&run_id),
        "openviking.proposed",
        json!({"candidate_id": id, "source_packet_id": source_packet_id}),
    )?;
    Ok(CurationCandidate { id, path })
}

// ---------- Phase 5: risks / constraints / non-goals / option matrix ----------

pub fn risk_add(
    store: &Store,
    run: Option<String>,
    risk: Vec<String>,
    severity: String,
    mitigation: Option<String>,
) -> Result<String> {
    let run_id = store.resolve_run_id(run)?;
    let risk = risk.join(" ").trim().to_string();
    if risk.is_empty() {
        bail!("risk text is required");
    }
    let mut doc = store.load_risks(&run_id)?;
    let id = store.next_id(&run_id, "R", doc.risks.len());
    doc.risks.push(RiskItem {
        id: id.clone(),
        risk,
        severity,
        mitigation,
        created_at: Utc::now(),
    });
    store.save_risks(&run_id, &doc)?;
    store.append_event(Some(&run_id), "risk.added", json!({"id": id}))?;
    Ok(id)
}

pub fn constraint_add(
    store: &Store,
    run: Option<String>,
    constraint: Vec<String>,
) -> Result<String> {
    let run_id = store.resolve_run_id(run)?;
    let constraint = constraint.join(" ").trim().to_string();
    if constraint.is_empty() {
        bail!("constraint text is required");
    }
    let mut doc = store.load_constraints(&run_id)?;
    let id = store.next_id(&run_id, "C", doc.constraints.len());
    doc.constraints.push(ConstraintItem {
        id: id.clone(),
        constraint,
        created_at: Utc::now(),
    });
    store.save_constraints(&run_id, &doc)?;
    store.append_event(Some(&run_id), "constraint.added", json!({"id": id}))?;
    Ok(id)
}

pub fn non_goal_add(store: &Store, run: Option<String>, non_goal: Vec<String>) -> Result<String> {
    let run_id = store.resolve_run_id(run)?;
    let non_goal = non_goal.join(" ").trim().to_string();
    if non_goal.is_empty() {
        bail!("non-goal text is required");
    }
    let mut doc = store.load_non_goals(&run_id)?;
    let id = store.next_id(&run_id, "NG", doc.non_goals.len());
    doc.non_goals.push(NonGoalItem {
        id: id.clone(),
        non_goal,
        created_at: Utc::now(),
    });
    store.save_non_goals(&run_id, &doc)?;
    store.append_event(Some(&run_id), "non_goal.added", json!({"id": id}))?;
    Ok(id)
}

pub fn option_matrix_set(
    store: &Store,
    run: Option<String>,
    options: Vec<OptionRow>,
) -> Result<PathBuf> {
    let run_id = store.resolve_run_id(run)?;
    let doc = OptionMatrixDoc {
        schema_version: "fuzzy.option_matrix.v1".into(),
        options,
        updated_at: Utc::now(),
    };
    store.save_option_matrix(&run_id, &doc)?;
    let path = store.run_dir(&run_id).join("option_matrix.yaml");
    store.append_event(
        Some(&run_id),
        "option_matrix.set",
        json!({"count": doc.options.len()}),
    )?;
    Ok(path)
}

// ---------- Phase 5: run mode / permission ----------

pub fn set_mode(store: &Store, run: Option<String>, mode: WorkMode) -> Result<String> {
    let run_id = store.resolve_run_id(run)?;
    let mut run_doc = store.load_run(&run_id)?;
    run_doc.mode = mode;
    run_doc.updated_at = Utc::now();
    store.save_run(&run_doc)?;
    store.append_event(Some(&run_id), "run.mode_set", json!({"mode": mode}))?;
    Ok(run_id)
}

/// Persist a new permission level on the run's policies, deriving the coarse
/// write gates. The runtime still enforces per-call permission separately.
pub fn set_permission_level(
    store: &Store,
    run: Option<String>,
    level: PermissionLevel,
) -> Result<String> {
    let run_id = store.resolve_run_id(run)?;
    let mut run_doc = store.load_run(&run_id)?;
    let rank = crate::tools::permission_rank(level);
    run_doc.policies.permission_level = level;
    run_doc.policies.scratch_writes_allowed =
        rank >= crate::tools::permission_rank(PermissionLevel::ScratchOnly);
    run_doc.policies.repo_writes_allowed =
        rank >= crate::tools::permission_rank(PermissionLevel::BranchWrite);
    run_doc.updated_at = Utc::now();
    store.save_run(&run_doc)?;
    store.append_event(
        Some(&run_id),
        "permission.set",
        json!({"level": format!("{level:?}")}),
    )?;
    Ok(run_id)
}

// ---------- Phase 5: read-only repository inspection ----------

/// Resolve a repo-relative path under `root`, rejecting escapes and protected
/// paths. Used by every repo read/write tool.
fn safe_repo_path(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        bail!("path must be repository-relative, not absolute: {rel}");
    }
    if rel_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        bail!("path must not escape the repository: {rel}");
    }
    if is_protected_path(rel) {
        bail!("path is protected and may not be accessed: {rel}");
    }
    Ok(root.join(rel_path))
}

/// True for paths the harness must never read or write (secrets, VCS, vendored).
fn is_protected_path(rel: &str) -> bool {
    const PROTECTED: [&str; 6] = [
        ".git/",
        ".env",
        "node_modules/",
        "target/",
        ".venv/",
        "venv/",
    ];
    PROTECTED.iter().any(|p| {
        if let Some(stripped) = p.strip_suffix('/') {
            rel == stripped || rel.starts_with(p) || rel.contains(&format!("/{p}"))
        } else {
            rel.contains(p)
        }
    })
}

pub fn read_repo_file(store: &Store, rel: &str, max_bytes: usize) -> Result<String> {
    let path = safe_repo_path(&store.root, rel)?;
    let text = fs::read_to_string(&path).with_context(|| format!("reading {rel}"))?;
    Ok(text.chars().take(max_bytes).collect())
}

pub fn list_repo_files(store: &Store, rel: &str) -> Result<Vec<String>> {
    let dir = safe_repo_path(&store.root, rel)?;
    let mut names = Vec::new();
    for entry in fs::read_dir(&dir).with_context(|| format!("listing {rel}"))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let suffix = if entry.path().is_dir() { "/" } else { "" };
        names.push(format!("{name}{suffix}"));
    }
    names.sort();
    Ok(names)
}

pub struct GrepHit {
    pub path: String,
    pub line: usize,
    pub text: String,
}

pub fn grep_repo(store: &Store, pattern: &str, rel: &str, max_hits: usize) -> Result<Vec<GrepHit>> {
    let base = safe_repo_path(&store.root, rel)?;
    let mut hits = Vec::new();
    grep_dir(&store.root, &base, pattern, max_hits, &mut hits)?;
    Ok(hits)
}

fn grep_dir(
    root: &Path,
    dir: &Path,
    pattern: &str,
    max_hits: usize,
    hits: &mut Vec<GrepHit>,
) -> Result<()> {
    if hits.len() >= max_hits || !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        if hits.len() >= max_hits {
            break;
        }
        let entry = entry?;
        let path = entry.path();
        let rel = relative_to(&path, root);
        if is_protected_path(&rel) {
            continue;
        }
        if path.is_dir() {
            grep_dir(root, &path, pattern, max_hits, hits)?;
        } else if let Ok(text) = fs::read_to_string(&path) {
            for (i, line) in text.lines().enumerate() {
                if line.contains(pattern) {
                    hits.push(GrepHit {
                        path: rel.clone(),
                        line: i + 1,
                        text: line.chars().take(200).collect(),
                    });
                    if hits.len() >= max_hits {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

// ---------- Phase 5: scratch command / repo write ----------

pub struct CommandRun {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Run a command with its working directory pinned to the run's `scratch/`
/// directory. No shell is invoked, so there is no shell-injection surface.
pub fn run_safe_command(
    store: &Store,
    run: Option<String>,
    program: String,
    args: Vec<String>,
) -> Result<CommandRun> {
    let run_id = store.resolve_run_id(run)?;
    let scratch = store.run_dir(&run_id).join("scratch");
    crate::util::ensure_dir(&scratch)?;
    let output = std::process::Command::new(&program)
        .args(&args)
        .current_dir(&scratch)
        .output()
        .with_context(|| format!("running `{program}` in scratch"))?;
    let truncate = |b: &[u8]| {
        String::from_utf8_lossy(b)
            .chars()
            .take(4000)
            .collect::<String>()
    };
    store.append_event(
        Some(&run_id),
        "command.ran",
        json!({"program": program, "exit_code": output.status.code()}),
    )?;
    Ok(CommandRun {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: truncate(&output.stdout),
        stderr: truncate(&output.stderr),
    })
}

/// Write a repository file (branch-write tier). Protected paths are rejected.
pub fn write_repo_file(
    store: &Store,
    run: Option<String>,
    rel: &str,
    content: &str,
) -> Result<PathBuf> {
    let run_id = store.resolve_run_id(run)?;
    let path = safe_repo_path(&store.root, rel)?;
    write_string(&path, content)?;
    store.append_event(Some(&run_id), "repo.file_written", json!({"path": rel}))?;
    Ok(path)
}

// ---------- gate / report / exit ----------

pub fn gate(store: &Store, run: Option<String>) -> Result<GateOutcome> {
    let run_id = store.resolve_run_id(run)?;
    let run_doc = store.load_run(&run_id)?;
    let q = store.load_questions(&run_id)?;
    let h = store.load_hypotheses(&run_id)?;
    let e = store.load_evidence(&run_id)?;
    let d = store.load_decisions(&run_id)?;
    let report = build_gate_report(&store.run_dir(&run_id), &run_doc, &q, &h, &e, &d);
    let path = store.run_dir(&run_id).join("outputs/gate_report.json");
    write_json_pretty(&path, &report)?;
    store.append_event(
        Some(&run_id),
        "gate.evaluated",
        json!({"pass": report.pass, "score": report.score}),
    )?;
    Ok(GateOutcome { report, path })
}

fn build_gate_report(
    run_dir: &Path,
    run: &RunDoc,
    questions: &OpenQuestionsDoc,
    hypotheses: &HypothesisLedgerDoc,
    evidence: &EvidenceLedgerDoc,
    decisions: &DecisionLogDoc,
) -> GateReport {
    let mut checks = Vec::new();
    let mut add = |name: &str, pass: bool, severity: &str, message: String| {
        checks.push(GateCheck {
            name: name.into(),
            pass,
            severity: severity.into(),
            message,
        });
    };

    add(
        "run artifact",
        run_dir.join("run.yaml").exists(),
        "fail",
        "run.yaml exists".into(),
    );
    add(
        "event log",
        run_dir.join("events.jsonl").exists(),
        "warn",
        "events.jsonl exists".into(),
    );
    add(
        "safe default",
        !run.policies.repo_writes_allowed
            && !run.policies.git_push_allowed
            && !run.policies.pr_creation_allowed,
        "fail",
        "repo writes, pushes, and PR creation are disabled by default".into(),
    );

    let blocking_open = questions
        .questions
        .iter()
        .filter(|q| q.blocking && q.status == QuestionStatus::Open)
        .count();
    add(
        "blocking questions",
        blocking_open == 0,
        "fail",
        format!("{blocking_open} open blocking question(s)"),
    );

    let needs_evidence = matches!(
        run.mode,
        WorkMode::Investigate
            | WorkMode::Troubleshoot
            | WorkMode::Debug
            | WorkMode::Research
            | WorkMode::Audit
            | WorkMode::Incident
    );
    if needs_evidence {
        add(
            "evidence ledger",
            !evidence.evidence.is_empty(),
            "fail",
            format!("{} evidence entrie(s)", evidence.evidence.len()),
        );
    } else {
        add(
            "evidence ledger",
            !evidence.evidence.is_empty(),
            "warn",
            format!("{} evidence entrie(s)", evidence.evidence.len()),
        );
    }

    if matches!(
        run.mode,
        WorkMode::Troubleshoot | WorkMode::Debug | WorkMode::Incident
    ) {
        let strong = hypotheses
            .hypotheses
            .iter()
            .filter(|h| {
                matches!(
                    h.status,
                    HypothesisStatus::Likely | HypothesisStatus::Confirmed
                ) && h.confidence >= 0.5
            })
            .count();
        add(
            "causal hypothesis",
            strong > 0,
            "warn",
            format!("{strong} likely or confirmed hypothesis/hypotheses"),
        );
    }

    if matches!(
        run.mode,
        WorkMode::Design | WorkMode::Scope | WorkMode::Deliver
    ) {
        add(
            "decision log",
            !decisions.decisions.is_empty(),
            "warn",
            format!("{} decision(s)", decisions.decisions.len()),
        );
    }

    let story_path = run_dir.join("outputs/story.json");
    if story_path.exists() {
        let valid = read_json_if_exists::<serde_json::Value>(&story_path)
            .ok()
            .flatten()
            .is_some();
        add(
            "story.json parse",
            valid,
            "fail",
            "outputs/story.json is valid JSON".into(),
        );
    } else if run.mode == WorkMode::Deliver {
        add(
            "story.json present",
            false,
            "fail",
            "delivery mode requires outputs/story.json".into(),
        );
    }

    let fail_count = checks
        .iter()
        .filter(|c| !c.pass && c.severity == "fail")
        .count();
    let pass_count = checks.iter().filter(|c| c.pass).count();
    let score = if checks.is_empty() {
        0.0
    } else {
        pass_count as f32 / checks.len() as f32
    };
    let next_actions = if fail_count == 0 {
        vec!["Choose a typed exit with `fuzzy exit --type ...`.".into()]
    } else {
        checks
            .iter()
            .filter(|c| !c.pass && c.severity == "fail")
            .map(|c| format!("Fix gate: {}", c.name))
            .collect()
    };
    GateReport {
        run_id: run.id.clone(),
        mode: run.mode,
        pass: fail_count == 0,
        score,
        checks,
        next_actions,
        created_at: Utc::now(),
    }
}

pub fn report(store: &Store, run: Option<String>, out: Option<PathBuf>) -> Result<PathBuf> {
    let run_id = store.resolve_run_id(run)?;
    let run_doc = store.load_run(&run_id)?;
    let q = store.load_questions(&run_id)?;
    let h = store.load_hypotheses(&run_id)?;
    let e = store.load_evidence(&run_id)?;
    let d = store.load_decisions(&run_id)?;
    let content = render::final_report(&run_doc, &q, &h, &e, &d);
    let path = out.unwrap_or_else(|| store.run_dir(&run_id).join("outputs/final_report.md"));
    write_string(&path, &content)?;
    store.append_event(
        Some(&run_id),
        "report.generated",
        json!({"path": relative_to(&path, &store.run_dir(&run_id))}),
    )?;
    Ok(path)
}

/// Records a typed exit. The caller is responsible for evaluating and printing
/// the gate first; `gate` is the already-computed gate report.
pub fn record_exit(
    store: &Store,
    run_id: &str,
    exit_type: ExitType,
    note: Vec<String>,
    gate: &GateReport,
) -> Result<PathBuf> {
    if !gate.pass && exit_type != ExitType::Abandon && exit_type != ExitType::Escalation {
        bail!("gate did not pass; use escalation/abandon or resolve gate failures first");
    }
    if exit_type == ExitType::DeliveryStory
        && !store.run_dir(run_id).join("outputs/story.json").exists()
    {
        bail!("delivery-story exit requires outputs/story.json");
    }
    let artifacts = collect_output_artifacts(store, run_id)?;
    let handoff = HandoffDoc {
        schema_version: "fuzzy.handoff.v1".into(),
        run_id: run_id.to_string(),
        exit_type,
        status: "ready".into(),
        created_at: Utc::now(),
        artifacts,
        notes: vec![note.join(" ")]
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect(),
    };
    let path = store.run_dir(run_id).join("handoff.yaml");
    write_yaml(&path, &handoff)?;
    let mut run_doc = store.load_run(run_id)?;
    run_doc.status = if exit_type == ExitType::Abandon {
        RunStatus::Abandoned
    } else {
        RunStatus::Completed
    };
    run_doc.updated_at = Utc::now();
    store.save_run(&run_doc)?;
    store.append_event(Some(run_id), "run.exit", json!({"exit_type": exit_type}))?;
    Ok(path)
}

// ---------- promotions ----------

/// Validate + stage a Workbench story handoff. Proposal only: copies the story
/// and writes a validation report; never invokes Agent Workbench. Requires a
/// valid `story.json` and a passing uncertainty gate.
pub fn promote_workbench(
    store: &Store,
    run: Option<String>,
    story: Option<PathBuf>,
) -> Result<PathBuf> {
    let run_id = store.resolve_run_id(run)?;
    let story_path = story.unwrap_or_else(|| store.run_dir(&run_id).join("outputs/story.json"));
    if !story_path.exists() {
        bail!("story file not found: {}", story_path.display());
    }
    let _value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&story_path)?)
        .with_context(|| format!("story file is not valid JSON: {}", story_path.display()))?;
    let g = gate(store, Some(run_id.clone()))?;
    if !g.report.pass {
        bail!("workbench gate did not pass; resolve gate failures before promotion");
    }
    let dest = store.run_dir(&run_id).join("handoff/workbench/story.json");
    crate::util::ensure_dir(dest.parent().unwrap())?;
    fs::copy(&story_path, &dest)?;
    write_json_pretty(
        &store
            .run_dir(&run_id)
            .join("handoff/workbench/validation_report.json"),
        &json!({
            "valid_json": true,
            "gate_pass": true,
            "story_file": relative_to(&dest, &store.run_dir(&run_id)),
            "validated_at": Utc::now(),
        }),
    )?;
    store.append_event(
        Some(&run_id),
        "promote.workbench",
        json!({"story": relative_to(&dest, &store.run_dir(&run_id))}),
    )?;
    Ok(dest)
}

/// Stage an OpenViking knowledge candidate from the final report. Proposal only:
/// writes a `status: proposed` candidate that still requires curator approval
/// and permission >= handoff before any durable `ov add-resource`.
pub fn promote_openviking(store: &Store, run: Option<String>) -> Result<PathBuf> {
    let run_id = store.resolve_run_id(run)?;
    let report = store.run_dir(&run_id).join("outputs/final_report.md");
    if !report.exists() {
        bail!("final report not found; write a report before promoting to OpenViking");
    }
    let candidate = store
        .run_dir(&run_id)
        .join("handoff/openviking/knowledge_candidate.json");
    write_json_pretty(
        &candidate,
        &json!({
            "schema_version": "fuzzy.ov_candidate.v1",
            "run_id": run_id,
            "report": relative_to(&report, &store.run_dir(&run_id)),
            "status": "proposed",
            "requires": "curator approval + permission >= handoff",
            "created_at": Utc::now(),
        }),
    )?;
    store.append_event(
        Some(&run_id),
        "promote.openviking",
        json!({"candidate": relative_to(&candidate, &store.run_dir(&run_id)), "status": "proposed"}),
    )?;
    Ok(candidate)
}

/// Write an AutoContext review proposal for this run. Proposal only: durable
/// learning still requires approval/promotion downstream.
pub fn run_autocontext_review(store: &Store, run: Option<String>) -> Result<PathBuf> {
    let run_id = store.resolve_run_id(run)?;
    let run_doc = store.load_run(&run_id)?;
    let path = store
        .run_dir(&run_id)
        .join("handoff/autocontext_review.json");
    write_json_pretty(
        &path,
        &json!({
            "schema_version": "fuzzy.autocontext_review.v1",
            "run_id": run_id,
            "mode": run_doc.mode,
            "final_report": "outputs/final_report.md",
            "events": "events.jsonl",
            "status": "proposed",
            "created_at": Utc::now(),
        }),
    )?;
    store.append_event(
        Some(&run_id),
        "autocontext.review",
        json!({"path": relative_to(&path, &store.run_dir(&run_id))}),
    )?;
    Ok(path)
}

// ---------- artifact scaffolds ----------

pub fn write_example_story(store: &Store, run: Option<String>) -> Result<PathBuf> {
    let run_id = store.resolve_run_id(run)?;
    let path = store.run_dir(&run_id).join("outputs/story.json");
    let story = json!({
        "title": "Example delivery story from fuzzy run",
        "description": "Replace this with the converged scope from the fuzzy harness.",
        "acceptance_criteria": [
            "Given the relevant precondition, when the user action occurs, then the expected outcome is observable."
        ],
        "source_fuzzy_run": run_id,
    });
    write_json_pretty(&path, &story)?;
    Ok(path)
}

pub fn new_template(store: &Store, run: Option<String>, artifact: String) -> Result<PathBuf> {
    let run_id = store.resolve_run_id(run)?;
    let path = store.run_dir(&run_id).join("outputs").join(&artifact);
    if path.exists() {
        bail!("artifact already exists: {}", path.display());
    }
    let content = match artifact.as_str() {
        "diagnosis_report.md" => "# Diagnosis Report\n\n## Most likely cause\n\nTBD\n\n## Evidence\n\nTBD\n\n## Alternatives ruled out\n\nTBD\n\n## Remediation\n\nTBD\n",
        "investigation_report.md" => "# Investigation Report\n\n## Question\n\nTBD\n\n## Findings\n\nTBD\n\n## Evidence\n\nTBD\n\n## Recommendation\n\nTBD\n",
        "decision_record.md" => "# Decision Record\n\n## Status\n\nProposed\n\n## Context\n\nTBD\n\n## Decision\n\nTBD\n\n## Consequences\n\nTBD\n",
        "escalation_packet.md" => "# Escalation Packet\n\n## Blocking question\n\nTBD\n\n## What we know\n\nTBD\n\n## What we tried\n\nTBD\n\n## Decision needed\n\nTBD\n",
        _ => "# Artifact\n\nTBD\n",
    };
    write_string(&path, content)?;
    Ok(path)
}

// ---------- helpers ----------

fn collect_output_artifacts(store: &Store, run_id: &str) -> Result<Vec<String>> {
    let mut artifacts = Vec::new();
    let out = store.run_dir(run_id).join("outputs");
    if !out.exists() {
        return Ok(artifacts);
    }
    for entry in fs::read_dir(&out)? {
        let entry = entry?;
        if entry.path().is_file() {
            artifacts.push(relative_to(&entry.path(), &store.run_dir(run_id)));
        }
    }
    artifacts.sort();
    Ok(artifacts)
}

fn count_files(dir: PathBuf, prefix: &str) -> Result<usize> {
    if !dir.exists() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(prefix) {
            count += 1;
        }
    }
    Ok(count)
}
