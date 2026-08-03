# Gamelog Enrichment (v1.5) & Analytics Roadmap — Design

**Date:** 2026-08-02  
**Status:** Draft (pending review)  
**App:** IncFleetChat  
**Depends on:** [2026-08-02-run-analytics-tools-design.md](./2026-08-02-run-analytics-tools-design.md) (v1 wallet analytics + RunDesk)

## Problem

v1 site timing uses wallet payout gaps only. That cannot separate **warp between sites** from **in-site clear time**, and it cannot measure **dead missiles** (reloads fired but no hit line because the target died in-flight). Gamelogs contain warp, regroup, combat hits, and reload notifications across ~17 multibox listeners; the FC character often lacks `Following … in warp`.

## Roadmap

| Version | Scope | Status |
|---------|--------|--------|
| **v1** | Wallet paste analytics + ammo planner | Shipped |
| **v1.5** | Gamelog enrich-on-Analyze: warp vs in-site, dead missiles, Re-enrich | **This spec** |
| **v2** | ESI character wallet journal → same events as paste | Roadmap stub only |
| **Later** | Fleet-chat session markers (`start session` / `break`) | Out of scope |

## Goals (v1.5)

- After a sealed wallet **run**, scan local Gamelogs and enrich that run.
- Split each inter-payout gap into **warp** vs **in-site** (with v1 break heuristic fallback).
- Compute **dead missiles** per Listener and fleet total using Ammo-tab cycle size.
- **Re-enrich** without re-pasting wallet.
- FC log is primary for combat/reload; **borrow** warp-start lines from other listeners when FC has none.
- Persist enrichment on the run so Overall / Spawn aggregates can use enriched times when present.

## Non-goals (v1.5)

- Live Gamelog filesystem watch during the run
- ESI / SSO
- Fleet-chat keyword markers
- Full combat analytics (DPS graphs, neuts, scramble drill-down beyond what’s needed for timing)
- Cross-run gamelog dedupe beyond the sealed run’s time window

## Decisions (grilled)

| Topic | Choice |
|-------|--------|
| Planning slice | Roadmap for all; full design for v1.5 gamelog only |
| v1.5 must-ship | Warp vs in-site **and** dead missiles |
| Ingress | Auto-scan after Analyze + **Re-enrich**; path in settings |
| Warp source | FC primary; borrow `Following in warp` from others if FC has none |
| Site split | Markers when present; else break-threshold / in-site heuristic (never leave unknown-only gaps) |
| Dead missiles | Per character + fleet sum; `missiles_per_cycle` from Ammo settings |
| FC identity | Settings name → wallet reward name → fewest follow-warps heuristic |
| Architecture | Enrich-on-Analyze in-process (not live watch, not separate CLI) |
| v2 roadmap | Character wallet journal only into existing event model |

## Architecture

```
Analyze (wallet run sealed)
        │
        ▼
Scan Gamelogs (newest→oldest, wallet window + 1 local day)
        │
        ▼
Parse per Listener → events
        │
        ├── FC = combat + reload primary
        └── Borrow following_warp if FC has none in gap
        │
        ▼
Align to wallet sites → warp / in-site / break
        │
        ▼
Dead missiles (reload_cycles × cycle − hits)
        │
        ▼
Persist enrichment_json on run → EditionFocus
```

### Modules (Rust)

| Module | Responsibility |
|--------|----------------|
| `gamelog_scan` | Resolve files in window; share-mode read (reuse encoding helpers) |
| `gamelog_parse` | Line → typed events (warp, regroup, hit, reload) |
| `enrichment` | FC resolve, borrow, site alignment, missile math |
| `run_desk` / `db` | Trigger enrich, store/overwrite, surface in snapshot |

### Settings

- `gamelogs_dir` — default `%USERPROFILE%\Documents\EVE\logs\Gamelogs`
- `fc_character` — optional string
- Ammo profile (existing): `launchers`, `ammo_per_launcher` → cycle size at enrich time

## Domain algorithms

### Scan window

- Wallet bounds: oldest and newest qualifying payout timestamps on the run.
- Expand start by **one local calendar day** for file selection (UTC vs local filename/mtime skew).
- Consider files newest→oldest; skip files whose session start is before the expanded window; stop when remaining files are entirely older.

### Event kinds

- `following_warp` — notify `Following … in warp`
- `regrouping` — notify `Regrouping to …`
- `combat_hit` — combat lines ending in missile **Hits** (outgoing)
- `reload` — prefer `Loading the … into the Missile Launcher; this will take approximately …` as one cycle (avoid double-count with “run out of charges” alone)

### FC resolution

1. `settings.fc_character` if non-empty and matches a Listener in range  
2. Else character name extracted from wallet description (`CONCORD rewarded X for…`) if that Listener exists  
3. Else Listener with fewest `following_warp` events in the window  

### Warp borrowing

For each site gap, if the FC listener has **zero** `following_warp` in that gap, use the **earliest** `following_warp` from any other listener in the same gap. Combat/reload stats still come from each character’s own log (FC not required for per-char missiles).

### Per-site alignment

For gap before payout *n* (or from `run_start` for first site when set):

1. **Markers present:** warp = warp-start → regroup or first combat; in-site = end of warp → payout (or next warp).  
2. **Else if gap > break threshold:** entire gap is **break** (excluded from averages, as v1).  
3. **Else:** entire gap is **in-site**.

When enrichment is present, session “time spent” / avg clear prefer **in-site** totals; surface warp separately.

### Dead missiles

For each Listener with parsed logs in range:

```
missiles_per_cycle = launchers × ammo_per_launcher   // from Ammo settings
dead = max(0, reload_cycles × missiles_per_cycle − hit_count)
```

Fleet dead = sum of per-character dead.  
**Known bias:** ammo left in magazines never reloaded undercounts expended missiles; document in UI help. Optional later: treat “run out of charges” as end-of-cycle signal.

### Persistence

Store `enrichment_json` on `analytics_runs` (or sibling table keyed by `run_id`). Re-enrich overwrites. Aggregates for Spawn/Overall: if a run has enrichment, use enriched in-site/warp fields; else fall back to v1 gap timing.

## RunDesk surface

Extend amend / commands (names illustrative):

- Auto-enrich after successful `analyze` (can be settings-toggle later; default on)
- `amend({ op: "reenrich_run", run_id? })` or `run_desk_reenrich` → `EditionFocus`
- Snapshot gains optional `enrichment` block: diagnostics, per-site warp/in-site, missile table, resolved FC, listeners used

## UI (Analytics tab)

- Gamelogs path + FC character fields  
- Status: enriching / enriched N listeners / warnings  
- **Re-enrich** on current run  
- Summary: warp time, in-site time, avg in-site, fleet dead missiles (+ expandable per char)  
- Drill-down: warp sec, in-site sec, source (`fc` | `borrowed` | `heuristic`)

## Errors & edges

| Case | Behavior |
|------|----------|
| Missing gamelogs dir | Warn; keep v1 report |
| No listeners in window | Warn; no enrichment |
| Locked file | Skip + diagnostic; continue |
| Partial parse | Enrich what we can + counts |
| Re-enrich with no sealed run | Error |
| Zero reload/hit data | Missile section empty/zero with note |

## Testing seams (for TDD)

1. Gamelog line parser  
2. Scan window / file selection  
3. FC resolution + warp borrow  
4. Site alignment (markers vs heuristic)  
5. Dead-missile math  
6. Persist / overwrite enrichment  

## v2 stub (ESI wallet — not designed in depth)

- OAuth2 SSO; identify app with User-Agent; pin compatibility date; honor Expires/ETag and 420/429  
- Scope: character wallet journal only  
- Paginate → map to same qualifying payout events as paste  
- Ingest into RunDesk event list; paste remains offline fallback  
- Follow `esi` skill discipline; no static data via ESI  

## Follow-on after v1.5

1. Implement v1.5 per writing-plans  
2. Design + implement v2 ESI journal  
3. Optional fleet-chat markers  
4. Hardening: incomplete-magazine missile bias, richer reload detection  
