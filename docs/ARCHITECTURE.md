# Architecture

## Core idea

The fuzzy harness is a deterministic control plane for uncertain work. It does not try to be a general agent runtime. It coordinates agents, knowledge, artifacts, gates, and exits.

```text
fuzzy CLI
  owns: run lifecycle, artifacts, ledgers, gates, policy, typed exits

Reference Librarian
  owns: knowledge access and synthesis

Information Explorer
  owns: focused evidence gathering on librarian request

OpenViking
  owns: durable long-term knowledge

AutoContext
  owns: learning/evaluation loops and reusable lessons

Pi / other runners
  own: agent execution

Agent Workbench
  owns: deterministic delivery after a story exists
```

## Borrowed Workbench patterns

- Artifact contracts over plausible return text
- Reference Librarian before exploration
- Information Explorer only as a delegated researcher
- Session/event logging
- Programmatic gates
- Scope/security discipline
- Dry/default-safe execution
- Explicit handoff artifacts

## What is intentionally different from Workbench

Workbench is for well-defined delivery work. The fuzzy harness is for work where the problem, cause, path, facts, or success criteria may still be unclear.

A fuzzy run can end successfully as:

- diagnosis
- investigation report
- decision
- escalation
- experiment result
- runbook update
- delivery story
- abandoned/not worth doing

## Knowledge rule

Worker agents should not query OpenViking directly. The librarian is the gateway. The explorer can gather evidence, but durable knowledge promotion should be curated.
