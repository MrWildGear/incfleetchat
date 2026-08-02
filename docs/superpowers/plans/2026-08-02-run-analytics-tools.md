# Run Analytics & Ammo Tools Implementation Plan

> **For agentic workers:** Execute task-by-task with TDD. Steps use checkbox syntax.

**Goal:** Add a Tools window with wallet-based run analytics (sheet parity + spawn-tagged runs) and an ammo load planner.

**Architecture:** Hybrid — Rust RunDesk (parse, timing, SQLite, EditionFocus snapshot); React Tools UI + pure-TS ammo math. Timer overlay opens/focuses a second webview.

**Tech Stack:** Tauri v2, React 19, Zustand, SQLite/sqlx, chrono, vitest (ammo), existing Rust `#[cfg(test)]` pattern.

## Global Constraints

- Site = `Corporate Reward Payout` ∧ expected ISK amount
- Within-run dedupe on `(timestamp, amount)`
- Break gaps excluded from time-spent / avg; first site needs run_start for duration
- Constellation is spawn join key
- Always return `EditionFocus` from RunDesk ops
- No ESI / gamelog in v1

## File map

| File | Responsibility |
|------|----------------|
| `src-tauri/src/vanguard_payouts.rs` | Uni table lookup |
| `src-tauri/src/wallet_parse.rs` | Journal line → payout events |
| `src-tauri/src/spawn_parse.rs` | Manifest → spawn fields |
| `src-tauri/src/timing.rs` | Gaps, breaks, hourly, summary |
| `src-tauri/src/analytics_types.rs` | DTOs / EditionFocus |
| `src-tauri/src/run_desk.rs` | Session state + analyze/focus |
| `src-tauri/src/db.rs` | Migrations + spawn/run CRUD |
| `src-tauri/src/commands.rs` | IPC + open tools window |
| `src/lib/ammo.ts` | Ammo formulas |
| `src/tools/ToolsApp.tsx` | Analytics + Ammo tabs |
| `src/main.tsx` | Route by window label |

### Task 1: Vanguard payouts

**Produces:** `lookup_payout(space, fleet_size) -> PayoutTicket { isk, lp_per_char }`

- [ ] Test low/null 15 → 15_000_000 / 2000; highsec 10 → 10_395_000 / 1400; low/null 16 → 13_875_000 / 1850
- [ ] Implement table
- [ ] `cargo test vanguard_payouts -- --nocapture`

### Task 2: Wallet parser

**Produces:** `parse_wallet_journal(text, expected_isk) -> ParseResult { events, ignored, duplicates_dropped }`

- [ ] Test sample Corporate Reward Payout lines; ignore wrong amount / wrong type; dedupe
- [ ] Implement
- [ ] `cargo test wallet_parse`

### Task 3: Manifest parser

**Produces:** `parse_manifest(text) -> SpawnDraft`

- [ ] Test Kundalini-style paste extracts constellation `4MY-AB`, region, systems
- [ ] Implement
- [ ] `cargo test spawn_parse`

### Task 4: Timing engine

**Produces:** `build_report(events, settings) -> Report { sites, hourly, session }`

- [ ] Test gaps, break threshold, run_start, hourly bucket
- [ ] Implement
- [ ] `cargo test timing`

### Task 5: DB + RunDesk

**Produces:** RunDesk open/paste/analyze/focus/amend → EditionFocus

- [ ] Migrate tables; persist run; aggregate overall/spawn
- [ ] `cargo test run_desk`

### Task 6: Ammo TS

**Produces:** `computeAmmoLoad(input) -> { missilesPerCycle, loadIntoShip, sites }`

- [ ] Vitest: 1e6/14 → 71429; 6×26=156; sites ≈ 208
- [ ] Implement

### Task 7: Tools window UI + IPC

- [ ] Register commands; create/focus tools webview; ToolsApp tabs; header button
- [ ] `npm run build` + `cargo test`

## Execution

Inline in this session (user requested full todo completion).
