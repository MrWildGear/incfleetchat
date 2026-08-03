# Fleet Timing, Re-enrich Scope & Listeners Filter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Time Warp/Combat→payout from a fleet-fused gamelog timeline (10s warp debounce + coalesce), enable Re-enrich by catalog scope after reopen, and hide all-zero listener rows.

**Architecture:** Rewrite `align_gap` in `enrichment.rs` to use the full listener event pool; add `EnrichmentSource::Fleet`; change `AmendOp::ReenrichRun` / RunDesk so `None` means “all runs in current scope” (not `sealed_run_id`); filter zero missile rows in Tools UI. Gap column stays removed.

**Tech Stack:** Tauri v2, Rust (`chrono`, `serde`), React Tools window, Vitest for the listener filter helper, `cargo test` for enrichment/run_desk.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-08-02-fleet-timing-reenrich-design.md`
- Supersedes combat-payout timing: fleet pool; Combat→payout starts on **`CombatHit` only** (not `CombatAny`)
- Warp end markers remain `Regrouping` | `CombatHit` | `CombatAny` (fleet-wide)
- Warp: coalesce overlapping segments; debounce accepted starts by **10 seconds** (hardcoded)
- Source emit: `fleet` | `heuristic`; keep deserializing legacy `fc` / `borrowed`
- Re-enrich enable: Run scope **or** Spawn/Overall with ≥1 catalog run — **not** `sealed_run_id`
- Spawn/Overall re-enrich: best-effort; keep prior snapshot on failure; top-strip warnings
- Listeners: hide when `reload_cycles === 0 && hits === 0 && dead === 0`; count = visible
- No wallet Duration / Avg site time changes; no configurable debounce; TDD; Conventional Commits
- Always return `EditionFocus` from RunDesk ops

## File map

| File | Responsibility |
|------|----------------|
| `src-tauri/src/analytics_types.rs` | Add `EnrichmentSource::Fleet`; AmendOp doc comment |
| `src/lib/analyticsTypes.ts` | Mirror `"fleet"` in `EnrichmentSource` |
| `src-tauri/src/enrichment.rs` | Rewrite `align_gap` + combat helper; update tests |
| `src-tauri/src/run_desk.rs` | Scope-wide re-enrich; diagnostics; tests |
| `src/lib/missileActivity.ts` | Pure `hasMissileActivity` filter (testable) |
| `src/lib/missileActivity.test.ts` | Filter unit tests |
| `src/tools/ToolsApp.tsx` | canReenrich, reenrich call, listeners filter/count, Gap already gone |

---

### Task 1: `EnrichmentSource::Fleet` (Rust + TS)

**Files:**
- Modify: `src-tauri/src/analytics_types.rs`
- Modify: `src/lib/analyticsTypes.ts`

**Interfaces:**
- Produces: `EnrichmentSource { Fc, Borrowed, Heuristic, Fleet }` with serde `snake_case` (`"fleet"`)
- Legacy `fc` / `borrowed` remain for deserialize

- [ ] **Step 1: Add variant in Rust**

In `analytics_types.rs`, update the enum to:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnrichmentSource {
    Fc,
    Borrowed,
    Heuristic,
    Fleet,
}
```

Update the doc comment on the enum to note new snapshots emit `fleet` | `heuristic`; `fc` / `borrowed` are legacy.

- [ ] **Step 2: Mirror in TypeScript**

```ts
export type EnrichmentSource = "fc" | "borrowed" | "heuristic" | "fleet";
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/analytics_types.rs src/lib/analyticsTypes.ts
git commit -m "feat(analytics): add EnrichmentSource::Fleet"
```

---

### Task 2: Fleet `align_gap` (TDD)

**Files:**
- Modify: `src-tauri/src/enrichment.rs`
- Test: `src-tauri/src/enrichment.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: `ListenerLog`, `GamelogEventKind`, `EnrichmentSource::{Fleet, Heuristic}`
- Produces: `align_gap(...) -> (warp_seconds: i64, combat_to_payout_seconds: Option<i64>, is_break: bool, source: EnrichmentSource)`
- Constant: `const WARP_START_DEBOUNCE_SECS: i64 = 10;`

**Algorithm (implement exactly):**

```text
all_gap_events = flatMap listeners → events_in_gap (gap_start, gap_end]

raw_starts = FollowingWarp in all_gap_events, sorted ascending
accepted_starts = []
for s in raw_starts:
  if accepted_starts empty OR (s - last_accepted) >= 10s:
    push s

if accepted_starts empty:
  gap_seconds = gap_end - gap_start
  if gap_seconds > break_threshold → (0, None, true, Heuristic)
  else clear_start = gap_start
       combat = earliest CombatHit in all_gap_events with clear_start <= t < gap_end
       return (0, combat, false, Heuristic)

segments = []
for each accepted start:
  end = min(
    next accepted start (if any),
    earliest is_end_marker after start in all_gap_events,
    gap_end
  )
  segments.push([start, end])

coalesce: sort by start; merge while next.start <= cur.end
warp_seconds = sum (end - start) of coalesced
clear_start = last coalesced end
combat = earliest CombatHit in all_gap_events with clear_start <= t < gap_end
return (warp_seconds, combat, false, Fleet)
```

Notes:
- `is_combat_kind` for **Combat→payout** must be **`CombatHit` only** (change from Hit|Any).
- `is_end_marker` unchanged (Regrouping|Hit|Any).
- Drop FC-preferred / borrowed warp-start branching; `resolved_fc` still computed for Summary but unused by `align_gap`.
- `combat_to_payout` helper should take the **fleet** event pool, not FC-only.

- [ ] **Step 1: Write failing tests** (append in `enrichment.rs` tests; keep helpers `ts` / `ev` / `listener_log`)

```rust
#[test]
fn fleet_same_jump_two_listeners_counts_one_warp() {
    // Both warp at 20:00:20; FC lands via Regrouping 20:02:30; Alt CombatHit 20:03:00; payout 20:08:00
    // Prev site 20:00:00. Warp = 130s (not 260). Source Fleet. Combat→payout from Alt hit = 300s.
    let fc = listener_log(
        "FC Pilot",
        vec![
            ev(20, 0, 20, GamelogEventKind::FollowingWarp),
            ev(20, 2, 30, GamelogEventKind::Regrouping),
        ],
    );
    let alt = listener_log(
        "Alt Pilot",
        vec![
            ev(20, 0, 21, GamelogEventKind::FollowingWarp), // within 10s → debounce
            ev(20, 3, 0, GamelogEventKind::CombatHit),
        ],
    );
    let snap = enrich_run(
        &[fc, alt],
        &[ts(20, 0, 0), ts(20, 8, 0)],
        25,
        Some(ts(19, 55, 0)),
        156,
        Some("FC Pilot"),
        None,
    );
    let site = &snap.sites[1];
    assert_eq!(site.warp_seconds, 130);
    assert_eq!(site.combat_to_payout_seconds, Some(300));
    assert_eq!(site.source, EnrichmentSource::Fleet);
}

#[test]
fn combat_any_does_not_start_combat_to_payout() {
    let fc = listener_log(
        "FC Pilot",
        vec![
            ev(20, 1, 0, GamelogEventKind::FollowingWarp),
            ev(20, 2, 0, GamelogEventKind::Regrouping),
            ev(20, 2, 30, GamelogEventKind::CombatAny), // incoming — must NOT start clock
        ],
    );
    let snap = enrich_run(
        &[fc],
        &[ts(20, 0, 0), ts(20, 8, 0)],
        25,
        Some(ts(19, 55, 0)),
        156,
        Some("FC Pilot"),
        None,
    );
    let site = &snap.sites[1];
    assert_eq!(site.warp_seconds, 60);
    assert_eq!(site.combat_to_payout_seconds, None);
    assert_eq!(site.source, EnrichmentSource::Fleet);
}

#[test]
fn combat_from_non_fc_hit_when_fc_quiet() {
    let fc = listener_log(
        "FC Pilot",
        vec![
            ev(20, 1, 0, GamelogEventKind::FollowingWarp),
            ev(20, 2, 0, GamelogEventKind::Regrouping),
        ],
    );
    let alt = listener_log(
        "Alt Pilot",
        vec![ev(20, 3, 0, GamelogEventKind::CombatHit)],
    );
    let snap = enrich_run(
        &[fc, alt],
        &[ts(20, 0, 0), ts(20, 8, 0)],
        25,
        Some(ts(19, 55, 0)),
        156,
        Some("FC Pilot"),
        None,
    );
    let site = &snap.sites[1];
    assert_eq!(site.warp_seconds, 60);
    assert_eq!(site.combat_to_payout_seconds, Some(300));
    assert_eq!(site.source, EnrichmentSource::Fleet);
}

#[test]
fn warp_start_outside_debounce_is_second_segment() {
    // Accepted starts 20:01:00 and 20:01:15 (≥10s) — two segments if each ends before the next
    let fc = listener_log(
        "FC Pilot",
        vec![
            ev(20, 1, 0, GamelogEventKind::FollowingWarp),
            ev(20, 1, 5, GamelogEventKind::Regrouping),
            ev(20, 1, 15, GamelogEventKind::FollowingWarp),
            ev(20, 1, 40, GamelogEventKind::Regrouping),
            ev(20, 2, 0, GamelogEventKind::CombatHit),
        ],
    );
    let snap = enrich_run(
        &[fc],
        &[ts(20, 0, 0), ts(20, 5, 0)],
        25,
        Some(ts(19, 55, 0)),
        156,
        Some("FC Pilot"),
        None,
    );
    let site = &snap.sites[1];
    // 5s + 25s = 30s warp
    assert_eq!(site.warp_seconds, 30);
    assert_eq!(site.combat_to_payout_seconds, Some(180)); // 20:05 - 20:02
    assert_eq!(site.source, EnrichmentSource::Fleet);
}
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cd src-tauri
cargo test --lib fleet_same_jump_two_listeners_counts_one_warp combat_any_does_not_start_combat_to_payout combat_from_non_fc_hit_when_fc_quiet warp_start_outside_debounce_is_second_segment -- --nocapture
```

Expected: FAIL (wrong source / wrong combat / double-counted warp / CombatAny still counting).

- [ ] **Step 3: Implement `align_gap` rewrite**

Minimal changes:
1. Change `is_combat_kind` to `CombatHit` only.
2. Replace FC/borrow warp_starts with fleet pool + 10s debounce + coalesce as above.
3. Point `combat_to_payout` at `all_gap_events` (or renamed `fleet_gap_events`).
4. Emit `EnrichmentSource::Fleet` when accepted starts non-empty.

Keep `events_in_gap`, `earliest_after`, `is_end_marker` unless coalesce needs a small local helper (`coalesce_intervals(segments: Vec<(DateTime, DateTime)>) -> Vec<(DateTime, DateTime)>`).

- [ ] **Step 4: Run new tests — expect PASS**

```bash
cd src-tauri
cargo test --lib fleet_same_jump_two_listeners_counts_one_warp combat_any_does_not_start_combat_to_payout combat_from_non_fc_hit_when_fc_quiet warp_start_outside_debounce_is_second_segment
```

Expected: PASS

- [ ] **Step 5: Update legacy enrichment tests that contradict the new rules**

Update expectations in the same file (do not delete coverage):

| Test | Change |
|------|--------|
| `worked_example_borrowed_warp_130_330` | Rename to fleet wording; `source` → `Fleet`; keep warp 130 / combat 330 (FC has CombatHit) |
| `combat_to_payout_from_first_fc_combat_after_warp` | Change `CombatAny` at 20:03:00 to `CombatHit`; `source` → `Fleet`; keep 130 / 300 |
| `no_markers_short_gap_combat_from_fc_log_only` | Rename; use `CombatHit` not `CombatAny` (or assert `None` if keeping Any — prefer Hit so heuristic combat still works); source stays `Heuristic` |
| Integration assertions using `EnrichmentSource::Fc` / `Borrowed` in this module | → `Fleet` where warp markers exist |
| `run_desk` analyze test expecting `Fc` + combat | Update in Task 3 if it fails after this commit — prefer fixing here if `cargo test --lib enrichment` is enough; leave run_desk for Task 3 |

Also update comments that say “FC log only”.

- [ ] **Step 6: Run full enrichment module tests**

```bash
cd src-tauri
cargo test --lib enrichment::
```

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/enrichment.rs
git commit -m "feat(analytics): fleet-fuse warp and CombatHit combat clocks"
```

---

### Task 3: Scope-wide Re-enrich (RunDesk)

**Files:**
- Modify: `src-tauri/src/analytics_types.rs` (AmendOp doc only)
- Modify: `src-tauri/src/run_desk.rs`
- Test: `src-tauri/src/run_desk.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: `db.list_run_ids_for_spawn`, `db.list_all_run_ids`, existing `reenrich_run`
- Produces: When `AmendOp::ReenrichRun { run_id: None }`, re-enrich **all run ids in current `scope`** (Run → that id if somehow None should not happen; Spawn → spawn ids; Overall → all ids). When `Some(id)`, re-enrich that id only.
- Do **not** fall back to `sealed_run_id`.
- Partial failure: continue; push diagnostics like `"{ok} of {total} re-enriched"` and per-failure `Failed re-enrich {run_id}: {err}` (warn level) onto desk diagnostics before snapshot (or merge into snapshot diagnostics returned via focus — match existing strip pattern: `EditionFocus.diagnostics`).

**AmendOp comment:**

```rust
/// Recompute gamelog enrichment. `Some(run_id)` = that run.
/// `None` = every run in the current report scope (Spawn/Overall = all
/// catalog runs in scope; unused for Run — UI always passes the id).
ReenrichRun { run_id: Option<String> },
```

- [ ] **Step 1: Write failing tests**

```rust
#[tokio::test]
async fn reenrich_none_uses_scope_not_sealed_id() {
    // Seed two runs in one spawn with gamelogs_dir unset so enrich "succeeds"
    // with empty/warn snapshot OR use temp gamelogs like analyze_saves test.
    // Clear sealed_run_id via lock after analyze of run A.
    // Focus Spawn. Amend ReenrichRun { None }.
    // Assert: both runs' enrichment rows refreshed (load_enrichment_status Ok)
    // and diagnostics mention "2 of 2" or similar — adjust to exact message you implement.
}

#[tokio::test]
async fn reenrich_continues_after_one_failure() {
    // Two run ids in spawn; make reenrich_run fail for one by deleting wallet
    // row mid-flight OR patch — simplest: second id not in DB → skip/fail.
    // Prefer: call internal loop with one good id and one missing id via
    // Amend after focusing spawn that lists both — if missing id can't be
    // in list_run_ids, instead force enrich_and_save error by emptying
    // gamelogs and asserting best-effort: first still Ok after amend.
    // Concrete approach used in repo tests: create two sealed runs; for the
    // second, corrupt is hard — assert that ReenrichRun { Some(good) } works
    // with sealed_run_id = None (proves enable path), AND
    // ReenrichRun { None } on Spawn calls both ids (spy via enrichment
    // timestamps or overwrite a sentinel field).
}
```

Use this concrete first test (preferred):

```rust
#[tokio::test]
async fn reenrich_run_with_explicit_id_works_when_sealed_is_none() {
    // Copy setup from analyze_attaches_enrichment (gamelog + wallet + analyze).
    let run_id = /* from focus.scope or catalog.runs[0] */;
    {
        let mut st = desk.inner.lock();
        st.sealed_run_id = None;
    }
    desk.focus(ReportScope::Run { run_id: run_id.clone() }).await.unwrap();
    let after = desk
        .amend(AmendOp::ReenrichRun {
            run_id: Some(run_id.clone()),
        })
        .await
        .unwrap();
    assert!(after.enrichment.is_some());
    assert!(after.sealed_run_id.is_none());
}

#[tokio::test]
async fn reenrich_none_on_spawn_refreshes_all_runs_in_spawn() {
    // Two analyzes / two save_run with enrichment; focus Spawn;
    // sealed_run_id = None; Amend ReenrichRun { None };
    // both load_enrichment_status == Ok; diagnostics contain "2 of 2 re-enriched"
    // (exact string: implement as format!("{} of {} re-enriched", ok, total)).
}
```

- [ ] **Step 2: Run — expect FAIL**

```bash
cd src-tauri
cargo test --lib reenrich_run_with_explicit_id_works_when_sealed_is_none reenrich_none_on_spawn_refreshes_all_runs_in_spawn
```

Expected: FAIL (`None` still uses sealed / spawn no-op).

- [ ] **Step 3: Implement**

Replace `AmendOp::ReenrichRun` arm roughly with:

```rust
AmendOp::ReenrichRun { run_id } => {
    let ids: Vec<String> = match run_id {
        Some(id) => vec![id],
        None => {
            let scope = self.inner.lock().scope.clone();
            match scope {
                ReportScope::Run { run_id } => vec![run_id],
                ReportScope::Spawn { constellation } => self
                    .db
                    .list_run_ids_for_spawn(&constellation)
                    .await
                    .map_err(|e| e.to_string())?,
                ReportScope::Overall => self
                    .db
                    .list_all_run_ids()
                    .await
                    .map_err(|e| e.to_string())?,
            }
        }
    };
    let total = ids.len();
    let mut ok = 0usize;
    let mut diags = Vec::new();
    for id in &ids {
        match self.reenrich_run(id).await {
            Ok(()) => ok += 1,
            Err(e) => diags.push(Diagnostic {
                level: "warn".into(),
                message: format!("Failed re-enrich {id}: {e}"),
            }),
        }
    }
    if total > 0 {
        diags.push(Diagnostic {
            level: if ok == total { "info".into() } else { "warn".into() },
            message: format!("{ok} of {total} re-enriched"),
        });
    }
    self.inner.lock().diagnostics.extend(diags);
}
```

Also fix the existing test `analyze_…` that does `ReenrichRun { run_id: None }` after analyze — it still works if scope is Run (passes run id from scope). Update assertion comments that say “falls back to sealed run”.

Update any run_desk test expecting `EnrichmentSource::Fc` → `Fleet` after Task 2.

- [ ] **Step 4: Run tests**

```bash
cd src-tauri
cargo test --lib reenrich_ run_desk::
```

Expected: PASS (or fix remaining Fc→Fleet assertions).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/analytics_types.rs src-tauri/src/run_desk.rs
git commit -m "fix(analytics): re-enrich all runs in report scope"
```

---

### Task 4: Tools UI — enable Re-enrich, hide zero listeners, Gap gone

**Files:**
- Create: `src/lib/missileActivity.ts`
- Create: `src/lib/missileActivity.test.ts`
- Modify: `src/tools/ToolsApp.tsx`

**Interfaces:**
- Produces: `export function hasMissileActivity(m: Pick<MissileStat, "reload_cycles" | "hits" | "dead">): boolean`
- Returns `true` unless all three are 0

- [ ] **Step 1: Failing Vitest**

```ts
// src/lib/missileActivity.test.ts
import { describe, expect, it } from "vitest";
import { hasMissileActivity } from "./missileActivity";

describe("hasMissileActivity", () => {
  it("hides all-zero rows", () => {
    expect(
      hasMissileActivity({ reload_cycles: 0, hits: 0, dead: 0 }),
    ).toBe(false);
  });
  it("keeps any non-zero reload/hits/dead", () => {
    expect(
      hasMissileActivity({ reload_cycles: 1, hits: 0, dead: 0 }),
    ).toBe(true);
    expect(
      hasMissileActivity({ reload_cycles: 0, hits: 1, dead: 0 }),
    ).toBe(true);
    expect(
      hasMissileActivity({ reload_cycles: 0, hits: 0, dead: 1 }),
    ).toBe(true);
  });
});
```

```ts
// src/lib/missileActivity.ts
import type { MissileStat } from "./analyticsTypes";

export function hasMissileActivity(
  m: Pick<MissileStat, "reload_cycles" | "hits" | "dead">,
): boolean {
  return m.reload_cycles !== 0 || m.hits !== 0 || m.dead !== 0;
}
```

Write the test first with a stub that always returns `true`, run fail, then implement — or write correct impl after red if stub is awkward; prefer empty file + import fail first.

- [ ] **Step 2: Run Vitest — red then green**

```bash
npm test -- src/lib/missileActivity.test.ts
```

- [ ] **Step 3: Wire ToolsApp**

1. Import `hasMissileActivity`.
2. Replace:

```ts
const canReenrich =
  focus?.scope.kind === "run" || Boolean(focus?.sealed_run_id);
```

with:

```ts
const catalogRunCount = focus?.catalog?.runs?.length ?? 0;
const canReenrich =
  focus?.scope.kind === "run" ||
  ((focus?.scope.kind === "spawn" || focus?.scope.kind === "overall") &&
    catalogRunCount > 0);
```

Note: for Spawn, `catalog.runs` may be all runs globally — if the UI catalog lists only scoped runs already, this is correct. If catalog is global, use spawn `run_count` instead:

```ts
const canReenrich =
  focus?.scope.kind === "run" ||
  (focus?.scope.kind === "spawn" && (focus.spawn?.run_count ?? 0) > 0) ||
  (focus?.scope.kind === "overall" && (focus.catalog?.runs?.length ?? 0) > 0);
```

Use the **second** form (spawn uses `focus.spawn.run_count`).

3. `reenrich()` already passes `runId` only for run scope (`undefined` otherwise) — that now triggers scope-wide backend. Keep as-is.

4. Listeners:

```ts
const visibleMissiles =
  focus?.enrichment?.missiles?.filter(hasMissileActivity) ?? [];
```

- Toggle enable / panel open condition: `visibleMissiles.length`
- Title: `Listeners ({visibleMissiles.length})`
- Map `visibleMissiles` in the table body

5. Confirm Gap column is absent (already removed in working tree). If present, delete Gap `<th>` / `<td>` for `gap_seconds`.

- [ ] **Step 4: Manual sanity** (optional if app running): reopen app → Spawn with runs → Re-enrich enabled; Listeners hides zeros.

- [ ] **Step 5: Commit**

```bash
git add src/lib/missileActivity.ts src/lib/missileActivity.test.ts src/tools/ToolsApp.tsx
git commit -m "feat(tools): scope re-enrich enable and hide zero listeners"
```

---

### Task 5: Verify CI gate + code-reviewer

**Files:** none new

- [ ] **Step 1: Run Rust + Vitest**

```bash
cd src-tauri
cargo test --lib
cd ..
npm test
```

Expected: PASS

- [ ] **Step 2: If repo has `npm run validate:ci` / `preflight`, run it**

```bash
npm run validate:ci
```

Expected: PASS (skip only if script missing)

- [ ] **Step 3: Code-reviewer on the branch diff since plan start**

Use the code-reviewer skill / subagent on local changes since `docs(analytics): spec fleet timing…` (or `git diff 82d6412`). Fix Critical findings; leave nits unless required.

- [ ] **Step 4: Final commit only if review forced fixes**

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| Fleet-fused timeline | 2 |
| 10s debounce + coalesce | 2 |
| CombatHit-only combat→payout | 2 |
| Source fleet \| heuristic | 1, 2 |
| Re-enrich by scope, not sealed_id | 3, 4 |
| Partial spawn re-enrich + strip warns | 3 |
| Hide all-zero listeners + count | 4 |
| Gap removed | 4 |
| TDD seams | 2, 3, 4 |
| Legacy fc/borrowed deserialize | 1 (variants kept) |

## Placeholder scan

No TBD/TODO steps. Exact commands and code included.

## Type consistency

- `EnrichmentSource::Fleet` / `"fleet"` Task 1 → used in Task 2–3
- `hasMissileActivity` Task 4
- Re-enrich `None` = scope ids Task 3 ↔ UI passes `undefined` off Run Task 4
