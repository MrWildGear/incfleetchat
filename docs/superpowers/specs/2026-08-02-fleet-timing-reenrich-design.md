# Fleet Timing, Re-enrich Scope & Listeners Filter — Design

**Date:** 2026-08-02  
**Status:** Approved  
**App:** IncFleetChat  
**Depends on:** [2026-08-02-analytics-tools-combat-payout-design.md](./2026-08-02-analytics-tools-combat-payout-design.md)  
**Supersedes (timing only):** That doc’s FC/borrow timing pool and “first `CombatHit` \| `CombatAny` on the fused FC stream” for Combat→payout. This spec’s fleet pool + `CombatHit`-only start replace those rules. Wallet Duration / Avg site time, missile merge, and snapshot field layout from that doc remain.

## Problem

Warp and Combat→payout clocks are wrong in practice because timing is still FC-gated: warp may borrow, but combat (and much of the effective clear clock) reads the FC log. Quiet or mis-resolved FC logs miss triggers that other listeners have. After app restart, Re-enrich is often disabled on Spawn/Overall because `sealed_run_id` is session-only. Listeners with no missile activity still clutter the panel. Per-site Gap is redundant next to Duration.

## Goals

- Time Warp and Combat→payout from a **fleet-fused** gamelog timeline (all listeners), not FC-only combat.
- Coalesce overlapping fleet warp into wall-clock time; debounce new fleet warp starts within **10 seconds**.
- Start Combat→payout from earliest fleet **outgoing hit** (`CombatHit`); do not start it from `CombatAny`.
- Re-enrich without session `sealed_run_id`: Run = that run; Spawn/Overall = all runs in scope (best-effort).
- Hide Listeners rows where reload, hits, and dead are all zero; title count = visible rows.
- Keep Gap column removed from Per-site detail.

## Non-goals

- Changing wallet Duration / Avg site time formulas
- Configurable debounce (hardcode 10s)
- Fuzzy listener name matching
- Clearing enrichment on failed re-enrich attempts
- Persisting `sealed_run_id` across restarts (scope-based enablement instead)

## Decisions (grilled)

| Topic | Choice |
|-------|--------|
| Timing pool | Merge all listeners into one timeline; run warp + combat→payout on that stream |
| Multi-listener same jump | Coalesce overlapping segments + **10s** accepted-start debounce |
| Warp end markers | `Regrouping` \| `CombatHit` \| `CombatAny` from the **jump cohort** only (listeners who `FollowingWarp`'d within the 10s window) — blocks straggler combat on the previous grid |
| Combat→payout start | Earliest fleet `CombatHit` after `clear_start`; not `CombatAny` |
| Source | `fleet` when warp markers used; else `heuristic` (replace fc/borrowed meaning for this clock) |
| Re-enrich enable | Run scope **or** Spawn/Overall with ≥1 run in scope (not sealed_run_id) |
| Spawn/Overall re-enrich | All runs in scope; partial OK; keep old snapshot on failure; top-strip warnings |
| Zero listeners | Hide when reload=hits=dead=0 |
| Approach | Rewrite `align_gap` in place in `enrichment.rs` |

## Architecture

```
Listener logs (all in run window)
        │
        ▼
align_gap (fleet pool)
  · debounce FollowingWarp starts (10s)
  · segment + coalesce → warp_seconds
  · clear_start → earliest CombatHit → combat_to_payout
  · source: fleet | heuristic
        │
        ▼
EnrichmentSnapshot (persisted) → EditionFocus → Tools UI
```

Re-enrich / Listeners filter / Gap removal are orthogonal UI–desk changes on the same snapshot.

### EnrichmentSource

Emit **`fleet`** | **`heuristic`** for new snapshots. Keep deserializing legacy `fc` / `borrowed` so old JSON loads; UI may display them as-is until Re-enrich. Prefer adding `Fleet` to the enum (serde `snake_case`) rather than overloading `Borrowed`.

Persisted field set for sites/totals is unchanged — meaning of clocks changes; users must **Re-enrich** for correct values.

## Fleet timing rules

For each site gap `(prev payout | run_start) → this payout`:

1. Pool all listeners’ events in `(gap_start, gap_end]`.
2. Collect `FollowingWarp` starts, sorted. Accept a start only if ≥ **10s** after the previous accepted start.
3. Each accepted start ends at earliest of: next accepted start, jump-cohort end-marker after start (`Regrouping` \| `CombatHit` \| `CombatAny` from listeners who `FollowingWarp`'d within 10s of that start), or payout.
4. Coalesce overlapping/adjacent segments; `warp_seconds` = sum of coalesced wall-clock lengths.
5. `clear_start` = end of last coalesced segment. No markers + non-break → `clear_start = gap_start`, warp `0`, source `heuristic`. Break (no markers, gap > threshold) → warp `0`, combat `null`, source `heuristic`.
6. `combat_to_payout_seconds` = `payout −` earliest fleet `CombatHit` with `clear_start ≤ t < payout`, or `null`.
7. Source = `fleet` if any warp markers were used; else `heuristic`.

Missile stats remain per-listener. `resolved_fc` may remain for Summary display; it does **not** drive these clocks.

## Re-enrich

| Scope | Behavior |
|-------|----------|
| Run | Re-enrich that `run_id` |
| Spawn / Overall | Re-enrich each run in scope; successes overwrite; failures leave prior snapshot |
| Enable button | Run scope, or Spawn/Overall with ≥1 catalog run in scope — **not** gated on in-memory `sealed_run_id` |
| Diagnostics | e.g. `K of M re-enriched` plus skip reasons in the top strip |

## UI

| Surface | Behavior |
|---------|----------|
| Per-site | No Gap column; Warp; Combat→payout; Source `fleet`/`heuristic` (legacy labels until Re-enrich) |
| Listeners | Omit rows with reload=hits=dead=0; header count = visible rows |
| Re-enrich | Enabled per rules above after reopen |

## Errors & edges

| Case | Behavior |
|------|----------|
| No gamelogs | heuristic / null clocks; strip warn |
| Partial spawn re-enrich | Keep failures’ old data; warn |
| All listeners zeroed | Empty table / count 0 |
| Pre-fleet snapshots | Schema OK, clocks wrong until Re-enrich |

## Testing seams

1. **enrichment:** multi-listener same jump → one warp (not N×); 10s debounce folds late start; Combat→payout from earliest fleet `CombatHit`, not FC-only / not `CombatAny`.
2. **run_desk:** Spawn re-enrich attempts all runs; one failure does not block others; Run re-enrich works with `sealed_run_id = None`.
3. **Listeners filter:** all-zero row omitted; count matches visible.

## Implementation notes

- TDD at seams above; Conventional Commits; code-reviewer before merge.
- Approach: rewrite `align_gap` in place (same module; no new file unless it becomes unreadable).
- Re-enrich after ship to refresh existing DBs.

## Out of scope / later

- Configurable debounce
- Persist last sealed run across restart
- One-click “force clear stale enrichment” on failed runs
