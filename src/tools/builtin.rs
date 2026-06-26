//! Built-in Phase 2 tools.
//!
//! Each tool is observe-only and delegates to `ops::*` so it shares the exact
//! logic of the deterministic CLI subcommands. Tools never print to stdout —
//! they return a [`ToolResult`]; the REPL renders summaries.

use super::{FuzzyTool, ToolContext, ToolResult, ToolRisk};
use crate::models::{Confidence, ExitType, HypothesisStatus, OptionRow, PermissionLevel, WorkMode};
use crate::ops;
use anyhow::Result;
use clap::ValueEnum;
use serde_json::{json, Value};

/// Read-only snapshot of the active run.
pub struct LoadStatusTool;

impl FuzzyTool for LoadStatusTool {
    fn name(&self) -> &'static str {
        "load_status"
    }
    fn description(&self) -> &'static str {
        "Load the current run's status (questions, hypotheses, evidence, decisions)."
    }
    fn json_schema(&self) -> Value {
        json!({"type": "object", "properties": {}, "required": []})
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Safe
    }
    fn execute(&self, ctx: &mut ToolContext, _args: Value) -> Result<ToolResult> {
        let bundle = ops::status(ctx.store, ctx.run_id.clone())?;
        let summary = format!(
            "run {} [{:?}/{:?}]: {} questions, {} hypotheses, {} evidence, {} decisions",
            bundle.run.id,
            bundle.run.mode,
            bundle.run.status,
            bundle.questions.questions.len(),
            bundle.hypotheses.hypotheses.len(),
            bundle.evidence.evidence.len(),
            bundle.decisions.decisions.len(),
        );
        let data = json!({
            "run_id": bundle.run.id,
            "mode": format!("{:?}", bundle.run.mode),
            "status": format!("{:?}", bundle.run.status),
            "open_questions": bundle.questions.questions.len(),
        });
        Ok(ToolResult::ok(summary, data))
    }
}

/// Create a new run (used on the first turn when no run is active).
pub struct CreateRunTool;

impl FuzzyTool for CreateRunTool {
    fn name(&self) -> &'static str {
        "create_run"
    }
    fn description(&self) -> &'static str {
        "Create a new run for substantive work. Args: request (string), mode (optional), title (optional)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "request": {"type": "string"},
                "mode": {"type": "string"},
                "title": {"type": "string"}
            },
            "required": ["request"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn requires_run(&self) -> bool {
        false
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let request = args
            .get("request")
            .and_then(|v| v.as_str())
            .unwrap_or("interactive session")
            .to_string();
        let mode = args
            .get("mode")
            .and_then(|v| v.as_str())
            .and_then(|s| WorkMode::from_str(s, true).ok())
            .unwrap_or(WorkMode::Troubleshoot);
        let title = args
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let (run, dir) = ops::start(ctx.store, mode, title, vec![request])?;
        let summary = format!("created run {} ({:?})", run.id, run.mode);
        let data = json!({"run_id": run.id, "mode": format!("{:?}", run.mode)});
        Ok(ToolResult {
            ok: true,
            summary,
            data,
            artifacts_written: vec![dir],
            events: Vec::new(),
        })
    }
}

/// Record a blocking or non-blocking open question.
pub struct AddQuestionTool;

impl FuzzyTool for AddQuestionTool {
    fn name(&self) -> &'static str {
        "add_question"
    }
    fn description(&self) -> &'static str {
        "Add an open question. Args: question (string), blocking (optional bool), owner (optional)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "question": {"type": "string"},
                "blocking": {"type": "boolean"},
                "owner": {"type": "string"}
            },
            "required": ["question"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let blocking = args
            .get("blocking")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let owner = args
            .get("owner")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let id = ops::question_add(
            ctx.store,
            ctx.run_id.clone(),
            vec![question],
            blocking,
            owner,
        )?;
        let summary = format!("added question {id} (blocking: {blocking})");
        Ok(ToolResult::ok(summary, json!({"question_id": id})))
    }
}

/// Query the Reference Librarian (knowledge-first).
pub struct AskLibrarianTool;

impl FuzzyTool for AskLibrarianTool {
    fn name(&self) -> &'static str {
        "ask_librarian"
    }
    fn description(&self) -> &'static str {
        "Ask the Reference Librarian. Args: question (string), allow_explorer (optional bool)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "question": {"type": "string"},
                "allow_explorer": {"type": "boolean"}
            },
            "required": ["question"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Safe
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let allow_explorer = args
            .get("allow_explorer")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let result = ops::librarian_ask(
            ctx.store,
            ctx.run_id.clone(),
            vec![question],
            false,
            !allow_explorer,
        )?;
        let explorer_used = result.exploration_report_path.is_some();
        let summary = format!(
            "librarian packet {} (confidence: {:?}, {} sources{})",
            result.packet.id,
            result.packet.confidence,
            result.packet.sources.len(),
            if explorer_used { ", explorer used" } else { "" },
        );
        let data = json!({
            "knowledge_packet_id": result.packet.id,
            "confidence": format!("{:?}", result.packet.confidence),
            "knowledge_mode": format!("{:?}", result.packet.knowledge_mode),
            "source_count": result.packet.sources.len(),
            "explorer_used": explorer_used,
        });
        Ok(ToolResult::ok(summary, data))
    }
}

/// Run a read-only repository exploration pass (librarian-routed evidence).
///
/// Writes an `explorer/evidence_packets/EP-*.json` report but never records
/// durable knowledge or mutates the repository.
pub struct RunExplorerReadonlyTool;

impl FuzzyTool for RunExplorerReadonlyTool {
    fn name(&self) -> &'static str {
        "run_explorer_readonly"
    }
    fn description(&self) -> &'static str {
        "Run a read-only repository exploration. Args: question (string), scope (optional path). Writes an evidence packet only."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "question": {"type": "string"},
                "scope": {"type": "string"}
            },
            "required": ["question"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Safe
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let scope = args
            .get("scope")
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from);
        // record_evidence = false keeps the pass strictly read-only.
        let result =
            ops::explorer_run(ctx.store, ctx.run_id.clone(), vec![question], scope, false)?;
        let summary = format!(
            "explorer {} ({} evidence, confidence {:?})",
            result.report.exploration_id,
            result.report.evidence.len(),
            result.report.confidence,
        );
        let data = json!({
            "exploration_id": result.report.exploration_id,
            "evidence_count": result.report.evidence.len(),
            "confidence": format!("{:?}", result.report.confidence),
        });
        Ok(ToolResult {
            ok: true,
            summary,
            data,
            artifacts_written: vec![result.report_path],
            events: Vec::new(),
        })
    }
}

/// Propose an OpenViking memory candidate (written under run artifacts only).
///
/// This does NOT write durable OpenViking knowledge; a later promotion step
/// (with explicit approval) does that.
pub struct ProposeOpenvikingMemoryTool;

impl FuzzyTool for ProposeOpenvikingMemoryTool {
    fn name(&self) -> &'static str {
        "propose_openviking_memory"
    }
    fn description(&self) -> &'static str {
        "Propose a durable OpenViking memory candidate (saved under run artifacts only, not promoted). Args: title (string), body (string), source_packet_id (optional), sources (optional array of strings)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "title": {"type": "string"},
                "body": {"type": "string"},
                "source_packet_id": {"type": "string"},
                "sources": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["title", "body"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let title = args
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let body = args
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let source_packet_id = args
            .get("source_packet_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let sources = args
            .get("sources")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let candidate = ops::propose_openviking_memory(
            ctx.store,
            ctx.run_id.clone(),
            title,
            body,
            source_packet_id,
            sources,
        )?;
        let summary = format!(
            "proposed OpenViking candidate {} (not promoted)",
            candidate.id
        );
        Ok(ToolResult {
            ok: true,
            summary,
            data: json!({"candidate_id": candidate.id}),
            artifacts_written: vec![candidate.path],
            events: Vec::new(),
        })
    }
}

/// Run the uncertainty gate for the active run.
pub struct ValidateGateTool;

impl FuzzyTool for ValidateGateTool {
    fn name(&self) -> &'static str {
        "validate_gate"
    }
    fn description(&self) -> &'static str {
        "Validate the active run against its uncertainty gate."
    }
    fn json_schema(&self) -> Value {
        json!({"type": "object", "properties": {}, "required": []})
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Safe
    }
    fn execute(&self, ctx: &mut ToolContext, _args: Value) -> Result<ToolResult> {
        let outcome = ops::gate(ctx.store, ctx.run_id.clone())?;
        let summary = format!(
            "gate {} (score {:.2}) for run {}",
            if outcome.report.pass { "PASS" } else { "FAIL" },
            outcome.report.score,
            outcome.report.run_id
        );
        let data = json!({
            "pass": outcome.report.pass,
            "score": outcome.report.score,
        });
        Ok(ToolResult::ok(summary, data))
    }
}

// =====================================================================
// Phase 5 ledger tools (observe-only mutations within run artifacts).
// =====================================================================

fn arg_str(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn arg_opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn arg_str_vec(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve an open question.
pub struct ResolveQuestionTool;

impl FuzzyTool for ResolveQuestionTool {
    fn name(&self) -> &'static str {
        "resolve_question"
    }
    fn description(&self) -> &'static str {
        "Resolve an open question. Args: id (string), resolution (string), evidence (optional array of evidence IDs)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "resolution": {"type": "string"},
                "evidence": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["id", "resolution"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let id = arg_str(&args, "id");
        let resolution = arg_str(&args, "resolution");
        let evidence = arg_str_vec(&args, "evidence");
        let resolved = ops::question_resolve(
            ctx.store,
            ctx.run_id.clone(),
            id,
            vec![resolution],
            evidence,
        )?;
        Ok(ToolResult::ok(
            format!("resolved question {resolved}"),
            json!({"question_id": resolved}),
        ))
    }
}

/// Add a hypothesis to the ledger.
pub struct AddHypothesisTool;

impl FuzzyTool for AddHypothesisTool {
    fn name(&self) -> &'static str {
        "add_hypothesis"
    }
    fn description(&self) -> &'static str {
        "Add a hypothesis. Args: claim (string), confidence (optional 0..1), falsification (optional string)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "claim": {"type": "string"},
                "confidence": {"type": "number"},
                "falsification": {"type": "string"}
            },
            "required": ["claim"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let claim = arg_str(&args, "claim");
        let confidence = args
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.3) as f32;
        let falsification = arg_opt_str(&args, "falsification");
        let id = ops::hypothesis_add(
            ctx.store,
            ctx.run_id.clone(),
            vec![claim],
            confidence,
            falsification,
        )?;
        Ok(ToolResult::ok(
            format!("added hypothesis {id}"),
            json!({"hypothesis_id": id}),
        ))
    }
}

/// Update a hypothesis (status / confidence / linked evidence).
pub struct UpdateHypothesisTool;

impl FuzzyTool for UpdateHypothesisTool {
    fn name(&self) -> &'static str {
        "update_hypothesis"
    }
    fn description(&self) -> &'static str {
        "Update a hypothesis. Args: id (string), status (optional: open|likely|unlikely|confirmed|refuted), confidence (optional 0..1), evidence (optional array)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "status": {"type": "string"},
                "confidence": {"type": "number"},
                "evidence": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["id"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let id = arg_str(&args, "id");
        let status = args
            .get("status")
            .and_then(|v| v.as_str())
            .and_then(|s| HypothesisStatus::from_str(s, true).ok());
        let confidence = args
            .get("confidence")
            .and_then(|v| v.as_f64())
            .map(|c| c as f32);
        let evidence = arg_str_vec(&args, "evidence");
        let updated = ops::hypothesis_update(
            ctx.store,
            ctx.run_id.clone(),
            id,
            status,
            confidence,
            evidence,
        )?;
        Ok(ToolResult::ok(
            format!("updated hypothesis {updated}"),
            json!({"hypothesis_id": updated}),
        ))
    }
}

/// Record an evidence entry.
pub struct AddEvidenceTool;

impl FuzzyTool for AddEvidenceTool {
    fn name(&self) -> &'static str {
        "add_evidence"
    }
    fn description(&self) -> &'static str {
        "Record evidence. Args: claim (string), source_type (optional), source (optional), confidence (optional: none|low|partial|medium|high|full), excerpt (optional), used_by (optional array)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "claim": {"type": "string"},
                "source_type": {"type": "string"},
                "source": {"type": "string"},
                "confidence": {"type": "string"},
                "excerpt": {"type": "string"},
                "used_by": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["claim"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let claim = arg_str(&args, "claim");
        let source_type = arg_opt_str(&args, "source_type").unwrap_or_else(|| "observation".into());
        let source = arg_opt_str(&args, "source");
        let confidence = args
            .get("confidence")
            .and_then(|v| v.as_str())
            .and_then(|s| Confidence::from_str(s, true).ok())
            .unwrap_or(Confidence::Partial);
        let excerpt = arg_opt_str(&args, "excerpt");
        let used_by = arg_str_vec(&args, "used_by");
        let id = ops::evidence_add(
            ctx.store,
            ctx.run_id.clone(),
            vec![claim],
            source_type,
            source,
            confidence,
            excerpt,
            None,
            used_by,
        )?;
        Ok(ToolResult::ok(
            format!("added evidence {id}"),
            json!({"evidence_id": id}),
        ))
    }
}

/// Record a decision.
pub struct AddDecisionTool;

impl FuzzyTool for AddDecisionTool {
    fn name(&self) -> &'static str {
        "add_decision"
    }
    fn description(&self) -> &'static str {
        "Record a decision. Args: decision (string), rationale (optional), evidence (optional array)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "decision": {"type": "string"},
                "rationale": {"type": "string"},
                "evidence": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["decision"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let decision = arg_str(&args, "decision");
        let rationale = arg_opt_str(&args, "rationale");
        let evidence = arg_str_vec(&args, "evidence");
        let id = ops::decision_add(
            ctx.store,
            ctx.run_id.clone(),
            vec![decision],
            rationale,
            evidence,
        )?;
        Ok(ToolResult::ok(
            format!("added decision {id}"),
            json!({"decision_id": id}),
        ))
    }
}

/// Add a risk to the risk log.
pub struct AddRiskTool;

impl FuzzyTool for AddRiskTool {
    fn name(&self) -> &'static str {
        "add_risk"
    }
    fn description(&self) -> &'static str {
        "Add a risk. Args: risk (string), severity (optional: low|medium|high), mitigation (optional)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "risk": {"type": "string"},
                "severity": {"type": "string"},
                "mitigation": {"type": "string"}
            },
            "required": ["risk"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let risk = arg_str(&args, "risk");
        let severity = arg_opt_str(&args, "severity").unwrap_or_else(|| "medium".into());
        let mitigation = arg_opt_str(&args, "mitigation");
        let id = ops::risk_add(
            ctx.store,
            ctx.run_id.clone(),
            vec![risk],
            severity,
            mitigation,
        )?;
        Ok(ToolResult::ok(
            format!("added risk {id}"),
            json!({"risk_id": id}),
        ))
    }
}

/// Add a constraint.
pub struct AddConstraintTool;

impl FuzzyTool for AddConstraintTool {
    fn name(&self) -> &'static str {
        "add_constraint"
    }
    fn description(&self) -> &'static str {
        "Add a constraint. Args: constraint (string)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {"constraint": {"type": "string"}},
            "required": ["constraint"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let constraint = arg_str(&args, "constraint");
        let id = ops::constraint_add(ctx.store, ctx.run_id.clone(), vec![constraint])?;
        Ok(ToolResult::ok(
            format!("added constraint {id}"),
            json!({"constraint_id": id}),
        ))
    }
}

/// Add a non-goal.
pub struct AddNonGoalTool;

impl FuzzyTool for AddNonGoalTool {
    fn name(&self) -> &'static str {
        "add_non_goal"
    }
    fn description(&self) -> &'static str {
        "Add a non-goal (explicitly out of scope). Args: non_goal (string)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {"non_goal": {"type": "string"}},
            "required": ["non_goal"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let non_goal = arg_str(&args, "non_goal");
        let id = ops::non_goal_add(ctx.store, ctx.run_id.clone(), vec![non_goal])?;
        Ok(ToolResult::ok(
            format!("added non-goal {id}"),
            json!({"non_goal_id": id}),
        ))
    }
}

/// Replace the option matrix.
pub struct WriteOptionMatrixTool;

impl FuzzyTool for WriteOptionMatrixTool {
    fn name(&self) -> &'static str {
        "write_option_matrix"
    }
    fn description(&self) -> &'static str {
        "Write the option matrix. Args: options (array of {option, pros[], cons[], recommendation?})."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "options": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "option": {"type": "string"},
                            "pros": {"type": "array", "items": {"type": "string"}},
                            "cons": {"type": "array", "items": {"type": "string"}},
                            "recommendation": {"type": "string"}
                        },
                        "required": ["option"]
                    }
                }
            },
            "required": ["options"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let options: Vec<OptionRow> = args
            .get("options")
            .and_then(|v| v.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| {
                        let option = r.get("option")?.as_str()?.to_string();
                        Some(OptionRow {
                            option,
                            pros: arg_str_vec(r, "pros"),
                            cons: arg_str_vec(r, "cons"),
                            recommendation: arg_opt_str(r, "recommendation"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let count = options.len();
        let path = ops::option_matrix_set(ctx.store, ctx.run_id.clone(), options)?;
        Ok(ToolResult {
            ok: true,
            summary: format!("wrote option matrix with {count} option(s)"),
            data: json!({"options": count}),
            artifacts_written: vec![path],
            events: Vec::new(),
        })
    }
}

/// Change the active run's work mode.
pub struct SetModeTool;

impl FuzzyTool for SetModeTool {
    fn name(&self) -> &'static str {
        "set_mode"
    }
    fn description(&self) -> &'static str {
        "Set the run's work mode. Args: mode (string, e.g. troubleshoot|investigate|design|scope|deliver)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {"mode": {"type": "string"}},
            "required": ["mode"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let mode = args
            .get("mode")
            .and_then(|v| v.as_str())
            .and_then(|s| WorkMode::from_str(s, true).ok())
            .ok_or_else(|| anyhow::anyhow!("`mode` must be a valid work mode"))?;
        ops::set_mode(ctx.store, ctx.run_id.clone(), mode)?;
        Ok(ToolResult::ok(
            format!("set mode to {mode:?}"),
            json!({"mode": format!("{mode:?}")}),
        ))
    }
}

/// Change the run's permission level (requires confirmation).
pub struct SetPermissionLevelTool;

impl FuzzyTool for SetPermissionLevelTool {
    fn name(&self) -> &'static str {
        "set_permission_level"
    }
    fn description(&self) -> &'static str {
        "Request a new permission level (requires confirmation). Args: level (observe-only|scratch-only|branch-write|repo-write|remediate|handoff)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {"level": {"type": "string"}},
            "required": ["level"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        // Permission elevation always warrants explicit confirmation.
        ToolRisk::Medium
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let level = ops::parse_permission(&arg_str(&args, "level"))?;
        ops::set_permission_level(ctx.store, ctx.run_id.clone(), level)?;
        // Elevate the in-flight context so later calls this turn see the change.
        ctx.permission = level;
        Ok(ToolResult::ok(
            format!("permission level set to {level:?}"),
            json!({"level": format!("{level:?}")}),
        ))
    }
}

/// Write the run's final / diagnosis report.
pub struct WriteReportTool;

impl FuzzyTool for WriteReportTool {
    fn name(&self) -> &'static str {
        "write_report"
    }
    fn description(&self) -> &'static str {
        "Write the run's report (diagnosis / final) from its ledgers."
    }
    fn json_schema(&self) -> Value {
        json!({"type": "object", "properties": {}, "required": []})
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, _args: Value) -> Result<ToolResult> {
        let path = ops::report(ctx.store, ctx.run_id.clone(), None)?;
        Ok(ToolResult {
            ok: true,
            summary: "wrote run report".into(),
            data: json!({"path": path.to_string_lossy()}),
            artifacts_written: vec![path],
            events: Vec::new(),
        })
    }
}

/// Record a typed exit (gate is evaluated first; requires confirmation).
pub struct RecordExitTool;

impl FuzzyTool for RecordExitTool {
    fn name(&self) -> &'static str {
        "record_exit"
    }
    fn description(&self) -> &'static str {
        "Record a typed exit after evaluating the gate. Args: exit_type (string), note (optional)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "exit_type": {"type": "string"},
                "note": {"type": "string"}
            },
            "required": ["exit_type"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Medium
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let exit_type = ExitType::from_str(&arg_str(&args, "exit_type"), true)
            .map_err(|_| anyhow::anyhow!("`exit_type` must be a valid exit type"))?;
        let note = arg_opt_str(&args, "note")
            .map(|n| vec![n])
            .unwrap_or_default();
        let gate = ops::gate(ctx.store, ctx.run_id.clone())?;
        let run_id = gate.report.run_id.clone();
        let path = ops::record_exit(ctx.store, &run_id, exit_type, note, &gate.report)?;
        Ok(ToolResult {
            ok: true,
            summary: format!("recorded {exit_type:?} exit for run {run_id}"),
            data: json!({"exit_type": format!("{exit_type:?}"), "gate_pass": gate.report.pass}),
            artifacts_written: vec![path],
            events: Vec::new(),
        })
    }
}

// =====================================================================
// Phase 5 repository tools (read-only inspection + gated mutation).
// =====================================================================

/// Read a repository file (read-only).
pub struct ReadFileTool;

impl FuzzyTool for ReadFileTool {
    fn name(&self) -> &'static str {
        "read_file"
    }
    fn description(&self) -> &'static str {
        "Read a repository file (read-only). Args: path (repo-relative string)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Safe
    }
    fn requires_run(&self) -> bool {
        false
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let path = arg_str(&args, "path");
        let content = ops::read_repo_file(ctx.store, &path, 8000)?;
        Ok(ToolResult::ok(
            format!("read {path} ({} chars)", content.len()),
            json!({"path": path, "content": content}),
        ))
    }
}

/// Grep the repository (read-only).
pub struct GrepRepoTool;

impl FuzzyTool for GrepRepoTool {
    fn name(&self) -> &'static str {
        "grep_repo"
    }
    fn description(&self) -> &'static str {
        "Search repository file contents (read-only). Args: pattern (string), path (optional repo-relative dir)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string"},
                "path": {"type": "string"}
            },
            "required": ["pattern"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Safe
    }
    fn requires_run(&self) -> bool {
        false
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let pattern = arg_str(&args, "pattern");
        let path = arg_opt_str(&args, "path").unwrap_or_else(|| ".".into());
        let hits = ops::grep_repo(ctx.store, &pattern, &path, 50)?;
        let rendered: Vec<Value> = hits
            .iter()
            .map(|h| json!({"path": h.path, "line": h.line, "text": h.text}))
            .collect();
        Ok(ToolResult::ok(
            format!("{} match(es) for `{pattern}`", hits.len()),
            json!({"hits": rendered}),
        ))
    }
}

/// List repository files in a directory (read-only).
pub struct ListFilesTool;

impl FuzzyTool for ListFilesTool {
    fn name(&self) -> &'static str {
        "list_files"
    }
    fn description(&self) -> &'static str {
        "List entries in a repository directory (read-only). Args: path (optional repo-relative dir)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": []
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Safe
    }
    fn requires_run(&self) -> bool {
        false
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let path = arg_opt_str(&args, "path").unwrap_or_else(|| ".".into());
        let entries = ops::list_repo_files(ctx.store, &path)?;
        Ok(ToolResult::ok(
            format!("{} entrie(s) in {path}", entries.len()),
            json!({"entries": entries}),
        ))
    }
}

/// Run a command pinned to the run's scratch directory (scratch-only tier).
pub struct RunSafeCommandTool;

impl FuzzyTool for RunSafeCommandTool {
    fn name(&self) -> &'static str {
        "run_safe_command"
    }
    fn description(&self) -> &'static str {
        "Run a command in the run scratch directory (no shell). Args: program (string), args (optional array)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "program": {"type": "string"},
                "args": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["program"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ScratchOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Medium
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let program = arg_str(&args, "program");
        let cmd_args = arg_str_vec(&args, "args");
        let run = ops::run_safe_command(ctx.store, ctx.run_id.clone(), program, cmd_args)?;
        Ok(ToolResult::ok(
            format!("command exited {}", run.exit_code),
            json!({"exit_code": run.exit_code, "stdout": run.stdout, "stderr": run.stderr}),
        ))
    }
}

/// Write a repository file (branch-write tier; high risk).
pub struct WriteRepoFileTool;

impl FuzzyTool for WriteRepoFileTool {
    fn name(&self) -> &'static str {
        "write_repo_file"
    }
    fn description(&self) -> &'static str {
        "Write a repository file (branch-write; protected paths rejected). Args: path (repo-relative string), content (string)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            },
            "required": ["path", "content"]
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::BranchWrite
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::High
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let path = arg_str(&args, "path");
        let content = arg_str(&args, "content");
        let written = ops::write_repo_file(ctx.store, ctx.run_id.clone(), &path, &content)?;
        Ok(ToolResult {
            ok: true,
            summary: format!("wrote repository file {path}"),
            data: json!({"path": path}),
            artifacts_written: vec![written],
            events: Vec::new(),
        })
    }
}

pub struct PromoteWorkbenchTool;

impl FuzzyTool for PromoteWorkbenchTool {
    fn name(&self) -> &'static str {
        "promote_workbench"
    }
    fn description(&self) -> &'static str {
        "Stage an Agent Workbench story handoff (handoff permission). Requires a valid outputs/story.json and a passing gate; proposal only, never auto-invokes Workbench. Args: story (optional repo path string)."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "story": {"type": "string"}
            },
            "required": []
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::Handoff
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::High
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult> {
        let story = arg_opt_str(&args, "story").map(std::path::PathBuf::from);
        let dest = ops::promote_workbench(ctx.store, ctx.run_id.clone(), story)?;
        Ok(ToolResult {
            ok: true,
            summary: "staged Workbench story handoff (proposal only)".into(),
            data: json!({"story": dest.to_string_lossy()}),
            artifacts_written: vec![dest],
            events: Vec::new(),
        })
    }
}

pub struct PromoteOpenvikingTool;

impl FuzzyTool for PromoteOpenvikingTool {
    fn name(&self) -> &'static str {
        "promote_openviking"
    }
    fn description(&self) -> &'static str {
        "Stage an OpenViking knowledge candidate from the final report (handoff permission). Proposal only: writes status=proposed; durable add-resource still needs curator approval. No args."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::Handoff
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::High
    }
    fn execute(&self, ctx: &mut ToolContext, _args: Value) -> Result<ToolResult> {
        let candidate = ops::promote_openviking(ctx.store, ctx.run_id.clone())?;
        Ok(ToolResult {
            ok: true,
            summary: "staged OpenViking knowledge candidate (status: proposed)".into(),
            data: json!({"candidate": candidate.to_string_lossy()}),
            artifacts_written: vec![candidate],
            events: Vec::new(),
        })
    }
}

pub struct RunAutocontextReviewTool;

impl FuzzyTool for RunAutocontextReviewTool {
    fn name(&self) -> &'static str {
        "run_autocontext_review"
    }
    fn description(&self) -> &'static str {
        "Write an AutoContext review proposal for the active run (observe-only). Proposal only: durable learning requires downstream approval. No args."
    }
    fn json_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    fn required_permission(&self) -> PermissionLevel {
        PermissionLevel::ObserveOnly
    }
    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Low
    }
    fn execute(&self, ctx: &mut ToolContext, _args: Value) -> Result<ToolResult> {
        let path = ops::run_autocontext_review(ctx.store, ctx.run_id.clone())?;
        Ok(ToolResult {
            ok: true,
            summary: "wrote AutoContext review proposal".into(),
            data: json!({"path": path.to_string_lossy()}),
            artifacts_written: vec![path],
            events: Vec::new(),
        })
    }
}
