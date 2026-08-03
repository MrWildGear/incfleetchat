# Gamelog Enrichment (v1.5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After a sealed wallet run, scan local EVE Gamelogs and enrich the run with warp vs in-site timing and dead-missile stats (per Listener + fleet), with Re-enrich.

**Architecture:** Enrich-on-Analyze in Rust (`gamelog_parse` → `gamelog_scan` → `enrichment`), persist `enrichment_json` on `analytics_runs`, surface via `EditionFocus`. Tools Analytics UI shows new summary/drill-down and Re-enrich. No ESI. Reuse `encoding::read_chatlog` for locked files.

**Tech Stack:** Tauri v2, Rust (chrono, regex, sqlx), React Tools window, existing RunDesk / Ammo settings.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-08-02-gamelog-enrichment-v15-design.md`
- Listener identity = trimmed `Listener:` header (case-sensitive as EVE writes it)
- Timing markers prefer FC; missile hits/reloads always per-Listener
- Break heuristic only when no warp markers in gap
- First site without `run_start`: no leading warp/in-site; exclude from avg denominators
- `missiles_per_cycle = launchers × ammo_per_launcher` from Ammo settings at enrich time
- No live Gamelog watch; no ESI in this plan
- Always return `EditionFocus` from RunDesk ops
- TDD: red → green per seam; `cargo test --manifest-path src-tauri/Cargo.toml --lib <filter>`

## File map

| File | Responsibility |
|------|----------------|
| `src-tauri/src/gamelog_parse.rs` | Header + line events |
| `src-tauri/src/gamelog_scan.rs` | Pick files in wallet time window |
| `src-tauri/src/enrichment.rs` | FC, borrow, align sites, dead missiles |
| `src-tauri/src/analytics_types.rs` | `EnrichmentSnapshot`, amend op, EditionFocus field |
| `src-tauri/src/db.rs` | `enrichment_json` column + save/load |
| `src-tauri/src/run_desk.rs` | Auto-enrich after analyze; reenrich |
| `src-tauri/src/commands.rs` | Wire reenrich + settings if needed |
| `src-tauri/src/lib.rs` | `mod` declarations |
| `src-tauri/tests/fixtures/gamelog_sample.txt` | Fixture for parser/scan tests |
| `src/lib/analyticsTypes.ts` | Mirror enrichment types |
| `src/tools/ToolsApp.tsx` | UI: path, FC, Re-enrich, summary, drill-down |
| `src/lib/ammo.ts` | Already has cycle inputs — pass into enrich IPC |

---

### Task 1: Gamelog parser

**Files:**
- Create: `src-tauri/src/gamelog_parse.rs`
- Create: `src-tauri/tests/fixtures/gamelog_sample.txt`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces:
  - `parse_listener(text) -> Option<String>`
  - `parse_session_started(text) -> Option<DateTime<Utc>>`
  - `parse_gamelog_events(text) -> Vec<GamelogEvent>`
  - `GamelogEvent { occurred_at, kind: FollowingWarp | Regrouping | CombatHit | Reload | CombatAny, raw_hint }`

- [ ] **Step 1: Write fixture** `gamelog_sample.txt` with header `Listener: Chelien Alabel Maricadie`, `Session Started: 2026.08.02 18:20:40`, and lines:
  - Following in warp
  - Regrouping
  - missile Hits combat
  - Loading … approximately 35 seconds
  - run out of charges (must NOT double-count reload)
  - a `(question)` line (ignored)

- [ ] **Step 2: Write failing tests** in `gamelog_parse.rs` `#[cfg(test)]`:
  - parses listener and session started
  - classifies following_warp, regrouping, combat_hit, reload (one), combat_any
  - ignores question lines

- [ ] **Step 3: Run** `cargo test --manifest-path src-tauri/Cargo.toml --lib gamelog_parse`  
  Expected: FAIL (module missing)

- [ ] **Step 4: Implement** parser using regex similar to chatlog timestamps:  
  `^\u{feff}?\[ (\d{4}\.\d{2}\.\d{2} \d{2}:\d{2}:\d{2}) \] \((\w+)\) (.*)$`  
  Rules:
  - channel `notify` + body contains `Following` and `in warp` → `FollowingWarp`
  - channel `notify` + `Regrouping` → `Regrouping`
  - channel `notify` + `Loading the` and `Missile Launcher` → `Reload`
  - channel `combat` + body contains `Hits` → `CombatHit` (also counts as combat_any for timing via kind or dual emit — prefer single `CombatHit` that enrichment treats as both hit and combat_any end)
  - channel `combat` without Hits → `CombatAny`
  - Do not emit `Reload` for `run out of charges` alone

- [ ] **Step 5: Run tests** — Expected: PASS

- [ ] **Step 6: Commit**
```bash
git add src-tauri/src/gamelog_parse.rs src-tauri/src/lib.rs src-tauri/tests/fixtures/gamelog_sample.txt
git commit -m "feat(enrichment): parse EVE gamelog warp, combat, and reload events"
```

---

### Task 2: Gamelog scan window

**Files:**
- Create: `src-tauri/src/gamelog_scan.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `gamelog_parse::{parse_listener, parse_session_started, parse_gamelog_events}`, `encoding::read_chatlog`
- Produces:
  - `default_gamelogs_dir() -> PathBuf`
  - `scan_gamelogs(dir, wallet_start, wallet_end) -> ScanResult`
  - `ScanResult { logs: Vec<ListenerLog>, diagnostics: Vec<String> }`
  - `ListenerLog { listener: String, path: PathBuf, events: Vec<GamelogEvent> }`
  - Expand `wallet_start` by 1 local calendar day for inclusion; newest→oldest; skip unreadable/locked with diagnostic

- [ ] **Step 1: Failing test** with `tempdir`: write two fake gamelog files (different session starts); assert only overlapping file included; assert locked/missing handled via diagnostic when path missing

- [ ] **Step 2: Run** — Expected: FAIL

- [ ] **Step 3: Implement** scan:
  - List files in dir (non-recursive)
  - Sort by mtime descending (or filename)
  - For each: `read_chatlog`; parse session; if session ≥ expanded_start AND session ≤ wallet_end (+ small slack), parse events and push
  - Stop early once files are entirely older than expanded_start (optional optimization; correctness first)

- [ ] **Step 4: Run tests** — PASS

- [ ] **Step 5: Commit**
```bash
git add src-tauri/src/gamelog_scan.rs src-tauri/src/lib.rs
git commit -m "feat(enrichment): scan Gamelogs by wallet time window"
```

---

### Task 3: Enrichment engine (FC, align, missiles)

**Files:**
- Create: `src-tauri/src/enrichment.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `ListenerLog`, wallet site timestamps from `AnalyticsReport.sites` / payout times, `RunSettings.break_threshold_minutes`, `run_start`, `missiles_per_cycle: u32`, `fc_character: Option<String>`, wallet description hint
- Produces: `enrich_run(...) -> EnrichmentSnapshot` (types live in `analytics_types.rs` — add in this task or Task 4)

```rust
pub fn enrich_run(
    logs: &[ListenerLog],
    site_times: &[DateTime<Utc>], // sorted payout times
    break_threshold_minutes: u32,
    run_start: Option<DateTime<Utc>>,
    missiles_per_cycle: u32,
    fc_character: Option<&str>,
    wallet_fc_hint: Option<&str>,
) -> EnrichmentSnapshot;
```

- [ ] **Step 1: Add** `EnrichmentSnapshot` (+ nested types) to `analytics_types.rs` matching the spec TypeScript shape (serde rename snake_case)

- [ ] **Step 2: Failing test — worked example** from spec:
  - Sites: 20:00:00, 20:08:00
  - FC listener: combat at 20:02:30 only
  - Alt: following_warp at 20:00:20
  - Assert warp_seconds=130, in_site_seconds=330, source=`borrowed`

- [ ] **Step 3: Failing test — break heuristic** without markers, gap 40m, threshold 25 → `is_break`

- [ ] **Step 4: Failing test — dead missiles** one listener: 2 reloads, 100 hits, cycle 156 → dead = 2*156-100 = 212

- [ ] **Step 5: Failing test — first site no run_start** → first site warp/in_site 0 or excluded from totals avg denominator

- [ ] **Step 6: Implement** `resolve_fc`, borrow, alignment, missile loop per spec

- [ ] **Step 7: Run** `cargo test --manifest-path src-tauri/Cargo.toml --lib enrichment` — PASS

- [ ] **Step 8: Commit**
```bash
git add src-tauri/src/enrichment.rs src-tauri/src/analytics_types.rs src-tauri/src/lib.rs
git commit -m "feat(enrichment): align warp/in-site and compute dead missiles"
```

---

### Task 4: DB + RunDesk wire-up

**Files:**
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/run_desk.rs`
- Modify: `src-tauri/src/analytics_types.rs` (`AmendOp::ReenrichRun`, `EditionFocus.enrichment`)
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/types.rs` or settings storage for `gamelogs_dir` / `fc_character` (prefer columns on `settings` or JSON blob — add nullable TEXT columns)

**Interfaces:**
- `Db::save_enrichment(run_id, &EnrichmentSnapshot)`
- `Db::load_enrichment(run_id) -> Option<EnrichmentSnapshot>`
- After `analyze` succeeds: call enrich (if dir exists), save, attach to focus
- `amend(ReenrichRun { run_id: Option<String> })` re-runs enrich for sealed/current run
- Pass `missiles_per_cycle` from command arg or stored ammo defaults in settings (add `ammo_launchers`, `ammo_per_launcher` integers default 6/26 if not present)

- [ ] **Step 1: Migration** `ALTER TABLE` / recreate-safe: add `enrichment_json TEXT` to `analytics_runs`; add `gamelogs_dir`, `fc_character`, `ammo_launchers`, `ammo_per_launcher` to `settings` (nullable / defaults)

- [ ] **Step 2: Test** round-trip enrichment_json on temp db

- [ ] **Step 3: RunDesk** after save_run, build site_times from report, scan, enrich, save; diagnostics into snapshot

- [ ] **Step 4: Command** `run_desk_reenrich` + extend amend; register in `generate_handler!`

- [ ] **Step 5: Integration test** in `run_desk` tests with fixture gamelogs in tempdir — analyze then assert `focus.enrichment.is_some()`

- [ ] **Step 6: Commit**
```bash
git add src-tauri/src/db.rs src-tauri/src/run_desk.rs src-tauri/src/analytics_types.rs src-tauri/src/commands.rs src-tauri/src/types.rs
git commit -m "feat(enrichment): persist and auto-apply gamelog enrichment on Analyze"
```

---

### Task 5: Tools UI

**Files:**
- Modify: `src/lib/analyticsTypes.ts`
- Modify: `src/tools/ToolsApp.tsx`
- Optionally sync ammo localStorage → settings via invoke when enriching (pass launchers/ammoPerLauncher into reenrich/analyze path)

**Interfaces:**
- Display `focus.enrichment` summary + missile table
- Inputs: gamelogs path, FC character (persist via `set_settings`)
- Button **Re-enrich** → `run_desk_reenrich` or amend
- Drill-down columns: warp, in-site, source

- [ ] **Step 1: Extend** TS types for `EnrichmentSnapshot`

- [ ] **Step 2: UI** section under Summary; Re-enrich; settings fields

- [ ] **Step 3: On Analyze/Re-enrich**, send ammo cycle:  
  `invoke("run_desk_reenrich", { missilesPerCycle: launchers * ammoPerLauncher })`  
  (or fold into amend payload)

- [ ] **Step 4:** `npm run build` — PASS

- [ ] **Step 5: Commit**
```bash
git add src/lib/analyticsTypes.ts src/tools/ToolsApp.tsx
git commit -m "feat(tools): show warp/in-site and dead missiles with Re-enrich"
```

---

### Task 6: Verify & polish

- [ ] **Step 1:** `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --skip live_hamilton`
- [ ] **Step 2:** `npm test && npm run build`
- [ ] **Step 3:** Manual smoke: Analyze with sample wallet + point gamelogs_dir at a folder containing the session’s logs; confirm warp/dead numbers
- [ ] **Step 4:** Fix warnings (`unused import` etc.) if introduced
- [ ] **Step 5: Commit** only if fixes needed

---

## Spec coverage check

| Spec requirement | Task |
|------------------|------|
| Gamelog parse (warp/regroup/hit/reload) | 1 |
| Scan window +1 local day | 2 |
| FC resolve + borrow | 3 |
| Site alignment + break rules + worked example | 3 |
| Dead missiles from Ammo cycle | 3, 5 |
| Persist enrichment_json | 4 |
| Auto-enrich + Re-enrich | 4, 5 |
| UI summary/drill-down | 5 |
| ESI | Explicitly excluded |

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-02-gamelog-enrichment-v15.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks  
2. **Inline Execution** — execute tasks in this session with checkpoints  

Which approach?
