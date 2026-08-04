# Dead Cycles, Hit/Miss %, and Site Missile Drill-down — Design

**Date:** 2026-08-03  
**Status:** Approved  
**App:** IncFleetChat  
**Depends on:** [2026-08-02-gamelog-enrichment-v15-design.md](./2026-08-02-gamelog-enrichment-v15-design.md)

## Problem

Run-level missile UI emphasizes **dead missiles**. Operators care more about **how many full cycles died**, plus **hit % / miss %**, and want to drill from a **site** into **site missile totals** and then **per-listener hit vs miss**. Summary also lacks an **expended → hits → dead** story that puts dead missiles in perspective.

Today missile stats exist only at run level (`MissileStat` on `EnrichmentSnapshot.missiles`). Per-site drill-down shows timing only.

## Goals

- Prefer **dead cycles** in the Listeners table while still showing **dead missiles**.
- Show **hit %** and **miss %** (vs expended) on Listeners, enrichment Summary, and site drill-down layers.
- Enrichment Summary: **Expended → Hits → Dead missiles → Dead cycles → Hit % / Miss %**.
- Per-site (run scope): click site → site totals → per-listener missile table.
- Persist per-site listener missile stats in `enrichment_json` (Re-enrich for old runs).

## Non-goals

- Changing the dead-missile formula (`expended − hits`) or ammo incomplete-magazine bias.
- Live gamelog watch, ESI, or combat graphs.
- Missile drill-down layers on Overall / Spawn scopes.
- Forcing site missile sums to equal run totals.
- Storing derived fields (`dead_cycles`, percentages) in JSON.

## Decisions

| Topic | Choice |
|-------|--------|
| Dead cycles | Replaced by **Dead volleys** = `floor(dead_missiles / launchers)` |
| Hit % / Miss % | `hits / expended`, `dead / expended`; `—` if expended = 0 |
| Expended | `reload_cycles × missiles_per_cycle` |
| Volley size | `launchers` persisted on each `MissileStat` at enrich time |
| Site window | Same gap as timing: `(gap_start, occurred_at]` |
| Breaks | `missiles: []` on the site; run-level totals still include break-gap events |
| Unalignable first site (no gap_start) | `missiles: []` |
| Persistence | `EnrichmentSite.missiles: MissileStat[]`; missing field → stale → Re-enrich |
| Drill-down UX | Push levels with Back (site list → site totals → listeners) |
| Aggregate scopes | Timing table only; no site missile click-through |
| Fleet dead cycles | Sum of per-listener `dead_cycles` (not floor of fleet dead) |
| Approach | Schema + shared derived helpers; UI stays thin |

## Formulas

```
expended     = reload_cycles × missiles_per_cycle
dead         = max(0, expended − hits)          # already stored
dead_cycles  = (removed — use dead_volleys)
dead_volleys = floor(dead / launchers)          # launchers = ammo launchers at enrich
hit_pct      = hits / expended                  # undefined if expended = 0
miss_pct     = dead / expended                  # undefined if expended = 0
```

Derived values are **not** persisted. Shared TypeScript helpers (and Rust tests at the enrich seam) own the math.

Fleet Summary rates use **fleet hits / fleet expended** for Hit % and **fleet dead / fleet expended** for Miss %. Fleet **Dead volleys** = sum of each listener’s `dead_volleys`.

## Data model

`MissileStat` unchanged:

```ts
{ listener, reload_cycles, hits, missiles_per_cycle, dead }
```

`EnrichmentSite` gains:

```ts
missiles: MissileStat[]  // empty for breaks / unalignable sites
```

`EnrichmentTotals` unchanged (`fleet_dead` remains). Summary Expended / Hits / % derive from run-level `missiles`.

Old snapshots without `sites[].missiles` fail deserialize (required field) and are treated as stale.

## Enrichment pipeline

In `enrich_run`, for each site:

1. Timing alignment unchanged.
2. If `is_break` or no `gap_start` → `missiles: []`.
3. Else, per listener, count Reload / CombatHit in `(gap_start, occurred_at]` and build `MissileStat` with the same dead math as run-level.
4. Run-level `missiles` still use the full clipped run window (including events that fall in break gaps).

**Invariant:** Σ site missile counts need not equal run-level totals (break / unassigned gaps). Run-level remains source of truth for fleet ammo.

### Aggregation

`aggregate_enrichments` continues to merge run-level `MissileStat` by listener. Do **not** invent cross-run merges of per-site missile arrays for Overall/Spawn; UI ignores site missiles outside run scope.

## UI

### Gamelog enrichment (Summary aside)

After combat timing rows:

| Row | Value |
|-----|--------|
| Expended | Σ listener expended |
| Hits | Σ listener hits |
| Dead missiles | `totals.fleet_dead` |
| Dead cycles | Σ per-listener dead_cycles |
| Hit % / Miss % | Hit % = fleet dead / fleet expended; Miss % = fleet hits / fleet expended; `—` if expended = 0 |

Keep the incomplete-magazine undercount note.

### Listeners panel

Columns: Listener | Reload cycles | Hits | Missiles/cycle | **Dead cycles** | **Dead missiles** | **Hit %** | **Miss %**.

Filter with existing `hasMissileActivity`. Format percentages to **one decimal** (e.g. `12.3%`); undefined → `—`.

### Per-site detail (run scope only for missile layers)

| Level | Content |
|-------|---------|
| 0 | Existing site timing table; rows clickable when `scope.kind === "run"`, enrichment present, and the site’s `missiles` is non-empty |
| 1 | Site totals: Reload cycles, Hits, Dead cycles, Dead missiles, Hit %, Miss %; Back → 0; open listeners → 2 |
| 2 | Per-listener table for that site (same columns as Listeners); Back → 1 |

**Level 1 aggregation:** same rule as fleet Summary — sum base counts (`reload_cycles`, `hits`, `dead`, per-listener `expended` / `dead_cycles`), then derive Hit % = summed dead / expended and Miss % = summed hits / expended (do not average per-listener percentages).

Sites with empty `missiles` (breaks and unalignable first sites) are **not clickable** for missile layers; they remain visible in the level-0 timing table only.

**Overall / Spawn:** flat timing table only; Listeners + Summary still show aggregated run-level missile metrics when enrichment exists.

## Error handling / edge cases

| Case | Behavior |
|------|----------|
| expended = 0 | Hit % / Miss % → `—` |
| Incomplete magazines | Existing undercount bias; keep UI note |
| Break-gap events | In run totals; not under any site |
| Stale enrichment | Deserialize fail → Re-enrich |
| Aggregate scope | No site missile drill-down |

## Testing seams

1. **Rust enrich:** hits/reloads inside vs outside a site gap land in the correct `EnrichmentSite.missiles`.
2. **Rust enrich:** break site has empty `missiles`; run-level still counts break-gap events.
3. **TS helpers:** expended, dead_cycles, hit_pct, miss_pct (including expended = 0).
4. Optional: percentage formatting helper.

## Success

- Listeners show dead cycles + dead missiles + hit/miss %.
- Summary shows expended → hits → dead missiles → dead cycles → hit/miss %.
- On a single run, site → totals → listeners missile drill-down works from persisted site `missiles`.
- Old runs require Re-enrich; Overall/Spawn do not offer site missile layers.
