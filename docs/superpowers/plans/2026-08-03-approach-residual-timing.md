# Approach Residual Timing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace segment Warp with Approach = Duration − Combat→payout, and start Combat→payout at the first fleet CombatHit after the first FollowingWarp in the gap.

**Architecture:** Breaking rename `warp_seconds` → `approach_seconds` (`Option<i64>` on sites; nullable total). Rewrite `align_gap` / `enrich_run` to drop segment warp math; pass wallet `duration_seconds` per site into enrich; update RunDesk aggregate + Tools UI labels.

**Tech Stack:** Tauri v2, Rust (`chrono`, `serde`), React Tools window, `cargo test` / Vitest as needed.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-08-03-approach-residual-timing-design.md`
- Approach = Duration − Combat→payout when both non-null and ≥ 0; else null → UI `—`
- Combat→payout = first fleet `CombatHit` with `t ≥` first `FollowingWarp` in gap and `t < payout`
- No `CombatAny` start; no segment warp / debounce / coalesce / cohort ends
- Rename fields; stale `warp_*` JSON → Re-enrich
- No `avg_approach_seconds`; Summary total Approach only
- Breaks / missing Duration / missing Combat → Approach and Combat null
- TDD; Conventional Commits; always return `EditionFocus` from RunDesk ops

## File map

| File | Responsibility |
|------|----------------|
| `src-tauri/src/analytics_types.rs` | `approach_seconds` on site + totals |
| `src/lib/analyticsTypes.ts` | Mirror types |
| `src-tauri/src/enrichment.rs` | New timing rules; drop segment helpers; tests |
| `src-tauri/src/run_desk.rs` | Pass durations into `enrich_run`; aggregate approach |
| `src-tauri/src/db.rs` | Fixture JSON / stale tests using warp → approach |
| `src/tools/ToolsApp.tsx` | Labels Approach; read `approach_seconds` |

---

### Task 1: Rename types (Rust + TS)

**Files:**
- Modify: `src-tauri/src/analytics_types.rs`
- Modify: `src/lib/analyticsTypes.ts`
- Modify minimally: anything that won’t compile (`enrichment.rs`, `run_desk.rs`, `db.rs`, `ToolsApp.tsx`) — leave logic for later tasks; prefer `approach_seconds: None` stubs only where required to compile

**Interfaces:**
- Produces:
  - `EnrichmentSite { occurred_at, approach_seconds: Option<i64>, combat_to_payout_seconds: Option<i64>, is_break, source }`
  - `EnrichmentTotals { approach_seconds: Option<i64>, combat_to_payout_seconds: Option<i64>, avg_combat_to_payout_seconds: Option<f64>, fleet_dead }`

- [ ] **Step 1: Update Rust structs** in `analytics_types.rs`

Replace site/totals `warp_seconds: i64` with `approach_seconds: Option<i64>`. Update the inline serde round-trip test JSON keys from `"warp_seconds"` to `"approach_seconds"`.

- [ ] **Step 2: Mirror in `analyticsTypes.ts`**

```ts
export type EnrichmentSite = {
  occurred_at: string;
  approach_seconds: number | null;
  combat_to_payout_seconds: number | null;
  is_break: boolean;
  source: EnrichmentSource;
};

export type EnrichmentTotals = {
  approach_seconds: number | null;
  combat_to_payout_seconds: number | null;
  avg_combat_to_payout_seconds: number | null;
  fleet_dead: number;
};
```

- [ ] **Step 3: Fix compile breakages minimally** — rename fields; temporary wrong values OK until Task 2/3. Prefer failing enrichment tests left red for Task 2.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/analytics_types.rs src/lib/analyticsTypes.ts src-tauri/src/enrichment.rs src-tauri/src/run_desk.rs src-tauri/src/db.rs src/tools/ToolsApp.tsx
git commit -m "refactor(analytics): rename warp_seconds to approach_seconds"
```

---

### Task 2: Enrichment timing rewrite (TDD)

**Files:**
- Modify: `src-tauri/src/enrichment.rs`
- Modify: `src-tauri/src/run_desk.rs` (only the `enrich_run(` call site to pass durations)
- Test: `src-tauri/src/enrichment.rs` `#[cfg(test)]`

**Interfaces:**
- Consumes: Task 1 types; `ListenerLog`; report durations
- Produces:

```rust
pub fn enrich_run(
    logs: &[ListenerLog],
    site_times: &[DateTime<Utc>],
    site_durations: &[Option<i64>], // parallel to site_times; wallet duration_seconds
    break_threshold_minutes: u32,
    run_start: Option<DateTime<Utc>>,
    missiles_per_cycle: u32,
    fc_character: Option<&str>,
    wallet_fc_hint: Option<&str>,
) -> EnrichmentSnapshot
```

**Algorithm per site (implement exactly):**

```text
gap_start = run_start if i==0 else Some(site_times[i-1])
duration = site_durations[i]

if gap_start is None:
  // first site, no run_start
  approach=None, combat=None, is_break=false, source=Heuristic
else if duration is None:
  // break or non-countable site
  approach=None, combat=None, is_break=true (if gap > threshold) or true when wallet marked break via None duration for i>0
  source=Heuristic
else:
  events in (gap_start, payout]
  warp_anchor = earliest FollowingWarp
  if no warp_anchor:
    combat=None, approach=None, source=Heuristic, is_break=false
  else:
    combat = earliest CombatHit with t >= warp_anchor && t < payout
    source = Fleet
    is_break = false
    if combat is Some(c) && duration - c >= 0:
      approach = Some(duration - c)
    else:
      approach = None
```

For `is_break` when `duration` is None: set `is_break = true` when `(payout - gap_start) > break_threshold_minutes * 60`, else `false` (covers first-site-with-run_start that is somehow None — rare).

Remove: `WARP_START_DEBOUNCE_SECS`, `coalesce_intervals`, segment/cohort end-marker logic, unused `is_end_marker`.

Call site in `run_desk.rs` `enrich_and_save`:

```rust
let site_times: Vec<_> = report.sites.iter().map(|s| s.occurred_at).collect();
let site_durations: Vec<_> = report.sites.iter().map(|s| s.duration_seconds).collect();
let mut snapshot = enrich_run(
    &scan.logs,
    &site_times,
    &site_durations,
    settings.break_threshold_minutes,
    settings.run_start,
    missiles_per_cycle,
    app_settings.fc_character.as_deref(),
    wallet_fc_hint.as_deref(),
);
```

Totals: `approach_seconds` = `Some(sum)` of non-null approaches, or `None` if none; combat sum/avg over non-null; exclude `is_break` sites from counted averages as today.

- [ ] **Step 1: Write failing tests** (replace obsolete segment-warp tests; add residual cases)

```rust
#[test]
fn approach_is_duration_minus_combat_after_following_warp() {
    // gap 20:00 → 20:10 (duration 600). FollowingWarp 20:01:00. CombatHit 20:02:30.
    // combat = 450. approach = 150.
    let pilot = listener_log(
        "Pilot",
        vec![
            ev(20, 1, 0, GamelogEventKind::FollowingWarp),
            ev(20, 2, 30, GamelogEventKind::CombatHit),
        ],
    );
    let snap = enrich_run(
        &[pilot],
        &[ts(20, 0, 0), ts(20, 10, 0)],
        &[None, Some(600)],
        25,
        Some(ts(19, 55, 0)),
        156,
        Some("Pilot"),
        None,
    );
    let site = &snap.sites[1];
    assert_eq!(site.combat_to_payout_seconds, Some(450));
    assert_eq!(site.approach_seconds, Some(150));
    assert_eq!(site.source, EnrichmentSource::Fleet);
}

#[test]
fn combat_hit_before_following_warp_ignored() {
    let pilot = listener_log(
        "Pilot",
        vec![
            ev(20, 0, 30, GamelogEventKind::CombatHit),
            ev(20, 1, 0, GamelogEventKind::FollowingWarp),
            ev(20, 2, 0, GamelogEventKind::CombatHit),
        ],
    );
    let snap = enrich_run(
        &[pilot],
        &[ts(20, 0, 0), ts(20, 10, 0)],
        &[None, Some(600)],
        25,
        Some(ts(19, 55, 0)),
        156,
        None,
        None,
    );
    let site = &snap.sites[1];
    assert_eq!(site.combat_to_payout_seconds, Some(480)); // 20:10 - 20:02
    assert_eq!(site.approach_seconds, Some(120));
}

#[test]
fn no_following_warp_null_combat_and_approach() {
    let pilot = listener_log(
        "Pilot",
        vec![ev(20, 2, 0, GamelogEventKind::CombatHit)],
    );
    let snap = enrich_run(
        &[pilot],
        &[ts(20, 0, 0), ts(20, 10, 0)],
        &[None, Some(600)],
        25,
        Some(ts(19, 55, 0)),
        156,
        None,
        None,
    );
    let site = &snap.sites[1];
    assert_eq!(site.combat_to_payout_seconds, None);
    assert_eq!(site.approach_seconds, None);
    assert_eq!(site.source, EnrichmentSource::Heuristic);
}

#[test]
fn break_duration_none_nulls_approach_and_combat() {
    let pilot = listener_log(
        "Pilot",
        vec![
            ev(20, 1, 0, GamelogEventKind::FollowingWarp),
            ev(20, 2, 0, GamelogEventKind::CombatHit),
        ],
    );
    let snap = enrich_run(
        &[pilot],
        &[ts(20, 0, 0), ts(20, 40, 0)],
        &[None, None],
        25,
        Some(ts(19, 55, 0)),
        156,
        None,
        None,
    );
    let site = &snap.sites[1];
    assert_eq!(site.combat_to_payout_seconds, None);
    assert_eq!(site.approach_seconds, None);
}

#[test]
fn negative_residual_yields_null_approach() {
    // payout 20:10, hit 20:01:40 → combat 500; duration 100 → approach null
    let pilot = listener_log(
        "Pilot",
        vec![
            ev(20, 1, 0, GamelogEventKind::FollowingWarp),
            ev(20, 1, 40, GamelogEventKind::CombatHit),
        ],
    );
    let snap = enrich_run(
        &[pilot],
        &[ts(20, 0, 0), ts(20, 10, 0)],
        &[None, Some(100)],
        25,
        Some(ts(19, 55, 0)),
        156,
        None,
        None,
    );
    let site = &snap.sites[1];
    assert_eq!(site.combat_to_payout_seconds, Some(500));
    assert_eq!(site.approach_seconds, None);
}
```

Delete or rewrite tests that assert segment warp lengths (`straggler_*`, `fleet_same_jump_*`, `warp_start_outside_debounce_*`, multi-leg warp sums, etc.). Keep missile / resolve_fc tests; update remaining to new signature + `approach_seconds`.

- [ ] **Step 2: Run — expect FAIL**

```bash
cd src-tauri
cargo test --lib approach_is_duration_minus_combat combat_hit_before_following_warp no_following_warp_null break_duration_none negative_residual -- --nocapture
```

Expected: FAIL (old API / old semantics).

- [ ] **Step 3: Implement** rewrite timing; wire `site_durations`; update call site; delete dead helpers.

- [ ] **Step 4: Run enrichment tests — expect PASS**

```bash
cd src-tauri
cargo test --lib enrichment::
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/enrichment.rs src-tauri/src/run_desk.rs
git commit -m "feat(analytics): Approach residual and post-warp CombatHit clock"
```

---

### Task 3: RunDesk aggregate + db fixtures

**Files:**
- Modify: `src-tauri/src/run_desk.rs` (`aggregate_enrichments` totals)
- Modify: `src-tauri/src/db.rs` tests/fixtures
- Test: `run_desk.rs` aggregate tests

**Interfaces:**
- Produces: `totals.approach_seconds = Some(sum)` of non-null site approaches, or `None` if none

- [ ] **Step 1: Failing / updated tests** — aggregate stubs use `approach_seconds: Some(100)` etc.; assert total `Some(300)`; update warp assertions to approach; fix analyze integration expectations under new combat/approach rules.

- [ ] **Step 2: Implement aggregate sum for approach (null if empty)**

- [ ] **Step 3: Fix db enrichment JSON fixtures** to use `approach_seconds`

- [ ] **Step 4: Run**

```bash
cd src-tauri
cargo test --lib run_desk::
cargo test --lib db::enrichment
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/run_desk.rs src-tauri/src/db.rs
git commit -m "fix(analytics): aggregate approach_seconds totals"
```

---

### Task 4: Tools UI labels

**Files:**
- Modify: `src/tools/ToolsApp.tsx`

- [ ] **Step 1: Summary** — label `Approach` (was Warp time); value `totals.approach_seconds`

- [ ] **Step 2: Per-site** — column `Approach`; cell `formatDuration(e.approach_seconds)`

- [ ] **Step 3:**

```bash
npm test
npx tsc --noEmit
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/tools/ToolsApp.tsx
git commit -m "feat(tools): show Approach instead of Warp"
```

---

### Task 5: Verify + review

- [ ] **Step 1:**

```bash
cd src-tauri
cargo test --lib -- --skip live_hamilton_log_parses_two_ones_when_present
cd ..
npm test
```

Expected: lib tests pass (hamilton skipped); Vitest pass

- [ ] **Step 2:** Code-reviewer on diff since plan start

- [ ] **Step 3:** Fix Critical/Important if found

---

## Spec coverage

| Spec requirement | Task |
|------------------|------|
| approach = duration − combat | 2 |
| CombatHit after first FollowingWarp | 2 |
| Null on break / missing | 2 |
| Rename fields / stale JSON | 1, 3 |
| Aggregate approach sum | 3 |
| UI Approach labels | 4 |
| Drop segment warp | 2 |
| No avg_approach | 1–3 |

## Placeholder scan

No TBD steps; exact signatures and test cases included.

## Type consistency

- `approach_seconds: Option<i64>` / `number | null` Task 1 → 2–4
- `enrich_run(..., site_durations: &[Option<i64>], ...)` Task 2
