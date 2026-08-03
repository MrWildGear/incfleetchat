# Analytics Metric Parity Implementation Plan

> **For agentic workers:** TDD on `timing.rs`, then UI. Checkbox steps.

**Goal:** Fleet vs character liquid, dual clocks, sheet-aligned \$/hr.

**Architecture:** Extend `SiteDetail`/`SessionSummary` in Rust; Tools Summary maps new fields.

**Tech Stack:** Existing Rust + React Tools.

## Global Constraints

- `fleet_isk` on every site; hourly/session sum fleet fields  
- Rates ÷ `active_site_seconds`  
- No fake fleet liquid from old character-only JSON  

### Task 1: timing.rs TDD

- [ ] Failing test: 2×15M, fleet 15 → character 30M, fleet 450M; rates use active  
- [ ] Add `fleet_isk`, new session fields; remove `liquid_isk` / `time_spent_seconds`  
- [ ] Fix `merge_reports` to sum `fleet_isk`  
- [ ] `cargo test timing`

### Task 2: DB catalog + UI

- [ ] `save_run` stores fleet liquid in `liquid_isk` column  
- [ ] Update TS types + ToolsApp Summary  
- [ ] `npm test` + `npm run build` + `cargo test --lib -- --skip live_hamilton`
