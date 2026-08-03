# Analytics Metric Parity — Design

**Date:** 2026-08-02  
**Status:** Approved  
**Parent:** [2026-08-02-run-analytics-tools-design.md](./2026-08-02-run-analytics-tools-design.md)  
**App:** IncFleetChat Tools → Analytics

## Problem

Summary totals from RunDesk do not match the Google Sheet for the same wallet run:

| Metric | Sheet (example) | App (before fix) |
|--------|-----------------|------------------|
| Sites / Net LP / LP value | 196 / 5.88M / 8.232B | match |
| Liquid / Net value | 43.875B / 52.107B | 2.94B / 11.172B |
| Time spent / avg site | 18:49 / 5:46 | 19:53 / 6:27 |
| \$/hr | ~2.33B liquid/hr | ~148M liquid/hr |

Root cause for value scale: liquid was **one character’s** wallet sum (`sites × 15M`), while the sheet’s Liquid Value is **fleet** (`Σ amount × fleet_size`). LP was already fleet-scaled. \$/hr followed the wrong liquid and a single clock.

Time gap vs sheet is partly unexplained sheet rules; we expose two clocks and use active site time for rates.

## Goals

- Sheet-parity **fleet** liquid, net value, and \$/hr.
- Show **character** liquid for sanity-checking a single wallet paste.
- Expose **Active site time** (primary) and **Wallet elapsed** (secondary).
- Hourly Total ISK = fleet liquid in that hour.
- Show Per-character LP total (`net_lp / fleet_size`).

## Non-goals

- Gamelog warp vs in-site split (still v1.5).
- Changing break-threshold / first-site timing rules beyond dual clocks.
- Automatically reverse-engineering opaque sheet formulas for time.

## Decisions

| Topic | Choice |
|-------|--------|
| Liquid display | Both character and fleet; fleet is primary (sheet Net/Liquid Value) |
| Fleet liquid formula | `Σ (wallet_amount × fleet_size)` |
| Clocks | Active site time + Wallet elapsed; Active is primary “Time spent” |
| \$/hr basis | Fleet liquid (and LP/net) ÷ **active** hours |
| Hourly Total ISK | Fleet |
| Approach | Explicit dual metrics on report DTO (not a mode toggle) |

## Metric definitions

Per site (`SiteDetail` — required fields for merge/hourly):

- `character_isk` / `amount_isk` = wallet payout amount  
- `fleet_isk` = `character_isk × fleet_size` (**stored on each site**, same pattern as `fleet_lp`)  
- `fleet_lp` = `lp_per_char × fleet_size` (unchanged)

Session (`SessionSummary`):

- `character_liquid_isk` = Σ character_isk  
- `fleet_liquid_isk` = Σ fleet_isk  
- `net_lp` = Σ fleet_lp  
- `lp_per_character_total` — see merge rules below  
- `lp_value` = `net_lp × isk_per_lp`  
- `net_value` = `fleet_liquid_isk + lp_value`  
- `active_hours` = `active_site_seconds / 3600.0`

Clocks:

- `wallet_elapsed_seconds` = last_payout − first_payout **within the report’s site set** (0 if fewer than 2 sites). For Overall/spawn merges this can span multiple days — that is intentional.  
- `active_site_seconds` = Σ durations where `counts_toward_avg` (existing break / run_start rules)  
- **`time_spent_seconds` is removed** from the DTO; UI uses `active_site_seconds` as “Time spent”.  
- Avg site time = `active_site_seconds / n_counted`  

Rates (if `active_site_seconds > 0`):

- `liquid_isk_per_hour` = `fleet_liquid_isk / active_hours`  
- `lp_value_per_hour` = `lp_value / active_hours`  
- `net_per_hour` = `net_value / active_hours`

Hourly:

- `total_isk` = Σ **fleet_isk** in bucket  
- `total_lp` = Σ fleet_lp  

### Merge / Overall / spawn

`merge_reports` concatenates sites (each already carries `fleet_isk` / `fleet_lp`). Totals = sum over sites. Do **not** re-multiply by a single live `fleet_size`.

- `lp_per_character_total` for a **single run** = `net_lp / max(fleet_size, 1)` using that run’s snapshotted fleet_size.  
- For **spawn / Overall** (mixed fleet sizes): `lp_per_character_total` = **sum of each run’s** `lp_per_character_total` when merging via stored run summaries; if merging only site rows without per-run snapshots, **omit** the field (null) rather than divide `net_lp` by a guessed size. Prefer omitting over a wrong number.

### Catalog

`RunSummary.liquid_isk` becomes **fleet** liquid for that run (rename to `fleet_liquid_isk` in DTO if touching the type; UI label “Liquid”).

## DTO / persistence

`SessionSummary` fields: `character_liquid_isk`, `fleet_liquid_isk`, `wallet_elapsed_seconds`, `active_site_seconds`, `lp_per_character_total` (nullable), rates as above. Remove `time_spent_seconds` and `liquid_isk`.

`SiteDetail` adds `fleet_isk` (keep `amount_isk` as character amount for drill-down).

Saved `report_json`: old runs without new fields:

1. Deserialize with serde defaults / `Option` where needed.  
2. If `fleet_liquid_isk` (or equivalent) is missing after load: treat the **session report as unavailable for display** — `EditionFocus.report` may still list sites from partial data **or** set `report: null` for that scope; either way Summary shows empty/placeholder totals and a diagnostic: `Re-Analyze to refresh metrics`.  
3. Do **not** multiply character-only `liquid_isk` by live `fleet_size` to fake fleet liquid.  
4. Rates when `active_site_seconds == 0`: all \$/hr fields are `0.0`.

`amount_isk` remains the DTO name for per-site character ISK (prose “character_isk” = that field).

Merge `lp_per_character_total`: sum each input report’s session value when present; if any input lacks it, result is null.

## UI

Summary primary block (sheet language):

1. Time spent (active), Wallet elapsed  
2. Sites, Avg site time  
3. Liquid/LP/Net per hour  
4. Net LP, Per character LP (hide if null), ISK/LP  
5. Fleet Liquid Value, LP Value, Net Value  
6. Secondary: Character liquid  

Hourly Total ISK = fleet (correct numbers; no new columns).

## Testing seams

1. `build_report` — character vs fleet liquid, `fleet_isk` on sites, clocks, rates  
2. `merge_reports` — sums precomputed `fleet_isk`; does not re-apply live fleet_size  
3. Worked example: 2×15M, fleet_size 15 → character 30M, fleet 450M  
4. Frontend types + Summary labels  

## Implementation notes

- Change `src-tauri/src/timing.rs` first (TDD).  
- Update `src/lib/analyticsTypes.ts` and `src/tools/ToolsApp.tsx`.  
- Drill-down Close UX is a separate small UI fix if still uncommitted — not required for metric parity.

## Follow-on

- Align active-time rules further with sheet once the sheet formula is known.  
- Gamelog enrichment for warp vs site (v1.5).
