//! Tool runtime types (Phase 2).
//!
//! Every capability the orchestrator can request is a [`FuzzyTool`]. Each tool
//! declares the permission level and risk it needs; the runtime
//! ([`runtime`]) validates those before execution. The LLM only *requests*
//! tools — Rust decides what actually runs.

pub mod builtin;
pub mod runtime;

use crate::budget::Budget;
use crate::models::{EventRecord, PermissionLevel};
use crate::store::Store;
use anyhow::Result;
use serde_json::Value;
use std::path::PathBuf;

/// Coarse risk classification used to decide when human confirmation is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRisk {
    /// Read-only / artifact-only; no confirmation required.
    Safe,
    /// Low-impact mutation within run artifacts.
    Low,
    // Higher risk tiers are emitted by the write/promotion tools added in
    // Phase 5; defined now so the runtime's confirmation path is complete.
    /// Notable mutation; may require confirmation in later phases.
    #[allow(dead_code)]
    Medium,
    /// Destructive or externally visible; always confirm.
    #[allow(dead_code)]
    High,
}

/// The structured result of executing a tool.
pub struct ToolResult {
    pub ok: bool,
    pub summary: String,
    pub data: Value,
    #[allow(dead_code)] // Read by the gate/promotion tools added in Phase 5.
    pub artifacts_written: Vec<PathBuf>,
    #[allow(dead_code)] // Surfaced into the event log by Phase 5 ledger tools.
    pub events: Vec<EventRecord>,
}

impl ToolResult {
    /// Convenience constructor for an ok result with no extra events.
    pub fn ok(summary: impl Into<String>, data: Value) -> Self {
        Self {
            ok: true,
            summary: summary.into(),
            data,
            artifacts_written: Vec::new(),
            events: Vec::new(),
        }
    }
}

/// Execution context handed to a tool. The store is the source of truth; tools
/// write only through `ops::*` so behavior matches the deterministic CLI.
pub struct ToolContext<'a> {
    pub store: &'a Store,
    pub run_id: Option<String>,
    pub permission: PermissionLevel,
    pub budget: &'a mut Budget,
    /// Auto-approve risky tools (MVP). A real confirmation engine lands in
    /// Phase 5 (`approval.rs`).
    pub approvals: bool,
}

/// A capability the orchestrator can request.
pub trait FuzzyTool {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn json_schema(&self) -> Value;
    fn required_permission(&self) -> PermissionLevel;
    fn risk_level(&self) -> ToolRisk;
    /// Whether the tool needs an active run to operate. Defaults to `true`;
    /// `create_run` overrides it.
    fn requires_run(&self) -> bool {
        true
    }
    fn execute(&self, ctx: &mut ToolContext, args: Value) -> Result<ToolResult>;
}

/// Numeric escalation rank for permission comparison. The enum variant order is
/// the escalation order (ObserveOnly is least privileged).
pub fn permission_rank(p: PermissionLevel) -> u8 {
    match p {
        PermissionLevel::ObserveOnly => 0,
        PermissionLevel::ScratchOnly => 1,
        PermissionLevel::BranchWrite => 2,
        PermissionLevel::RepoWrite => 3,
        PermissionLevel::Remediate => 4,
        PermissionLevel::Handoff => 5,
    }
}

/// True when `current` is privileged enough to run a tool requiring `required`.
pub fn permission_allows(current: PermissionLevel, required: PermissionLevel) -> bool {
    permission_rank(current) >= permission_rank(required)
}
