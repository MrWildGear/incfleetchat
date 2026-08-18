# AGENTS

Shared operating guidance for AI agents working in this repository.

## Project Navigation

- Core project docs: `README.md`, `CONTEXT.md` (canonical domain language)
- Agent docs hub: `.agents/README.md`
- Durable memory: `.memory-bank/README.md`
- Architecture / entities / deploy notes: `.agents/memory/`
- Design history: `docs/superpowers/`, ADRs in `docs/adr/`

## Working Rules

- Keep changes scoped to the user request.
- Prefer small, verifiable edits over large rewrites.
- Run relevant validation before claiming completion (`npm test`, `cargo test --lib` in `src-tauri` as appropriate).
- Never commit or push unless explicitly asked.

## Domain language

- Use terms from `CONTEXT.md` exactly (Listener, Board, Site, Ran, Phase, Clearable, RunDesk, EditionFocus, Spawn, Enrichment inputs, etc.).
- Do **not** confuse overlay **Site** with analytics **Site**.
- Prefer CONTEXT “Avoid” synonyms when writing code comments, UI copy, and docs.

## Stack conventions

- **Overlay UI:** Zustand in `src/store.ts`; Board updates via `board-updated` + periodic `refresh_board`.
- **Tools UI:** RunDesk ops return **EditionFocus**; do not invent a parallel session model in React.
- **Phase / Clearable:** Dual adapters (ADR-0001) — Rust gates Clear; UI `derivePhase` for display. Do not collapse to client-only or Rust-only Phase.
- **Settings:** Overlay settings may refresh the Board; Tools settings must **not**. Enrichment uses **EnrichmentInputs**, not full AppSettings.
- **Missile display math:** Only via `src/lib/missileRates.ts` (expended, dead volleys/missiles, Hit %, Miss %). Callers must not re-derive those.
- **Ammo planner:** Outside RunDesk; durable launcher prefs in Tools settings; ephemeral stock/ship/reload stay on the tab.
- **Persistence:** SQLite via `src-tauri/src/db.rs`; additive `ensure_column` migrations for older DBs.
- **Windows:** `main` = overlay (`App.tsx`), `tools` = Tools (`ToolsApp.tsx`); routed in `src/main.tsx` by window label.
- **Version:** Edit root `VERSION`, then `npm run sync-version` before release commits.

## Session Practice

- Record durable architecture or workflow facts in `.agents/memory/` (update `last_verified`).
- Keep daily work notes in `.agents/sessions/` using the session template.
- Promote long-lived project knowledge into `.memory-bank/` files.
- After meaningful work, update `.memory-bank/progress.md` and open items in `checklist.md`.
