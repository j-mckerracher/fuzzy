//! Per-turn tool budget accounting (Phase 2).
//!
//! Bounds how much work a single user turn may trigger. Defaults follow the
//! spec: at most 5 tool iterations and 20 total tool calls per turn. Limits are
//! configurable in later phases via `.fuzzy/config.toml`.

use anyhow::{bail, Result};

/// Default maximum tool iterations per user turn.
pub const DEFAULT_MAX_ITERATIONS: usize = 5;
/// Default maximum total tool calls per user turn.
pub const DEFAULT_MAX_CALLS: usize = 20;

/// Mutable budget tracker for one user turn.
#[derive(Debug, Clone)]
pub struct Budget {
    pub max_iterations: usize,
    pub max_calls: usize,
    pub iterations_used: usize,
    pub calls_used: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_ITERATIONS, DEFAULT_MAX_CALLS)
    }
}

impl Budget {
    pub fn new(max_iterations: usize, max_calls: usize) -> Self {
        Self {
            max_iterations,
            max_calls,
            iterations_used: 0,
            calls_used: 0,
        }
    }

    /// Record one tool call, failing if the call budget is exhausted.
    pub fn record_call(&mut self) -> Result<()> {
        if self.calls_used >= self.max_calls {
            bail!(
                "tool call budget exhausted ({} calls); stopping",
                self.max_calls
            );
        }
        self.calls_used += 1;
        Ok(())
    }

    /// Record one tool iteration (a round of model-requested tool calls),
    /// failing if the iteration budget is exhausted.
    #[allow(dead_code)] // Consumed by the multi-iteration follow-up loop in Phase 5.
    pub fn record_iteration(&mut self) -> Result<()> {
        if self.iterations_used >= self.max_iterations {
            bail!(
                "tool iteration budget exhausted ({} iterations); stopping",
                self.max_iterations
            );
        }
        self.iterations_used += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limits_match_spec() {
        let b = Budget::default();
        assert_eq!(b.max_iterations, 5);
        assert_eq!(b.max_calls, 20);
    }

    #[test]
    fn call_budget_is_enforced() {
        let mut b = Budget::new(5, 2);
        assert!(b.record_call().is_ok());
        assert!(b.record_call().is_ok());
        assert!(b.record_call().is_err());
    }
}
