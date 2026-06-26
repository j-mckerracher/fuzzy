# Fuzzy CLI Harness

`fuzzy` is a CLI-first Rust harness for uncertain work: troubleshooting, investigation, debugging, design, triage, incidents, experiments, scope shaping, and optional delivery handoff.

It is intentionally not a UI. The source of truth is the run folder: YAML ledgers, JSON packets, Markdown reports, event logs, and handoff artifacts.

## What this MVP implements

- File-first run folders under `fuzzy-runs/`
- `fuzzy init`, `start`, `list`, `status`, `open`
- Open questions ledger
- Hypothesis ledger
- Evidence ledger
- Decision log
- Reference Librarian command surface
- Information Explorer command surface
- OpenViking-aware librarian adapter with flat-file fallback
- Read-only repository exploration
- Uncertainty gate
- Markdown report generator
- Typed exits: diagnosis, investigation, decision, escalation, delivery story, abandon, etc.
- Dry Workbench promotion that validates/copies `story.json`
- Dry OpenViking and AutoContext promotion seams

## Design stance

The harness owns deterministic control:

- modes
- artifacts
- gates
- policy
- permission ladder
- run folders
- events
- exits
- handoff

Agents and external systems are adapters:

- Reference Librarian: single gateway to project knowledge
- Information Explorer: focused evidence gathering when the librarian cannot answer fully
- OpenViking: durable long-term knowledge backend
- AutoContext: learning/evaluation layer
- Pi: future runtime backend through RPC or extension seams
- Agent Workbench: downstream delivery target after a valid story exists

## Install locally

```bash
cargo build --release
cargo install --path .
```

The binary name is `fuzzy`.

## Quick start

```bash
fuzzy init

fuzzy start --mode troubleshoot "API imports fail intermittently in prod"

fuzzy librarian ask "what do we already know about import failures?"

fuzzy question add --blocking "Which deploy first showed the failure?"
fuzzy evidence add --source-type log --source prod.log --confidence high "Failures began after deploy 2026-06-21"
fuzzy hypothesis add --confidence 0.4 "Import path cache may be stale after deploy"
fuzzy hypothesis update H-001 --status likely --confidence 0.72 --evidence E-001
fuzzy question resolve Q-001 --evidence E-001 "Failures first appeared after deploy 2026-06-21"

fuzzy gate
fuzzy report
fuzzy exit --type diagnosis "Root cause likely enough for remediation planning."
```

## Interactive chat

`fuzzy chat` runs the guarded agent shell. The model proposes a JSON action envelope; Rust validates and executes any tool calls.

The default backend is `none`, so chat will not make network calls until you opt in:

```bash
fuzzy init
fuzzy chat --backend mock --one-shot "create_run for an auth investigation"
```

For the default Ollama Cloud model:

```bash
export OLLAMA_API_KEY=...

fuzzy config set agent.default_backend ollama
fuzzy config set agent.ollama.direct_cloud_api true
fuzzy config set agent.ollama.model glm-5.2:cloud

fuzzy chat --one-shot "start an investigation for the flaky importer"
fuzzy chat
```

Cloud-hosted models do not need `ollama pull`; no local weights are downloaded.

For local Ollama with a local model:

```bash
ollama pull llama3.2
ollama serve

fuzzy config set agent.default_backend ollama
fuzzy config set agent.ollama.direct_cloud_api false
fuzzy config set agent.ollama.model llama3.2
fuzzy chat
```

Useful inspection commands:

```bash
fuzzy chat --print-context
fuzzy chat --dry-run-turn "what would you do next?"
fuzzy doctor
```

## Work modes

```text
investigate
troubleshoot
debug
research
triage
design
audit
incident
experiment
scope
deliver
```

## Permission model

The MVP defaults to read-heavy operation:

```text
observe-only
scratch-only
branch-write
repo-write
remediate
handoff
```

Only the policy fields are implemented in v0. Actual sandboxing must be provided by the OS, container, worktree, or external runner.

## Librarian and explorer discipline

The intended workflow mirrors Agent Workbench:

1. Ask the Reference Librarian first.
2. The librarian checks OpenViking if available.
3. If OpenViking is unavailable or insufficient, the librarian falls back to flat-file knowledge.
4. If confidence is partial or none, the librarian can invoke the Information Explorer.
5. The explorer returns evidence to the librarian; it does not promote durable knowledge directly.
6. Curated knowledge can later be promoted to OpenViking.

Useful commands:

```bash
fuzzy librarian ask "where is the auth flow documented?"
fuzzy librarian ask --force-explorer "what changed around CI caching?"
fuzzy explorer run --record-evidence "cache key"
fuzzy promote openviking --exec --wait
```

## OpenViking integration

If `ov` is installed and healthy, `fuzzy librarian ask` probes:

```bash
ov system health
ov overview viking://resources/knowledge/
ov -o json --compact find "<query>" --limit 5
```

If that fails, the harness falls back to local knowledge files:

```text
.fuzzy/knowledge/
agent-context/knowledge/
knowledge/
docs/
```

## Agent Workbench handoff

Workbench is one downstream target, not the purpose of the fuzzy harness.

```bash
fuzzy example-story
fuzzy promote workbench
```

By default this is dry: it validates/copies the story into:

```text
fuzzy-runs/<RUN-ID>/handoff/workbench/story.json
```

To invoke Workbench directly:

```bash
fuzzy config set workbench_path /path/to/agent-workbench
fuzzy promote workbench --exec
```

The command executed is:

```bash
python /path/to/agent-workbench/run.py --manual-story-file <story.json>
```

## AutoContext seam

The MVP writes a payload for later AutoContext ingestion:

```bash
fuzzy promote autocontext
```

With `--exec`, it currently attempts:

```bash
autoctx ingest --json <payload>
```

Treat this as an adapter seam to adjust to your pinned AutoContext version.

## Project layout

```text
.fuzzy/
  config.toml
  knowledge/

fuzzy-runs/
  FUZ-.../
    run.yaml
    work_brief.md
    open_questions.yaml
    hypothesis_ledger.yaml
    evidence_ledger.yaml
    decision_log.md
    action_log.yaml
    policy.yaml
    events.jsonl
    librarian/
      knowledge_packets/
    explorer/
      evidence_packets/
    logs/
      reference_librarian/
      information_explorer/
    outputs/
    handoff/
      workbench/
    scratch/
```

## Useful commands

```bash
fuzzy doctor
fuzzy config show
fuzzy config set knowledge_backend openviking
fuzzy config set default_permission_level scratch-only

fuzzy list
fuzzy status
fuzzy open

fuzzy template diagnosis_report.md
fuzzy template investigation_report.md
fuzzy template decision_record.md
fuzzy template escalation_packet.md

fuzzy export --output my-run.tar.gz
```

## Development notes

This repository is intentionally small and boring. Most logic lives in:

```text
src/models.rs
src/store.rs
src/adapters.rs
src/commands.rs
src/render.rs
```

The next implementation step should be to split this into a Rust workspace once the domain stabilizes:

```text
fuzzy-core
fuzzy-store
fuzzy-gates
fuzzy-openviking
fuzzy-pi
fuzzy-autocontext
fuzzy-workbench
fuzzy-cli
```
