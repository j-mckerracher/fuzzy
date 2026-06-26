//! Bounded context builder (Phase 2).
//!
//! Assembles the orchestrator system prompt: the static template, the dynamic
//! tool catalog from the registry, and a capped snapshot of the active run.
//! Caps keep the prompt within a budget; richer `[chat.context]` configuration
//! lands in later phases.

use crate::ops;
use crate::store::Store;
use crate::tools::runtime::ToolRegistry;

/// The static orchestrator system prompt template.
const ORCHESTRATOR_PROMPT: &str = include_str!("../templates/prompts/fuzzy_orchestrator.md");

/// Upper bounds for assembled context sections.
pub struct ContextCaps {
    pub max_status_chars: usize,
}

impl Default for ContextCaps {
    fn default() -> Self {
        Self {
            max_status_chars: 2000,
        }
    }
}

/// Build the full system prompt: template + tool catalog + run snapshot.
pub fn build_system_prompt(
    registry: &ToolRegistry,
    store: Option<&Store>,
    run_id: Option<&str>,
    caps: &ContextCaps,
) -> String {
    let mut s = String::new();
    s.push_str(ORCHESTRATOR_PROMPT.trim_end());
    s.push_str("\n\n## Available tools\n");
    for spec in registry.specs() {
        s.push_str(&format!(
            "- `{}` (permission: {:?}, risk: {:?}): {}\n",
            spec.name, spec.permission, spec.risk, spec.description
        ));
    }

    s.push_str("\n## Active run\n");
    match (store, run_id) {
        (Some(store), Some(rid)) => match ops::status(store, Some(rid.to_string())) {
            Ok(bundle) => {
                let snippet = format!(
                    "id: {}\nmode: {:?}\nstatus: {:?}\nopen questions: {}\nhypotheses: {}\nevidence: {}\ndecisions: {}\n",
                    bundle.run.id,
                    bundle.run.mode,
                    bundle.run.status,
                    bundle.questions.questions.len(),
                    bundle.hypotheses.hypotheses.len(),
                    bundle.evidence.evidence.len(),
                    bundle.decisions.decisions.len(),
                );
                s.push_str(&truncate(&snippet, caps.max_status_chars));
            }
            Err(_) => s.push_str("(run not found)\n"),
        },
        _ => s.push_str("(none)\n"),
    }
    s
}

/// Truncate `text` to at most `max` characters, marking elision.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_lists_tools_and_envelope_marker() {
        let registry = ToolRegistry::builtin();
        let prompt = build_system_prompt(&registry, None, None, &ContextCaps::default());
        assert!(prompt.contains("Fuzzy Action Envelope"));
        assert!(prompt.contains("load_status"));
        assert!(prompt.contains("create_run"));
        assert!(prompt.contains("(none)"));
    }

    #[test]
    fn truncate_marks_elision() {
        let out = truncate("abcdefghij", 5);
        assert_eq!(out, "ab...");
    }
}
