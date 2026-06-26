# Fuzzy Interactive Agent Shell - Final Summary

## Outcome

`fuzzy` was converted from a deterministic Rust CLI into an interactive, LLM-orchestrated agent shell while keeping Rust as the authority for artifacts, permissions, gates, and execution policy.

The final shape is a hybrid system:

- deterministic subcommands remain available and reusable;
- `fuzzy chat` provides the interactive shell;
- the LLM emits a structured JSON action envelope;
- Rust validates, authorizes, executes, records, and gates every tool call;
- run folders remain the source of truth.

All planned phases, 0A through 7, are complete.

## Core Architecture

The agent loop is intentionally conservative:

1. Build a bounded system prompt from the orchestrator template, tool catalog, and active run snapshot.
2. Send one user turn to the configured backend.
3. Parse only a valid `Fuzzy Action Envelope` JSON response.
4. Repair malformed model output at most twice.
5. Fall back safely if parsing still fails.
6. Execute only registered tools through the Rust runtime.
7. Log lifecycle events and transcript entries with redaction.
8. Optionally send tool results back for a follow-up assistant message.

The LLM can propose actions. It cannot bypass the Rust tool registry, permission ladder, budget limits, approval checks, protected-path checks, or run-state requirements.

## Implemented Capabilities

### Interactive Shell

- `fuzzy chat` REPL with `/quit`, `/exit-chat`, EOF handling, one-shot turns, dry-start, dry-run-turn, and context printing.
- Owl banner gating for chat-only interactive use.
- Mock backend for deterministic tests.
- Ollama backend with local API and direct-cloud API support.
- Active run resume via `.fuzzy/current_run` when no `--run` is supplied.

### Protocol and Runtime

- JSON action envelope protocol with stable tool call IDs.
- Safe malformed-output handling and repair retry path.
- Built-in tool registry with JSON-schema-style required-argument checks.
- Tool lifecycle events: requested, approved, executed, failed, blocked.
- Per-turn budget accounting.
- Optional follow-up response after tool execution.

### Artifacts and Ledgers

- Chat sessions under `.fuzzy/chats/` with transcripts and turn artifacts.
- Active run pointer in `.fuzzy/current_run`.
- Run scaffold includes questions, hypotheses, evidence, decisions, risks, constraints, non-goals, option matrix, gate outputs, reports, handoff artifacts, and events.
- Reports and exits stay file-first and reproducible.

### Knowledge Flow

- `ask_librarian` tool wraps existing librarian behavior.
- OpenViking-aware knowledge lookup with flat-file fallback.
- Read-only explorer packets when librarian confidence is partial or none.
- OpenViking durable-memory proposals are written as curation candidates, not pushed directly by chat tools.

### Permissions and Approvals

Permission ladder:

```text
observe-only < scratch-only < branch-write < repo-write < remediate < handoff
```

Implemented enforcement includes:

- required permission per tool;
- risk metadata per tool;
- confirmation for medium/high-risk actions when approval is required;
- in-flight permission elevation through `set_permission_level`;
- protected path rejection for repository read/write helpers;
- shell-free scratch command execution for `run_safe_command`.

### Gates and Promotions

- Uncertainty gate evaluates unresolved blocking questions, evidence sufficiency, and causal hypothesis state.
- Report generation and typed exit flow are available to the orchestrator.
- Workbench, OpenViking, and AutoContext chat tools are proposal-only and do not perform external durable side effects.
- AutoContext review proposals are written after key decision points or blocked outcomes.

### Redaction and Transcript Safety

- Transcript content is redacted before persistence.
- Redaction covers sensitive assignments, authorization headers, bearer tokens, known token prefixes, long mixed-case secret-like tokens, and inline assignments embedded later in model JSON output.
- Phase 7 fixed a leak class where `api_key=...` inside a one-line model response could bypass first-delimiter assignment redaction.

### Long-Session Polish

- System prompt context is capped by `ContextCaps`.
- Interactive sessions write `.fuzzy/chats/<chat-id>/session_summary.md` every 20 turns.
- Resume behavior is covered by integration tests.

## Phase Completion

| Phase | Result |
| ----- | ------ |
| 0A | Baseline safety tests for existing deterministic CLI behavior. |
| 0B | Shared `ops.rs` extraction so commands and tools reuse structured logic. |
| 0C | Chat-only owl banner predicate and plumbing. |
| 1 | Chat skeleton, backend trait, mock backend, sessions, transcripts, dry modes. |
| 2 | Redaction, action envelope, tool runtime, first tools, bounded context. |
| 3 | Ollama backend, config aliases, doctor output, backend failure handling. |
| 4 | Librarian, read-only explorer, OpenViking curation candidate flow. |
| 5 | Full tool set, permission ladder, approvals, repo helpers, gates, reports. |
| 6 | Proposal-only Workbench/OpenViking/AutoContext promotions. |
| 7 | Resume, compaction, transcript redaction hardening, final acceptance tests. |

## Verification

Final gate passed:

```bash
rtk cargo fmt --check && rtk cargo build && rtk cargo test && rtk cargo clippy --all-targets -- -D warnings
```

Final observed result:

- 84 tests passed;
- build passed;
- formatting check passed;
- clippy passed with `-D warnings`.

## Deferred Items

The following were intentionally left out because they were optional, interactive-only, or lower value for the current harness:

- rustyline-backed history;
- streaming chat responses;
- multi-perspective probe;
- skeptic pass;
- chat-aware report command;
- full nested config table migration beyond accepted dotted-key aliases and flat compatibility.

## Current State

The project now supports a complete guarded agent workflow:

1. start or resume a run;
2. ask the librarian first;
3. gather read-only evidence when needed;
4. record questions, hypotheses, evidence, decisions, risks, constraints, and non-goals;
5. validate uncertainty gates;
6. generate reports;
7. record typed exits;
8. propose downstream promotions without unsafe durable side effects.

The main invariant holds: model intent is advisory, Rust execution is authoritative.