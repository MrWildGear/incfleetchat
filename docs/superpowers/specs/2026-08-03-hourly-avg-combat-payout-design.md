# Hourly Avg Combat→payout — Design

**Date:** 2026-08-03  
**Status:** Approved  
**App:** IncFleetChat  
**Depends on:** Enrichment per-site `combat_to_payout_seconds`; Results hour table in Tools

## Problem

Operators comparing pace by hour see **Avg site** but not combat pace. Avg combat→payout exists in Summary / enrichment, not in the Results hour blocks.

## Goals

- Add **Avg combat→payout** immediately to the right of **Avg site** in the Results hour table.
- Per hour: mean of enrichment sites’ `combat_to_payout_seconds` whose `occurred_at` falls in that UTC hour; skip nulls; `—` if none (including missing enrichment).
- UI-only join — do not change `HourlyBucket` / wallet `report` JSON.

## Non-goals

- Backend hourly fields or re-persist report JSON for this column
- Sum (total) combat→payout per hour
- Changing Summary / enrichment / Approach
- Changing Combat→payout start rules

## Decision

**UI join:** pure helper averages enrichment sites into each `hour_start` bucket already produced by wallet timing. Hour floor = UTC hour of `occurred_at` (same idea as Rust `hour_floor` used for wallet hourly). Prefer a small `src/lib` helper so Vitest covers averaging + hour matching; `ToolsApp` only renders.

## UI

| Surface | Behavior |
|---------|----------|
| Hour table header | … · Avg site · **Avg combat→payout** |
| Hour cell | `formatDuration(avg)` or `—` |
| Empty state colspan | Include the new column |

## Averaging rule

For hour `H`:

1. Take enrichment `sites` with UTC hour floor(`occurred_at`) == `H`.
2. Collect non-null `combat_to_payout_seconds`.
3. If empty → `null`; else arithmetic mean (same spirit as session `avg_combat_to_payout_seconds`).

Breaks / missing combat already null on enrichment sites and are skipped.

## Success

Hour rows show Avg combat→payout beside Avg site when enrichment has combat clocks for that hour; otherwise `—`; no report schema change.
