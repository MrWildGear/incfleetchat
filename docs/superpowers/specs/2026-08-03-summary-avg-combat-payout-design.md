# Summary Avg Combat→payout — Design

**Date:** 2026-08-03  
**Status:** Approved  
**App:** IncFleetChat  
**Depends on:** Existing enrichment totals (`avg_combat_to_payout_seconds`) from Approach residual timing / fleet timing work

## Problem

Avg combat→payout is only visible under **Gamelog enrichment**. Operators want it in the main **Summary** panel next to wallet timing (e.g. Avg site time), without removing Approach or changing enrichment math.

## Goals

- Always show **Avg combat→payout** in Summary when a session aggregate is shown.
- Value comes from `focus.enrichment.totals.avg_combat_to_payout_seconds` when enrichment exists; otherwise `—`.
- Keep the existing **Avg combat→payout** row under Gamelog enrichment (both places).

## Non-goals

- Removing Approach (left for a later change)
- Changing Combat→payout / avg formulas or enrichment JSON shape
- Lifting avg into the wallet `session` aggregate
- Per-site avg (site rows stay per-site Combat→payout only)

## Decision

**UI-only duplicate row** in `ToolsApp.tsx` Summary, after **Avg site time**, same label and `formatDuration` as enrichment.

## UI

| Surface | Behavior |
|---------|----------|
| Summary | Always (when `session` is present): row **Avg combat→payout** → `formatDuration(focus?.enrichment?.totals.avg_combat_to_payout_seconds)` |
| Gamelog enrichment | Unchanged: existing **Avg combat→payout** row |

## Out of scope for verification

No backend tests. Manual or existing Vitest/tsc as needed for the Tools UI touch.

## Success

Summary and enrichment both show avg Combat→payout; missing enrichment shows `—` in Summary; Approach unchanged.
