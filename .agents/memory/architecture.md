---
last_verified: 2026-08-04
---

# Architecture

IncFleetChat is a **local Tauri v2 desktop app** (Rust core + React/TypeScript UI) for EVE Online incursion fleets.

## Product surfaces

| Window | Label | UI root | Role |
|--------|-------|---------|------|
| Overlay | `main` | `src/App.tsx` | Site-timer Board from Listener fleet chat |
| Tools | `tools` | `src/tools/ToolsApp.tsx` | RunDesk analytics + Joined Results + Ammo planner |

Both load the same `index.html`; `src/main.tsx` picks the root from `getCurrentWindow().label`.

## Stack

- **Shell:** Tauri 2, plugins: opener, process, updater, window-state
- **Frontend:** React 19, TypeScript, Vite 7, Tailwind 4, Zustand (overlay store)
- **Backend:** Rust modules under `src-tauri/src/`
- **Persistence:** SQLite via sqlx (`src-tauri/src/db.rs`)
- **Version:** root `VERSION` (currently `0.1.3`) synced via `npm run sync-version`

## Overlay path

1. `watch` follows newest `Fleet_*.txt` for the configured **Listener**.
2. `parse` / `board` / `site_id` build Board sites from exact single-char tags (`0-9` / `a-z`).
3. Sites expire on a 20-minute timer; **Ran** / **Clear** persist in SQLite (`ran_marks`, `cleared_marks`).
4. UI hydrates via `get_board` / `get_overlay_settings`, listens to `board-updated`, ticks locally every 1s, refreshes Board every 5s.
5. **Phase / Clearable** use dual adapters (ADR-0001): Rust gates Clear; UI `derivePhase` for snappy unlock.

## Tools / RunDesk path

1. `RunDesk` (Rust + `useRunDesk` / `lib/runDesk.ts`) owns trays, analyze, focus, amend, reenrich, deletes.
2. Every op returns **EditionFocus** (trays, catalog, scope, report, enrichment, diagnostics).
3. **Enrichment pipeline** (`enrichment_pipeline`, `gamelog_scan`, `gamelog_parse`, `enrichment`) is separate from wallet analytics math and from RunDesk session ownership.
4. **Joined Results** (`joinedResults.ts` + `JoinedResultsView`) joins wallet report × enrichment; **missileRates** owns Hit%/Miss%/expended display math.
5. **Ammo planner** is a Tools tab outside RunDesk; durable launcher prefs live in Tools settings.

## Settings seams

- **Overlay settings:** Listener, chatlogs dir, always-on-top — may refresh Board.
- **Tools settings:** gamelogs dir, FC character, ammo launchers / per launcher — must **not** refresh Board.
- Enrichment uses a run-time **EnrichmentInputs** bag (not full AppSettings).

## SQLite (high level)

- `settings` — overlay + tools columns (single row `id = 1`)
- `ran_marks` / `cleared_marks` — overlay Board marks
- `spawns` — INC constellation metadata (Manifest)
- `analytics_runs` — sealed runs + `report_json` + optional `enrichment_json`

## Important module map (Rust)

| Module | Responsibility |
|--------|----------------|
| `commands` | Tauri commands + app bootstrap |
| `state` / `watch` / `board` | Overlay live state |
| `run_desk` | Tools document session |
| `db` | Persistence + migrations |
| `wallet_parse` / `spawn_parse` / `vanguard_payouts` | Analyze inputs |
| `enrichment*` / `gamelog_*` / `timing` | Enrichment |
| `analytics_types` | EditionFocus / ReportScope / trays |

## Frontend module map

| Area | Path |
|------|------|
| Overlay store | `src/store.ts` |
| Overlay UI | `App.tsx`, `SiteRow.tsx`, `SettingsPanel.tsx` |
| Tools shell | `tools/ToolsApp.tsx`, `useRunDesk.ts` |
| Results / ammo | `JoinedResultsView.tsx`, `AmmoPlannerTab.tsx` |
| Display math | `lib/missileRates.ts`, `lib/joinedResults.ts`, `lib/formatAnalytics.ts` |
| Updater UX | `hooks/useAppUpdater.ts`, `lib/updateSession.ts`, `UpdateModals.tsx` |

## Docs of record

- Domain language: `CONTEXT.md`
- Design/plans: `docs/superpowers/`
- ADR: `docs/adr/0001-dual-adapters-site-phase.md`
- Release: `README.md` + `.github/workflows/release.yml`
