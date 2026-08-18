---
type: note
tags: [architecture, status, overlay, rundesk, release]
importance: high
created: 2026-08-04
last_verified: 2026-08-04
---

# Project state snapshot (v0.1.3)

## What was done

- Mapped IncFleetChat from `CONTEXT.md`, `README.md`, `src/`, `src-tauri/`, ADRs, and release docs into agent memory + bank STATUS.

## Current shape

- Two windows: overlay Board (`main`) and Tools (`tools` → RunDesk + Joined Results + Ammo).
- Persistence: SQLite settings, ran/cleared marks, spawns, analytics_runs (+ enrichment_json).
- Release: Windows NSIS + updater; portable manual; macOS/Linux deferred.

## New knowledge

- Prefer `CONTEXT.md` terms; never confuse overlay Site vs analytics Site.
- Dual Phase adapters are intentional (ADR-0001).
- Tools settings must not refresh the Board; EnrichmentInputs is the enrich-time bag.
- Display Hit%/Miss%/expended only via `missileRates`.

## Follow-ups

- Choose next delivery milestone (I-007).
- Optionally add coding conventions to `AGENTS.md` (I-001).
