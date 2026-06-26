//! Phase 5 confirmation engine.
//!
//! The orchestrator only *requests* tools; before a risky tool runs, the
//! runtime asks this engine whether a human must confirm. With auto-approve
//! (the non-interactive default), confirmation is granted automatically; in an
//! interactive session the user is prompted on stdin. Every request and its
//! resolution is logged as an `approval.*` event by the runtime.

use crate::tools::ToolRisk;
use std::io::{self, BufRead, Write};

/// Outcome of a confirmation request.
pub enum Decision {
    Granted,
    Denied,
}

/// Whether a tool of this risk tier needs explicit human confirmation.
pub fn needs_confirmation(risk: ToolRisk) -> bool {
    matches!(risk, ToolRisk::Medium | ToolRisk::High)
}

/// Resolve a confirmation. With `auto_approve`, always grant. Otherwise prompt
/// on stdin; only an explicit yes grants. EOF or any other answer denies.
pub fn confirm(action: &str, auto_approve: bool) -> Decision {
    if auto_approve {
        return Decision::Granted;
    }
    print!("Allow `{action}`? [y/N] ");
    io::stdout().flush().ok();
    let mut line = String::new();
    let read = io::stdin().lock().read_line(&mut line).unwrap_or(0);
    if read == 0 {
        return Decision::Denied;
    }
    match line.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Decision::Granted,
        _ => Decision::Denied,
    }
}
