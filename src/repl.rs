//! Interactive chat REPL (Phase 2).
//!
//! Wires the owl banner (Phase 0C), backend resolution, the action-envelope
//! protocol, the tool runtime, redaction, and bounded context assembly into a
//! single-turn / interactive orchestrator. Real network backends land in
//! Phase 3.

use crate::budget::Budget;
use crate::chat::ChatSession;
use crate::context::{self, ContextCaps};
use crate::llm::{
    build_backend, ollama::OllamaConfig, resolve_backend_name, ChatBackend, ChatMessage,
    ChatRequest, Role, Usage,
};
use crate::models::{BannerMode, PermissionLevel};
use crate::owl;
use crate::protocol::{self, ActionEnvelope};
use crate::store::Store;
use crate::tools::runtime::{self, ToolOutcome, ToolRegistry};
use crate::tools::ToolContext;
use crate::transcript::TranscriptEvent;
use anyhow::Result;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

/// Options assembled by `main` from global flags + chat subcommand args.
pub struct ChatOptions {
    pub root: Option<PathBuf>,
    pub backend: Option<String>,
    pub agent_backend: Option<String>,
    pub run: Option<String>,
    pub one_shot: Option<String>,
    pub dry_start: bool,
    pub print_context: bool,
    pub dry_run_turn: Option<String>,
    pub banner: BannerMode,
    pub no_banner: bool,
    pub json: bool,
    pub approvals: bool,
}

/// Maximum repair retries when a model response fails to parse.
const MAX_REPAIRS: usize = 2;

pub fn run_chat(opts: ChatOptions) -> Result<()> {
    // The project store is optional: `--dry-start` and other inspection paths
    // must work without an initialized `.fuzzy` project.
    let store = Store::open(opts.root.clone()).ok();
    let config_default = store
        .as_ref()
        .map(|s| s.config.default_agent_backend.clone());

    // Banner first, governed by the Phase 0C predicate.
    if owl::should_print_banner_env(opts.banner, true, opts.json, opts.no_banner) {
        println!("{}", owl::OWL_BANNER);
    }

    let backend_name = resolve_backend_name(
        opts.backend.as_deref(),
        opts.agent_backend.as_deref(),
        config_default.as_deref(),
    );

    let registry = ToolRegistry::builtin();
    let caps = ContextCaps::default();

    // Resume: when no explicit --run is given, bind to the project's active run.
    let resolved_run = opts.run.clone().or_else(|| {
        store
            .as_ref()
            .and_then(|s| s.active_run_id().ok().flatten())
    });

    // --dry-start: prove startup wiring without contacting a backend.
    if opts.dry_start {
        println!("[dry-start] chat ready (backend: {backend_name}, no model call)");
        return Ok(());
    }

    // --print-context: show the assembled system prompt, no backend call.
    if opts.print_context {
        let prompt =
            context::build_system_prompt(&registry, store.as_ref(), resolved_run.as_deref(), &caps);
        println!("backend: {backend_name}");
        println!(
            "run: {}",
            resolved_run.as_deref().unwrap_or("(no active run)")
        );
        println!("--- system prompt ---");
        println!("{prompt}");
        return Ok(());
    }

    // --dry-run-turn: build the full request and show it, no backend call.
    if let Some(text) = opts.dry_run_turn.as_deref() {
        let prompt =
            context::build_system_prompt(&registry, store.as_ref(), resolved_run.as_deref(), &caps);
        let req = build_request(&backend_name, &prompt, text);
        print_request(&req);
        return Ok(());
    }

    // Announce a resumed run only when the user did not pin one explicitly.
    if opts.run.is_none() {
        if let Some(rid) = resolved_run.as_deref() {
            println!("[resume] active run: {rid}");
        }
    }
    let session = ChatSession::new(backend_name.clone(), resolved_run);
    if let Some(s) = store.as_ref() {
        s.create_chat_session(&session)?;
    }

    let backend = build_backend(&backend_name, ollama_config(store.as_ref()))?;
    let permission = store
        .as_ref()
        .map(|s| s.config.default_permission_level)
        .unwrap_or(PermissionLevel::ObserveOnly);

    // --one-shot: run exactly one turn and exit.
    if let Some(text) = opts.one_shot.as_deref() {
        run_turn(
            store.as_ref(),
            &session,
            &registry,
            backend.as_ref(),
            permission,
            &caps,
            text,
            1,
            opts.json,
            opts.approvals,
        )?;
        return Ok(());
    }

    interactive_loop(
        store.as_ref(),
        &session,
        &registry,
        backend.as_ref(),
        permission,
        &caps,
        &opts,
    )
}

#[allow(clippy::too_many_arguments)]
fn interactive_loop(
    store: Option<&Store>,
    session: &ChatSession,
    registry: &ToolRegistry,
    backend: &dyn ChatBackend,
    permission: PermissionLevel,
    caps: &ContextCaps,
    opts: &ChatOptions,
) -> Result<()> {
    print_status_line(store, &session.backend, session.run_id.as_deref());
    println!("Type /quit or /exit-chat to leave.");

    let stdin = io::stdin();
    let mut turn_index = 0usize;
    loop {
        print!("fuzzy> ");
        io::stdout().flush().ok();

        let mut line = String::new();
        let read = stdin.lock().read_line(&mut line)?;
        if read == 0 {
            // EOF (Ctrl-D): graceful exit.
            println!();
            break;
        }
        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        match input {
            "/quit" | "/exit-chat" => break,
            _ => {
                turn_index += 1;
                run_turn(
                    store,
                    session,
                    registry,
                    backend,
                    permission,
                    caps,
                    input,
                    turn_index,
                    opts.json,
                    opts.approvals,
                )?;
                if let Some(s) = store {
                    maybe_compact(s, session, turn_index)?;
                }
            }
        }
    }
    Ok(())
}

/// Completed-turn budget after which the interactive transcript is compacted
/// into a rolling session summary, keeping long sessions within context bounds.
const COMPACTION_TURN_BUDGET: usize = 20;

/// Whether `turn_index` crosses a compaction boundary.
fn should_compact(turn_index: usize) -> bool {
    turn_index > 0 && turn_index.is_multiple_of(COMPACTION_TURN_BUDGET)
}

/// Render the rolling session summary persisted at compaction boundaries.
fn session_summary(session: &ChatSession, turn_index: usize) -> String {
    format!(
        "# Session summary\n\nchat: {}\nbackend: {}\nrun: {}\nturns: {}\n\nOlder turns are compacted; see `transcript.jsonl` for the full record.\n",
        session.id,
        session.backend,
        session.run_id.as_deref().unwrap_or("(none)"),
        turn_index,
    )
}

/// Write the rolling session summary when a compaction boundary is reached.
fn maybe_compact(store: &Store, session: &ChatSession, turn_index: usize) -> Result<()> {
    if !should_compact(turn_index) {
        return Ok(());
    }
    let path = store.chat_dir(&session.id).join("session_summary.md");
    crate::util::write_string(&path, &session_summary(session, turn_index))?;
    Ok(())
}

/// Execute a single orchestrator turn: assemble context, call the backend,
/// parse the action envelope (with repair retries), execute any tool calls, and
/// optionally run a follow-up. All content is redacted before transcript
/// storage.
#[allow(clippy::too_many_arguments)]
fn run_turn(
    store: Option<&Store>,
    session: &ChatSession,
    registry: &ToolRegistry,
    backend: &dyn ChatBackend,
    permission: PermissionLevel,
    caps: &ContextCaps,
    user_input: &str,
    turn_index: usize,
    json: bool,
    approvals: bool,
) -> Result<()> {
    let current_run = resolve_current_run(store, session);
    let run_ref = current_run.as_deref();
    let system_prompt = context::build_system_prompt(registry, store, run_ref, caps);

    if let Some(s) = store {
        s.append_transcript_event(
            &session.id,
            &TranscriptEvent::entry(
                &session.id,
                run_ref,
                "user.message",
                Some("user"),
                user_input,
            ),
        )?;
        s.append_transcript_event(
            &session.id,
            &TranscriptEvent::meta(&session.id, run_ref, "llm.request", None, user_input),
        )?;
    }

    let req = build_request(backend.model(), &system_prompt, user_input);
    let resp = backend.complete_chat(req)?;
    if let Some(s) = store {
        s.append_transcript_event(
            &session.id,
            &TranscriptEvent::entry(&session.id, run_ref, "llm.response", None, &resp.content),
        )?;
    }

    // Parse with repair retries: raw → fenced JSON → repair prompt(s).
    let mut raw = resp.content.clone();
    let mut envelope = protocol::try_parse(&raw);
    let mut repairs = 0;
    while envelope.is_none() && repairs < MAX_REPAIRS {
        repairs += 1;
        let repair_req = build_repair_request(backend.model(), &system_prompt, user_input, &raw);
        let repair_resp = backend.complete_chat(repair_req)?;
        raw = repair_resp.content;
        if let Some(s) = store {
            s.append_transcript_event(
                &session.id,
                &TranscriptEvent::entry(&session.id, run_ref, "llm.repair_response", None, &raw),
            )?;
        }
        envelope = protocol::try_parse(&raw);
    }

    let Some(envelope) = envelope else {
        // Safe fallback: persist the raw malformed response, run no tools.
        if let Some(s) = store {
            s.append_transcript_event(
                &session.id,
                &TranscriptEvent::entry(&session.id, run_ref, "llm.malformed", None, &raw),
            )?;
        }
        println!("{}", protocol::FALLBACK_MESSAGE);
        return Ok(());
    };

    print_assistant(&envelope.assistant_message, json, &resp.usage);
    if let Some(s) = store {
        s.append_transcript_event(
            &session.id,
            &TranscriptEvent::entry(
                &session.id,
                run_ref,
                "assistant.message",
                Some("assistant"),
                &envelope.assistant_message,
            ),
        )?;
    }

    // Execute tool calls (only when a project store is available).
    let mut tool_results: Vec<Value> = Vec::new();
    let mut final_run = current_run.clone();
    if let Some(s) = store {
        if !envelope.tool_calls.is_empty() {
            let mut budget = Budget::default();
            let mut ctx = ToolContext {
                store: s,
                run_id: current_run.clone(),
                permission,
                budget: &mut budget,
                approvals,
            };
            for call in &envelope.tool_calls {
                let outcome = runtime::execute_call(registry, &mut ctx, call);
                print_outcome(&outcome, json);
                tool_results.push(json!({
                    "id": outcome.call_id,
                    "name": outcome.name,
                    "ok": outcome.ok,
                    "summary": outcome.summary,
                    "data": outcome.data,
                }));
            }
            final_run = ctx.run_id.clone();

            // Optional follow-up: feed tool results back for a final message.
            if envelope.needs_followup {
                let followup = build_followup_request(
                    backend.model(),
                    &system_prompt,
                    user_input,
                    &raw,
                    &tool_results,
                );
                let fresp = backend.complete_chat(followup)?;
                if let Some(env2) = protocol::try_parse(&fresp.content) {
                    print_assistant(&env2.assistant_message, json, &fresp.usage);
                    s.append_transcript_event(
                        &session.id,
                        &TranscriptEvent::entry(
                            &session.id,
                            final_run.as_deref(),
                            "assistant.message",
                            Some("assistant"),
                            &env2.assistant_message,
                        ),
                    )?;
                }
            }
        }
    }

    if let Some(s) = store {
        s.write_turn_artifacts(
            &session.id,
            turn_index,
            &turn_artifact(
                turn_index,
                backend.model(),
                &envelope,
                &tool_results,
                &resp.usage,
            ),
        )?;
    }
    let _ = final_run;
    Ok(())
}

/// Build Ollama connection settings from the project config (or defaults when
/// no project store is available).
fn ollama_config(store: Option<&Store>) -> OllamaConfig {
    match store {
        Some(s) => OllamaConfig {
            base_url: s.config.ollama_base_url.clone(),
            model: s.config.ollama_model.clone(),
            direct_cloud_api: s.config.ollama_direct_cloud_api,
            api_key_env: s.config.ollama_api_key_env.clone(),
        },
        None => OllamaConfig::default(),
    }
}

/// Resolve the run the turn should operate on: the session binding, else the
/// project's active-run pointer.
fn resolve_current_run(store: Option<&Store>, session: &ChatSession) -> Option<String> {
    if let Some(r) = session.run_id.clone() {
        return Some(r);
    }
    store.and_then(|s| s.active_run_id().ok().flatten())
}

fn build_request(model: &str, system_prompt: &str, user_input: &str) -> ChatRequest {
    ChatRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage::new(Role::System, system_prompt),
            ChatMessage::new(Role::User, user_input),
        ],
    }
}

fn build_repair_request(
    model: &str,
    system_prompt: &str,
    user_input: &str,
    prior: &str,
) -> ChatRequest {
    ChatRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage::new(Role::System, system_prompt),
            ChatMessage::new(Role::User, user_input),
            ChatMessage::new(Role::Assistant, prior),
            ChatMessage::new(Role::User, protocol::REPAIR_PROMPT),
        ],
    }
}

fn build_followup_request(
    model: &str,
    system_prompt: &str,
    user_input: &str,
    prior: &str,
    tool_results: &[Value],
) -> ChatRequest {
    let results = json!({ "tool_results": tool_results }).to_string();
    ChatRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage::new(Role::System, system_prompt),
            ChatMessage::new(Role::User, user_input),
            ChatMessage::new(Role::Assistant, prior),
            ChatMessage::new(Role::Tool, results),
        ],
    }
}

fn turn_artifact(
    turn_index: usize,
    backend: &str,
    envelope: &ActionEnvelope,
    tool_results: &[Value],
    usage: &Usage,
) -> Value {
    json!({
        "turn": turn_index,
        "backend": backend,
        "assistant_message": envelope.assistant_message,
        "tool_calls": envelope.tool_calls.len(),
        "tool_results": tool_results,
        "usage": {
            "prompt_tokens": usage.prompt_tokens,
            "completion_tokens": usage.completion_tokens,
            "total_tokens": usage.total_tokens,
        },
    })
}

fn print_assistant(message: &str, json: bool, usage: &Usage) {
    if json {
        let line = serde_json::to_string(&json!({
            "assistant": message,
            "usage": { "total_tokens": usage.total_tokens },
        }))
        .unwrap_or_default();
        println!("{line}");
    } else {
        println!("{message}");
    }
}

fn print_outcome(outcome: &ToolOutcome, json: bool) {
    if json {
        return;
    }
    if outcome.blocked {
        println!("[blocked] {}: {}", outcome.name, outcome.summary);
    } else if outcome.ok {
        println!("[tool] {}: {}", outcome.name, outcome.summary);
    } else {
        println!("[tool-error] {}: {}", outcome.name, outcome.summary);
    }
}

fn print_status_line(store: Option<&Store>, backend: &str, run: Option<&str>) {
    match store {
        Some(s) => {
            println!("project: {}", s.root.display());
            println!("backend: {backend}");
            println!("knowledge: {}", s.config.knowledge_backend);
            println!("permission: {:?}", s.config.default_permission_level);
        }
        None => {
            println!("project: (none)");
            println!("backend: {backend}");
        }
    }
    println!("run: {}", run.unwrap_or("(no active run)"));
}

fn print_request(req: &ChatRequest) {
    println!("[dry-run-turn] model: {}", req.model);
    for msg in &req.messages {
        println!("  {}: {}", msg.role.as_str(), msg.content);
    }
}

/// Print the assembled system prompt for a run (`fuzzy debug context`).
pub fn debug_context(root: Option<PathBuf>, run: Option<String>) -> Result<()> {
    let store = Store::open(root).ok();
    let registry = ToolRegistry::builtin();
    let caps = ContextCaps::default();
    let prompt = context::build_system_prompt(&registry, store.as_ref(), run.as_deref(), &caps);
    println!("{prompt}");
    Ok(())
}

/// Parse and validate an action-envelope JSON file (`fuzzy debug envelope`).
pub fn debug_envelope(file: PathBuf) -> Result<()> {
    let raw = std::fs::read_to_string(&file)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", file.display()))?;
    match protocol::try_parse(&raw) {
        Some(env) => {
            println!("valid envelope");
            println!("assistant_message: {}", env.assistant_message);
            println!("needs_user_input: {}", env.needs_user_input);
            println!("needs_followup: {}", env.needs_followup);
            println!("tool_calls: {}", env.tool_calls.len());
            for call in &env.tool_calls {
                println!("  - {} ({})", call.name, call.id);
            }
        }
        None => {
            println!("invalid: could not parse a Fuzzy Action Envelope");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compaction_boundary_logic() {
        assert!(!should_compact(0));
        assert!(!should_compact(1));
        assert!(!should_compact(COMPACTION_TURN_BUDGET - 1));
        assert!(should_compact(COMPACTION_TURN_BUDGET));
        assert!(should_compact(COMPACTION_TURN_BUDGET * 2));
        assert!(!should_compact(COMPACTION_TURN_BUDGET + 1));
    }

    #[test]
    fn summary_includes_identity_and_run() {
        let session = ChatSession {
            id: "CHAT-X".to_string(),
            backend: "mock".to_string(),
            run_id: Some("RUN-1".to_string()),
        };
        let out = session_summary(&session, 20);
        assert!(out.contains("chat: CHAT-X"));
        assert!(out.contains("backend: mock"));
        assert!(out.contains("run: RUN-1"));
        assert!(out.contains("turns: 20"));
    }
}
