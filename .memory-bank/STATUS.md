# STATUS

Last updated: 2026-08-04

[MEMORY BANK: ACTIVE]

## Summary

IncFleetChat **v0.1.3** — local Tauri v2 app for EVE incursion fleets:

1. **Overlay (`main`)** — Listener fleet-chat site timer Board (Ran / Clear / Clear ready).
2. **Tools (`tools`)** — RunDesk analytics (wallet + Manifest), gamelog enrichment, Joined Results, ammo planner.

Agent docs + Memory Bank are initialized; `.agents/memory/` has architecture, entities, deployment, and **seams**.

## Product state (as of 0.1.3)

Shipped / present in tree:

- Overlay watch + Board + dual Phase adapters (ADR-0001)
- Split Overlay vs Tools settings
- RunDesk open/paste/analyze/focus/amend/reenrich/delete
- Enrichment pipeline with EnrichmentInputs (not AppSettings)
- Joined Results + missileRates display module
- Ammo planner tab
- Windows release workflow + in-app updater (NSIS); portable is manual

Deferred / not wired:

- macOS and Linux release matrix (stubs in `docs/future-plans/`)
- Full `/mb` Cursor workflow is wired globally; project still uses local `.memory-bank/`.

## Active Focus

- Agent/memory setup complete (`AGENTS.md` + seams note).
- **Next:** choose a delivery milestone and write a dated plan under `.memory-bank/plans/`.
- On the next analytics refactor, bump `last_verified` on `.agents/memory/seams.md`.

## Decisions in force

- Track `AGENTS.md`, `.agents/**`, `.memory-bank/**`, `.editorconfig` in git.
- Skip Claude/Cursor platform entry files until requested.
- Dual Phase adapters for overlay Clear UX (ADR-0001).
- Overlay settings may refresh Board; Tools settings must not.
- Missile Hit%/Miss%/expended owned by `missileRates` (do not re-derive in callers).
- Ammo planner outside RunDesk; durable ammo prefs in Tools settings.

## Risks / Constraints

- Windows-first release; other OS deferred.
- `glib` RUSTSEC-2024-0429 accepted for Linux until Tauri 3.
- Enrichment snapshots from older schemas may require Re-enrich.
- No in-repo `agent-folder-init` scaffold script; bank CLI not on PATH.
