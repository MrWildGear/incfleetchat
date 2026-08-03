# Combat→Payout & Tools UI Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace enrichment in-site clocks with Combat→payout, aggregate Spawn/Overall enrichment (merged listeners), and polish Tools UI (listener panel chrome + `$#,###` / `#,###` formatting + diagnostics placement).

**Architecture:** Change `EnrichmentSnapshot` shape in Rust+TS; recompute combat→payout in `enrichment::align_gap` / `enrich_run`; add pure `aggregate_enrichments` used by `RunDesk::enrichment_for_scope`; update `ToolsApp` panels and formatters. Stale in-site JSON fails deserialize → needs Re-enrich.

**Tech Stack:** Tauri v2, Rust (chrono, serde, sqlx), React Tools window, Vitest for format helpers.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-08-02-analytics-tools-combat-payout-design.md`
- Remove public `in_site_seconds` / `avg_in_site_seconds`; add `combat_to_payout_seconds: Option<i64>` (site + totals nullable)
- Combat→payout uses fused warp stream; combat lines from **FC log only** (not fleet-earliest)
- Spawn/Overall enrichment: enrichment-only aggregate — **no** wallet-gap fill
- Merge missiles by exact Listener name; do **not** sum `missiles_per_cycle` (latest wins)
- ISK/liquid → `$#,###`; counts → `#,###`; LP no `$`
- Enrich diagnostics in top strip only — not Summary
- No ESI; TDD at seams; Conventional Commits
- Always return `EditionFocus` from RunDesk ops

## File map

| File | Responsibility |
|------|----------------|
| `src-tauri/src/analytics_types.rs` | Snapshot field rename / Option combat totals |
| `src-tauri/src/enrichment.rs` | Combat→payout alignment + totals |
| `src-tauri/src/run_desk.rs` | `aggregate_enrichments`; scope load; stale warn |
| `src-tauri/src/db.rs` | Optional: list enrichment rows for spawn/overall (or reuse existing report loaders + per-id load) |
| `src/lib/analyticsTypes.ts` | Mirror types |
| `src/lib/formatAnalytics.ts` | `formatIskMoney`, `formatCount` (testable) |
| `src/lib/formatAnalytics.test.ts` | Formatter tests |
| `src/tools/ToolsApp.tsx` | Panels, labels, formatters, diagnostics move |

---

### Task 1: Snapshot types (Rust + TS)

**Files:**
- Modify: `src-tauri/src/analytics_types.rs`
- Modify: `src/lib/analyticsTypes.ts`

**Interfaces:**
- Produces:
  - `EnrichmentSite { occurred_at, warp_seconds, combat_to_payout_seconds: Option<i64>, is_break, source }`
  - `EnrichmentTotals { warp_seconds, combat_to_payout_seconds: Option<i64>, avg_combat_to_payout_seconds: Option<f64>, fleet_dead }`

- [ ] **Step 1: Update Rust structs** — replace `in_site_seconds` / `avg_in_site_seconds` with combat fields as above. No serde aliases for old in-site (stale = deserialize fail by design).

- [ ] **Step 2: Mirror in `analyticsTypes.ts`**

- [ ] **Step 3: Fix compile breakages minimally** — touch only what won’t compile (`enrichment.rs`, `ToolsApp.tsx`, tests). Prefer leaving failing enrichment tests red for Task 2 rather than inventing combat values.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/analytics_types.rs src/lib/analyticsTypes.ts src-tauri/src/enrichment.rs src/tools/ToolsApp.tsx
git commit -m "refactor(analytics): replace in-site fields with combat_to_payout"
```

---

### Task 2: Combat→payout in enrichment engine

**Files:**
- Modify: `src-tauri/src/enrichment.rs`
- Test: `src-tauri/src/enrichment.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: Task 1 types; existing `ListenerLog`, `align_gap` helpers, `is_end_marker`
- Produces: `align_gap(...) -> (warp_seconds: i64, combat_to_payout_seconds: Option<i64>, is_break: bool, source: EnrichmentSource)`
- `enrich_run` fills sites/totals per spec

**Algorithm (implement exactly):**

```text
If first site with no gap_start → combat = None, warp = 0
Else align_gap:
  Build warp_starts (FC, else borrow) as today
  If warp_starts empty:
    gap_seconds = gap_end - gap_start
    if gap_seconds > break_threshold → (0, None, true, Heuristic)
    else clear_start = gap_start; compute combat from FC log only; (0, combat, false, Heuristic)
  Else:
    Compute warp_seconds as today (segment ends via FC/borrow end markers)
    clear_start = end of last warp segment (same segment_end logic as final iteration)
    combat = first FC-log CombatHit|CombatAny with clear_start <= t < gap_end
    (warp, combat, false, last_source)
```

Totals: sum warp over non-break sites; combat total/avg only over `Some` values; both combat total fields `None` when count is 0.

- [ ] **Step 1: Write failing tests** in `enrichment.rs`:

```rust
#[test]
fn combat_to_payout_from_first_fc_combat_after_warp() {
    // gap 20:00–20:08; warp 20:00:20→20:02:30 (130s); FC combat at 20:03:00 → payout
    // expect warp 130, combat Some(300)  // 20:08:00 - 20:03:00
}

#[test]
fn combat_null_on_break_and_when_no_combat() { /* … */ }

#[test]
fn first_site_without_run_start_has_null_combat() { /* … */ }
```

Update existing tests that assert `in_site_seconds` (e.g. `worked_example_borrowed_warp_130_330`) to expect combat→payout for that scenario (recompute from fixture events; if fixture has no FC combat, add one).

- [ ] **Step 2: Run**

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib enrichment -- --nocapture
```

Expected: FAIL on new assertions / missing fields.

- [ ] **Step 3: Implement** `align_gap` + `enrich_run` totals as above. Remove in-site accumulation.

- [ ] **Step 4: Run tests** — Expected: PASS for enrichment module.

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(enrichment): compute combat-to-payout instead of in-site"
```

---

### Task 3: Spawn/Overall enrichment aggregate + stale load

**Files:**
- Modify: `src-tauri/src/run_desk.rs` (prefer pure `fn aggregate_enrichments(...)` in this file or small `enrichment_aggregate.rs` if `run_desk` is too large — only split if needed)
- Modify: `src-tauri/src/db.rs` if you need `list_run_ids_for_spawn` / reuse existing loaders
- Test: `run_desk` integration tests and/or unit tests on `aggregate_enrichments`

**Interfaces:**
- Produces:

```rust
pub fn aggregate_enrichments(
    runs: &[(String /* run_id */, EnrichmentSnapshot)],
    /// total runs in scope (enriched + missing/stale)
    scope_run_count: usize,
) -> EnrichmentSnapshot
```

Merge rules: concat sites; merge missiles by listener (sum reload/hits/dead; `missiles_per_cycle` from last run in `runs` order); recompute totals; union listeners; union diagnostics + warn `"{enriched} of {scope_run_count} runs lack enrichment"` when `runs.len() < scope_run_count`; `resolved_fc` = last run’s.

- Consumes: `enrichment_for_scope` rewritten:
  - `ReportScope::Run` → `load_enrichment`; if JSON present but parse fails → `None` + diagnostic on focus (top strip via existing diagnostics merge): `"Enrichment needs Re-enrich (schema outdated)"`
  - Spawn/Overall → load all run_ids in scope; try load each enrichment; pass successful ones to `aggregate_enrichments` in catalog order (latest last)

- [ ] **Step 1: Failing unit test** for merge:

```rust
#[test]
fn aggregate_sums_same_listener_and_keeps_latest_cycle() {
    // two snapshots, listener "A": cycles 1+2, hits 10+20, dead 5+7, cycle 156 then 200
    // expect cycles 3, hits 30, dead 12, missiles_per_cycle 200
}
```

- [ ] **Step 2: Implement `aggregate_enrichments` + wire `enrichment_for_scope`**

- [ ] **Step 3: Integration test** — two analyze/enrich paths in one spawn (or construct DB rows) → focus Spawn → merged missiles / summed warp.

- [ ] **Step 4: Update** `analyze_persists_and_attaches_gamelog_enrichment` assertions for combat fields.

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(analytics): aggregate spawn/overall enrichment snapshots"
```

---

### Task 4: Format helpers + Tools UI

**Files:**
- Create: `src/lib/formatAnalytics.ts`
- Create: `src/lib/formatAnalytics.test.ts`
- Modify: `src/tools/ToolsApp.tsx`

**Interfaces:**
- Produces:

```ts
export function formatIskMoney(n: number): string; // "$1,234"
export function formatCount(n: number): string;    // "1,234"
export function formatLp(n: number): string;       // "1,234" no dollar
```

- [ ] **Step 1: Failing Vitest**

```ts
import { describe, expect, it } from "vitest";
import { formatIskMoney, formatCount, formatLp } from "./formatAnalytics";

describe("formatAnalytics", () => {
  it("formats ISK with dollar and commas", () => {
    expect(formatIskMoney(1234)).toBe("$1,234");
  });
  it("formats counts with commas only", () => {
    expect(formatCount(1234)).toBe("1,234");
  });
  it("formats LP without dollar", () => {
    expect(formatLp(1234)).toBe("1,234");
  });
});
```

- [ ] **Step 2: Implement formatters**; replace `formatIsk` usages for ISK/liquid/net with `formatIskMoney`; LP with `formatLp`; listener hits/dead/reloads/`fleet_dead` with `formatCount`.

- [ ] **Step 3: UI structure**
  - `showListeners` state + toolbar button “Listeners” next to drill-down (both independent).
  - Move missile table into panel matching site detail chrome (sticky title “Listeners (N)”, Close).
  - Drilldown: replace In-site column with Combat→payout (`formatDuration` or `—` when null).
  - Summary enrichment: Warp; Combat→payout; Avg combat→payout; Dead (`formatCount`); Resolved FC optional; **remove** enrich diagnostics list from Summary.
  - Ensure enrich diagnostics render only in the top Analytics diagnostics strip (reuse existing `enrichDiagnostics` / focus diagnostics near Gamelogs).

- [ ] **Step 4: Run**

```bash
npm test && npm run build
cargo test --manifest-path src-tauri/Cargo.toml --lib -- --skip live_hamilton
```

Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add src/lib/formatAnalytics.ts src/lib/formatAnalytics.test.ts src/tools/ToolsApp.tsx
git commit -m "feat(tools): listener panel, combat labels, and money formatting"
```

---

### Task 5: Verify & polish

- [ ] **Step 1:** `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --skip live_hamilton`
- [ ] **Step 2:** `npm test && npm run build`
- [ ] **Step 3:** Fix warnings introduced by this work only
- [ ] **Step 4:** Commit only if fixes needed
- [ ] **Step 5:** Manual smoke note in report: Re-enrich after pull; Spawn with 2 enriched runs shows merged listeners; Summary has no file diagnostics

---

## Spec coverage check

| Spec requirement | Task |
|------------------|------|
| Replace in-site with combat→payout types | 1–2 |
| Combat rule (FC combat, clear_start, null cases) | 2 |
| Spawn/Overall aggregate enrichment-only | 3 |
| Missile merge / cycle not summed | 3 |
| Stale schema → Re-enrich | 3 |
| Listener panel chrome + dual toggles | 4 |
| `$#,###` / `#,###` / LP no `$` | 4 |
| Diagnostics top strip only | 4 |
| Keep wallet Avg site time | 4 (unchanged row) |
| ESI excluded | — |

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-02-analytics-tools-combat-payout.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks  
2. **Inline Execution** — execute tasks in this session with checkpoints  

Which approach?
