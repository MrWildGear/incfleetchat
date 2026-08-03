# Analytics Tools UI & Combat→Payout — Design

**Date:** 2026-08-02  
**Status:** Approved  
**App:** IncFleetChat  
**Depends on:** [2026-08-02-gamelog-enrichment-v15-design.md](./2026-08-02-gamelog-enrichment-v15-design.md)

## Problem

v1.5 enrichment surfaces **in-site** as “gap minus warp,” which mixes clear time with post-payout loot/regroup and is hard to read against sheet-style ops review. The Listeners missile table is always open and does not match the per-site drilldown panel pattern. On **Spawn/Overall**, enrichment still attaches a single sealed run, so listener rows and timing clocks are not whole-op. ISK and counts lack consistent `$#,###` / `#,###` formatting. Enrichment file/listener diagnostics clutter Summary.

## Goals

- Replace public **in-site** clocks with **Combat→payout** (first combat after warp ends → wallet payout) on the FC/borrow timing log.
- Listeners panel uses the same chrome as per-site drilldown (sticky title, Close, toolbar toggle); both panels may be open.
- Spawn/Overall: merge missile rows by Listener name; aggregate enrichment timing and site lists across enriched runs in scope.
- Format ISK/liquid as `$#,###`; missile and similar counts as `#,###`; LP without `$`.
- Keep Summary metric-only; move enrich file/skip diagnostics to the top Gamelogs / Re-enrich strip.
- Keep existing wallet **Avg site time** in Summary.

## Non-goals

- ESI / live Gamelog watch
- Changing the wallet Avg site time formula
- Keeping `in_site_*` in the UI or public `EnrichmentSnapshot`
- Fuzzy Listener name matching across typos/renames
- Recomputing combat→payout from stale snapshots that only stored in-site (Re-enrich instead)

## Decisions (grilled)

| Topic | Choice |
|-------|--------|
| Listener chrome | Match drilldown: sticky header + Close |
| Open listeners | Separate toolbar toggle; both panels can be open |
| Spawn/Overall missiles | Merge by name; sum reload/hits/dead; **do not** sum missiles/cycle (latest if disagree) |
| Combat→payout definition | First combat on FC/borrow timing log after warp ends (or gap start if no warp) → payout |
| In-site | **Replace** everywhere in public snapshot/UI |
| No combat / breaks / first site | `null` → UI `—`; excluded from averages |
| Spawn/Overall timing | Aggregate all enriched runs in scope (sum warp/combat; avg over non-null; concat sites) |
| Diagnostics placement | Top Analytics strip near Gamelogs/Re-enrich — not Summary |
| Number format | ISK/liquid `$#,###`; counts `#,###`; LP no `$` |
| Approach | Replace in-site in enrichment model + aggregate in RunDesk |

## Architecture

```
Per-run enrich_run (persisted)
        │
        ├── sites: warp + combat_to_payout (Option) + source
        └── missiles: per Listener
        │
        ▼
EditionFocus.enrichment
        │
        ├── Run scope → load that run’s snapshot
        └── Spawn / Overall → aggregate enriched runs in scope
                (merge missiles, concat sites, recompute totals)
        │
        ▼
Tools UI: Summary metrics + optional Sites panel + optional Listeners panel
```

### Enrichment snapshot (breaking vs v1.5 public shape)

```ts
type EnrichmentSite = {
  occurred_at: string;
  warp_seconds: number;
  combat_to_payout_seconds: number | null; // null → "—"
  is_break: boolean;
  source: "fc" | "borrowed" | "heuristic";
};

type MissileStat = {
  listener: string;
  reload_cycles: number;
  hits: number;
  missiles_per_cycle: number; // not summed across runs
  dead: number;
};

type EnrichmentSnapshot = {
  resolved_fc: string | null;
  listeners: string[];
  diagnostics: Diagnostic[];
  sites: EnrichmentSite[];
  missiles: MissileStat[];
  totals: {
    warp_seconds: number;
    combat_to_payout_seconds: number; // sum of non-null only
    avg_combat_to_payout_seconds: number | null;
    fleet_dead: number;
  };
};
```

Remove `in_site_seconds` / `avg_in_site_seconds` from the public snapshot.

### Combat→payout rule

On the same timing event stream used for warp (FC, with borrowed warp-starts when needed):

1. If site is **break** or **first site** with no leading gap clocks (v1.5 rule): `combat_to_payout_seconds = null`.
2. Else determine `clear_start`:
   - If warp markers exist in the gap: end of the last warp segment in that gap.
   - Else (heuristic non-break gap with no markers): gap start.
3. First **combat** event (`CombatHit` or `CombatAny`) with `occurred_at >= clear_start` and `< payout`.
4. If none: `null`. Else: `(payout - first_combat).num_seconds().max(0)`.

### Spawn / Overall aggregation (RunDesk)

For each run in scope with readable enrichment:

- Concatenate `sites` (preserve `occurred_at` for drilldown join).
- Merge `missiles` by exact Listener string: sum `reload_cycles`, `hits`, `dead`; set `missiles_per_cycle` from the **latest** contributing run (by run seal / catalog order already used elsewhere); recompute `dead` is **not** re-derived from merged totals — use summed `dead` as stored per run (already `max(0, cycles×cycle−hits)` per run).
- Totals: sum `warp_seconds`; sum non-null combat; avg = sum / count non-null; `fleet_dead` = sum of merged dead (or sum of per-run fleet_dead — equivalent if merge is complete).
- `resolved_fc`: latest enriched run’s value (good enough; no multi-FC UI this slice).
- `diagnostics`: union, plus warn if some runs in scope lack enrichment (`N of M runs lack enrichment`).
- `listeners`: union of names present after merge.

Do **not** re-scan Gamelogs at aggregate time.

### Stale snapshots

Persisted JSON still containing `in_site_*` without `combat_to_payout_*` is treated as unreadable/stale for combat clocks: either fail soft into “needs Re-enrich” diagnostic or load with all combat fields null. Prefer an explicit top-strip warn over inventing values from old in-site.

## UI

| Surface | Behavior |
|---------|----------|
| Toolbar | “Per-site drill-down” + “Listeners”; independent toggles |
| Listeners panel | Same chrome as site detail; columns Listener, Reload cycles, Hits, Missiles/cycle, Dead; counts `#,###` |
| Site drilldown | Warp; **Combat→payout**; Source; ISK `$#,###` |
| Summary (session) | Keep Avg site time; ISK/liquid/net ISK `$#,###`; LP without `$` |
| Summary (enrichment) | Warp; Combat→payout total; Avg combat→payout; Dead missiles `#,###`; optional Resolved FC. No file lists. No enrich diagnostics. |
| Top strip | Enrichment status + diagnostics (missing dir, skipped files, partial spawn coverage) |

## Errors & edges

| Case | Behavior |
|------|----------|
| No combat before payout | `null` / `—`; exclude from avg |
| Break / first-site no clocks | `null` |
| Partial spawn enrichment | Aggregate what exists; warn `N of M` |
| Listener rename across runs | Separate rows |
| missiles/cycle differs across runs | Keep one row; use latest cycle value; do not sum cycle |

## Testing seams

1. **enrichment:** worked example for combat→payout; null on break, no combat, first site.
2. **run_desk aggregate:** two runs, same listener → summed counts; cycle not summed; totals/avg correct.
3. **format helpers:** ISK → `$1,234`; count → `1,234`; LP formatter has no `$`.

## Implementation notes

- TDD at the seams above; Conventional Commits; code-reviewer before merge of the implementation branch.
- Re-enrich after ship to refresh existing DBs.

## Out of scope / later

- Labeling mixed `resolved_fc` across runs
- Restoring in-site as a debug-only column
- Per-hour listener drilldown
