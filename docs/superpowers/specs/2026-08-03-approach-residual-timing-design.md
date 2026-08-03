# Approach Residual Timing — Design

**Date:** 2026-08-03  
**Status:** Approved  
**App:** IncFleetChat  
**Depends on:** [2026-08-02-fleet-timing-reenrich-design.md](./2026-08-02-fleet-timing-reenrich-design.md)  
**Supersedes (timing only):** Segment Warp (FollowingWarp → cohort end-marker, debounce/coalesce) and Combat→payout anchored on `clear_start` from that segment. Re-enrich enablement, Listeners zero-filter, Gap removal, and fleet event pool for combat **hits** from that doc remain.

## Problem

Per-site **Warp** from gamelog segments is often ~1s while real travel is ~1–2 minutes: cohort end-markers (including the warper’s own combat blip right after `Following in warp`) close the segment early and push travel into **Combat→payout**. Further patching segment ends (floors, Regrouping-only) adds complexity without a clean identity for “Warp.”

We already trust **Duration** (wallet payout→payout) and can redefine **Combat→payout**. Their difference is a stable residual.

## Goals

- Replace public **Warp** with **Approach** = Duration − Combat→payout when both measurable.
- Redefine Combat→payout start: first fleet `CombatHit` at/after the first `FollowingWarp` in the gap (not after segment end).
- Rename snapshot fields `warp_seconds` → `approach_seconds` (sites + totals); UI/Summary say Approach.
- Null Approach on breaks / missing Duration / missing Combat (no overnight multi-hour Approach).
- Drop segment warp math (debounce, coalesce, cohort end markers) from the public path.

## Non-goals

- Changing wallet Duration / Avg site time formulas
- Restoring segment Warp as a public or debug column
- Using `CombatAny` to start Combat→payout
- Configurable thresholds
- Persisting Approach without Re-enrich for old `warp_*` JSON

## Decisions (grilled)

| Topic | Choice |
|-------|--------|
| Column meaning | **Approach** (honest residual), not “Warp” |
| Formula | Approach = Duration − Combat→payout when both non-null and ≥ 0 |
| Combat→payout start | First fleet `CombatHit` ≥ first `FollowingWarp` in gap |
| Segment Warp | **Removed** from public snapshot |
| Missing clocks | Approach `—` if Duration or Combat null (incl. breaks) |
| Storage | Rename `warp_seconds` → `approach_seconds` in enrichment JSON |
| Approach | Residual computed in enrichment (not UI-only) |

## Architecture

```
Wallet Duration (site)          Gamelog pool in gap
        │                              │
        │                    first FollowingWarp
        │                              │
        │                    first CombatHit ≥ warp
        │                              ▼
        │                    combat_to_payout_seconds
        │                              │
        └──────────┬───────────────────┘
                   ▼
         approach = duration − combat  (or null)
                   │
                   ▼
         EnrichmentSnapshot → UI (Approach, Combat→payout)
```

### Snapshot shape (breaking)

```ts
type EnrichmentSite = {
  occurred_at: string;
  approach_seconds: number | null; // was warp_seconds: number
  combat_to_payout_seconds: number | null;
  is_break: boolean;
  source: "fleet" | "heuristic" | /* legacy */ "fc" | "borrowed";
};

type EnrichmentTotals = {
  approach_seconds: number | null; // sum of non-null site approach; null if none
  combat_to_payout_seconds: number | null;
  avg_combat_to_payout_seconds: number | null;
  fleet_dead: number;
};
```

No `avg_approach_seconds` this slice (totals today have no warp avg — only combat avg).

Persisted rows with `warp_seconds` and without `approach_seconds` are **stale** (deny_unknown_fields / missing field → Re-enrich), same operational pattern as prior enrichment breaks.

## Timing rules

For each site gap `(prev payout | run_start) → this payout`:

1. If no `gap_start`, or wallet/enrichment **break**: `combat_to_payout_seconds = null`, `approach_seconds = null`.
2. Else scan fleet events in `(gap_start, payout]`:
   - `warp_anchor` = earliest `FollowingWarp`, if any.
   - If no `warp_anchor`: combat `null`, approach `null`, source `heuristic`.
   - Else combat = earliest `CombatHit` with `occurred_at >= warp_anchor` and `< payout`; else combat `null`.
3. Let `duration` = wallet `duration_seconds` for this site (null on break).
4. If `duration` and combat both `Some(d)` / `Some(c)` and `d - c >= 0`: `approach_seconds = d - c`; else `null`.
5. Source = `fleet` if `warp_anchor` exists; else `heuristic`.

Do **not** use Regrouping / CombatAny / segment end for Approach or for Combat→payout start.

`enrich_run` must receive per-site duration (or compute the same gap/break rules from site times + break threshold already passed in). Prefer joining wallet duration already known at enrich time from the sealed report’s `SiteDetail.duration_seconds` by `occurred_at`.

## Aggregation (Spawn / Overall)

- Concat sites; sum non-null `approach_seconds` for totals (null total when zero measurable — mirror combat null policy: if none measurable, `approach_seconds` total `null`).
- Combat sum/avg over non-null combat values as today.
- Missiles / listeners / diagnostics unchanged.

## UI

| Surface | Behavior |
|---------|----------|
| Per-site | **Approach** column (was Warp); Combat→payout; Source |
| Summary | **Approach** total (was Warp); Combat→payout total/avg; Dead missiles |
| Null | `—` |

## Errors & edges

| Case | Behavior |
|------|----------|
| Break / no Duration | Approach `—`; Combat `—` |
| No FollowingWarp | Combat `—`; Approach `—` |
| Warp but no CombatHit after | Combat `—`; Approach `—` |
| `duration < combat` | Approach `null` |
| Stale `warp_*` JSON | Enrichment load stale → Re-enrich |

## Testing seams

1. **enrichment:** Duration 600 + Combat 450 → Approach 150; CombatHit before first FollowingWarp ignored; no FollowingWarp → both null; break → both null; negative residual → Approach null.
2. **run_desk aggregate:** two sites with approach 100 and 200 → total 300; nulls excluded from sum.
3. **Types/UI:** `approach_seconds` in TS; label Approach; stale warp JSON fails deserialize / needs Re-enrich.

## Implementation notes

- TDD at seams; Conventional Commits; code-reviewer before merge.
- Remove or gut `coalesce_intervals` / warp debounce helpers if unused after rewrite.
- Re-enrich after ship.
- Summary: total Approach only (no approach average).

## Out of scope / later

- Splitting Approach into “idle before warp” vs “travel”
- CombatAny starting Combat→payout
- Debug segment-warp column
