---
type: note
tags: [agents, memory-bank, gitignore, scaffolding]
importance: high
created: 2026-08-04
---

# Agent scaffold + Memory Bank decisions

## What was done

- Scaffolded lean agent docs (`.agents/`, `AGENTS.md`, `.editorconfig`) and a local Memory Bank (`.memory-bank/`).
- Confirmed dry-run first, then write on user approval.

## Decisions

1. **Git tracking** — Commit `AGENTS.md`, `.agents/**`, `.memory-bank/**`, and `.editorconfig`. Do not ignore the scaffold by default.
2. **Optional privacy** — If session logs should stay local, ignore `.agents/sessions/*.md` while keeping `README.md` and `TEMPLATE.md`. Not applied yet.
3. **Platform entry surfaces** — Do not add `CLAUDE.md` or `.cursor/rules/agent-context.mdc` unless those platforms are explicitly requested.
4. **Tooling gap** — No in-repo `scaffold.py`; no `memory-bank` CLI on PATH. Updates are manual until tooling is installed.

## New knowledge

- Product domain language and scope already documented in root `CONTEXT.md` (overlay + RunDesk analytics). Prefer promoting excerpts into memory rather than rewriting from scratch.
