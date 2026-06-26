//! Tool registry and execution pipeline (Phase 2).
//!
//! Validates every model-requested tool call before running it: name exists →
//! required args present → permission sufficient → run-state ok → budget ok →
//! (confirmation if risky) → execute. Each step appends a lifecycle event so
//! the full decision trail is auditable by call id.

use super::builtin;
use super::{permission_allows, FuzzyTool, ToolContext, ToolRisk};
use crate::models::PermissionLevel;
use crate::protocol::ToolCall;
use serde_json::{json, Value};

/// Public metadata for a registered tool (used to build the system prompt).
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub permission: PermissionLevel,
    pub risk: ToolRisk,
}

/// Registry of available tools.
pub struct ToolRegistry {
    tools: Vec<Box<dyn FuzzyTool>>,
}

impl ToolRegistry {
    /// Build the registry with the Phase 2 built-in tool set.
    pub fn builtin() -> Self {
        Self {
            tools: vec![
                Box::new(builtin::LoadStatusTool),
                Box::new(builtin::CreateRunTool),
                Box::new(builtin::AddQuestionTool),
                Box::new(builtin::AskLibrarianTool),
                Box::new(builtin::RunExplorerReadonlyTool),
                Box::new(builtin::ProposeOpenvikingMemoryTool),
                Box::new(builtin::ValidateGateTool),
                Box::new(builtin::ResolveQuestionTool),
                Box::new(builtin::AddHypothesisTool),
                Box::new(builtin::UpdateHypothesisTool),
                Box::new(builtin::AddEvidenceTool),
                Box::new(builtin::AddDecisionTool),
                Box::new(builtin::AddRiskTool),
                Box::new(builtin::AddConstraintTool),
                Box::new(builtin::AddNonGoalTool),
                Box::new(builtin::WriteOptionMatrixTool),
                Box::new(builtin::SetModeTool),
                Box::new(builtin::SetPermissionLevelTool),
                Box::new(builtin::WriteReportTool),
                Box::new(builtin::RecordExitTool),
                Box::new(builtin::ReadFileTool),
                Box::new(builtin::GrepRepoTool),
                Box::new(builtin::ListFilesTool),
                Box::new(builtin::RunSafeCommandTool),
                Box::new(builtin::WriteRepoFileTool),
                Box::new(builtin::PromoteWorkbenchTool),
                Box::new(builtin::PromoteOpenvikingTool),
                Box::new(builtin::RunAutocontextReviewTool),
            ],
        }
    }

    fn get(&self, name: &str) -> Option<&dyn FuzzyTool> {
        self.tools
            .iter()
            .find(|t| t.name() == name)
            .map(|t| t.as_ref())
    }

    /// Metadata for each tool, for prompt rendering.
    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools
            .iter()
            .map(|t| ToolSpec {
                name: t.name(),
                description: t.description(),
                permission: t.required_permission(),
                risk: t.risk_level(),
            })
            .collect()
    }
}

/// The outcome of attempting one tool call.
pub struct ToolOutcome {
    pub call_id: String,
    pub name: String,
    pub ok: bool,
    pub blocked: bool,
    pub summary: String,
    pub data: Value,
}

impl ToolOutcome {
    fn blocked(call: &ToolCall, reason: impl Into<String>) -> Self {
        Self {
            call_id: call.id.clone(),
            name: call.name.clone(),
            ok: false,
            blocked: true,
            summary: reason.into(),
            data: Value::Null,
        }
    }
}

/// Validate and execute a single tool call, logging lifecycle events.
pub fn execute_call(
    registry: &ToolRegistry,
    ctx: &mut ToolContext,
    call: &ToolCall,
) -> ToolOutcome {
    let outcome = execute_call_inner(registry, ctx, call);
    maybe_autocontext_review(ctx, &outcome);
    outcome
}

/// After terminal/decision-point tool calls, write an AutoContext review
/// proposal for the active run (proposal only; durable learning needs
/// downstream approval). Best-effort: failures are ignored.
fn maybe_autocontext_review(ctx: &mut ToolContext, outcome: &ToolOutcome) {
    const TRIGGERS: [&str; 4] = [
        "validate_gate",
        "record_exit",
        "promote_workbench",
        "promote_openviking",
    ];
    if outcome.name == "run_autocontext_review" {
        return;
    }
    let trigger = outcome.blocked || TRIGGERS.contains(&outcome.name.as_str());
    if !trigger {
        return;
    }
    let Some(run_id) = ctx.run_id.clone() else {
        return;
    };
    if let Ok(path) = crate::ops::run_autocontext_review(ctx.store, Some(run_id.clone())) {
        log_event(
            ctx,
            Some(&run_id),
            "autocontext.review.auto",
            json!({"trigger": outcome.name, "path": path.to_string_lossy()}),
        );
    }
}

fn execute_call_inner(
    registry: &ToolRegistry,
    ctx: &mut ToolContext,
    call: &ToolCall,
) -> ToolOutcome {
    let run_ref = ctx.run_id.clone();
    log_event(
        ctx,
        run_ref.as_deref(),
        "tool_call.requested",
        json!({"id": call.id, "name": call.name}),
    );

    // 1. Tool must exist.
    let Some(tool) = registry.get(&call.name) else {
        let reason = format!("unknown tool `{}`", call.name);
        log_event(
            ctx,
            run_ref.as_deref(),
            "tool_call.blocked",
            json!({"id": call.id, "name": call.name, "reason": reason}),
        );
        return ToolOutcome::blocked(call, reason);
    };

    // 2. Required arguments must be present.
    if let Some(missing) = missing_required_arg(&tool.json_schema(), &call.arguments) {
        let reason = format!("missing required argument `{missing}`");
        log_event(
            ctx,
            run_ref.as_deref(),
            "tool_call.blocked",
            json!({"id": call.id, "name": call.name, "reason": reason}),
        );
        return ToolOutcome::blocked(call, reason);
    }

    // 3. Permission must be sufficient.
    if !permission_allows(ctx.permission, tool.required_permission()) {
        let reason = format!(
            "permission `{:?}` insufficient (requires `{:?}`)",
            ctx.permission,
            tool.required_permission()
        );
        log_event(
            ctx,
            run_ref.as_deref(),
            "tool_call.blocked",
            json!({"id": call.id, "name": call.name, "reason": reason}),
        );
        return ToolOutcome::blocked(call, reason);
    }

    // 4. Run-state requirement.
    if tool.requires_run() && ctx.run_id.is_none() {
        let reason = "no active run; this tool needs a run".to_string();
        log_event(
            ctx,
            run_ref.as_deref(),
            "tool_call.blocked",
            json!({"id": call.id, "name": call.name, "reason": reason}),
        );
        return ToolOutcome::blocked(call, reason);
    }

    // 5. Budget.
    if let Err(e) = ctx.budget.record_call() {
        let reason = e.to_string();
        log_event(
            ctx,
            run_ref.as_deref(),
            "tool_call.blocked",
            json!({"id": call.id, "name": call.name, "reason": reason}),
        );
        return ToolOutcome::blocked(call, reason);
    }

    // 6. Confirmation for risky tools via the approval engine.
    if crate::approval::needs_confirmation(tool.risk_level()) {
        log_event(
            ctx,
            run_ref.as_deref(),
            "approval.requested",
            json!({"id": call.id, "name": call.name, "risk": format!("{:?}", tool.risk_level())}),
        );
        match crate::approval::confirm(&call.name, ctx.approvals) {
            crate::approval::Decision::Granted => {
                log_event(
                    ctx,
                    run_ref.as_deref(),
                    "approval.granted",
                    json!({"id": call.id, "name": call.name}),
                );
            }
            crate::approval::Decision::Denied => {
                let reason = "confirmation denied".to_string();
                log_event(
                    ctx,
                    run_ref.as_deref(),
                    "approval.denied",
                    json!({"id": call.id, "name": call.name, "reason": reason}),
                );
                return ToolOutcome::blocked(call, reason);
            }
        }
    }

    // 7. Execute.
    match tool.execute(ctx, call.arguments.clone()) {
        Ok(result) => {
            // Bind a freshly created run as the active run for subsequent calls.
            if ctx.run_id.is_none() {
                if let Some(new_run) = result.data.get("run_id").and_then(|v| v.as_str()) {
                    ctx.run_id = Some(new_run.to_string());
                    let _ = ctx.store.set_active_run_id(new_run);
                }
            }
            log_event(
                ctx,
                ctx.run_id.clone().as_deref(),
                "tool_call.executed",
                json!({"id": call.id, "name": call.name, "ok": result.ok}),
            );
            ToolOutcome {
                call_id: call.id.clone(),
                name: call.name.clone(),
                ok: result.ok,
                blocked: false,
                summary: result.summary,
                data: result.data,
            }
        }
        Err(e) => {
            let reason = e.to_string();
            log_event(
                ctx,
                run_ref.as_deref(),
                "tool_call.failed",
                json!({"id": call.id, "name": call.name, "reason": reason}),
            );
            ToolOutcome {
                call_id: call.id.clone(),
                name: call.name.clone(),
                ok: false,
                blocked: false,
                summary: reason,
                data: Value::Null,
            }
        }
    }
}

/// Append a tool lifecycle event to the store's event log (best-effort).
fn log_event(ctx: &ToolContext, run_id: Option<&str>, event_type: &str, data: Value) {
    let _ = ctx.store.append_event(run_id, event_type, data);
}

/// Return the first `required` schema field missing from `args`, if any.
fn missing_required_arg(schema: &Value, args: &Value) -> Option<String> {
    let required = schema.get("required").and_then(|v| v.as_array())?;
    let obj = args.as_object();
    for field in required {
        let Some(name) = field.as_str() else { continue };
        let present = obj.map(|o| o.contains_key(name)).unwrap_or(false);
        if !present {
            return Some(name.to_string());
        }
    }
    None
}
