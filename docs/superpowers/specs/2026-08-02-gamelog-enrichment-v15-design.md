# Gamelog Enrichment (v1.5) & Analytics Roadmap — Design

**Date:** 2026-08-02  
**Status:** Approved (pending user review)  
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
        ├── FC listener preferred for warp-end timing markers
        └── Borrow following_warp if FC has none in gap
            (missile hits/reloads always per-Listener)
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
| `gamelog_parse` | Line → typed events (warp, regroup, hit, reload); extract Listener from header |
| `enrichment` | FC resolve, borrow, site alignment, missile math |
| `run_desk` / `db` | Trigger enrich, store/overwrite, surface in snapshot |

**Listener identity:** Each gamelog file header contains `Listener: <character name>` (same pattern as fleet chatlogs). All matching and “N listeners” counts use that header string (trimmed, case-sensitive as written by EVE).

### Settings

- `gamelogs_dir` — default `%USERPROFILE%\Documents\EVE\logs\Gamelogs`
- `fc_character` — optional string (must match a `Listener:` header when resolving)
- Ammo profile (existing): `launchers`, `ammo_per_launcher` → cycle size at enrich time

## Domain algorithms

### Scan window

- Wallet bounds: oldest and newest qualifying payout timestamps on the run.
- Expand start by **one local calendar day** for file selection (UTC vs local filename/mtime skew).
- Walk files newest→oldest by filename/mtime; include a file if its `Session Started:` (or first event) overlaps the expanded window; stop once remaining candidates are entirely older than the expanded start.

### Event kinds

- `following_warp` — notify `Following … in warp`
- `regrouping` — notify `Regrouping to …`
- `combat_hit` — combat lines with outgoing missile **Hits**
- `reload` — prefer `Loading the … into the Missile Launcher; this will take approximately …` as one cycle (do not also count “run out of charges” as a second cycle)
- `combat_any` — any `(combat)` line (used only as a **timing** end-of-warp signal, not for missile math)

### Roles: timing vs missiles (no conflict)

| Concern | Source |
|---------|--------|
| **When warp ends** (regroup / first combat) | Prefer **FC listener** timeline; if FC has no combat/regroup in the gap after a borrowed warp-start, use earliest regroup/`combat_any` from **any** listener after that warp-start |
| **Hit counts & reload cycles for dead missiles** | **Always per Listener’s own log** — never substitute FC-only combat for other characters’ missile stats |

“FC = combat + reload primary” means: prefer FC’s log for **fleet timing markers** (and FC’s own missile row). It does **not** mean only FC hits count toward fleet dead missiles.

### FC resolution

1. `settings.fc_character` if non-empty and equals some file’s `Listener:` in range  
2. Else character name extracted from wallet description (`CONCORD rewarded X for…`) if that Listener exists in range  
3. Else Listener with fewest `following_warp` events in the window  

### Warp borrowing

For each site gap, if the FC listener has **zero** `following_warp` in that gap, use the **earliest** `following_warp` from any other listener in the same gap as warp-start. Missile stats remain per-character as above.

### Per-site alignment

**Gap** = open interval from previous payout (or optional `run_start` for the first site) to payout *n*.

**First site without `run_start`:** Inherit v1 — no alignable open bound; do not invent warp/in-site for that site’s leading edge; site still counts for ISK/LP; exclude from avg in-site/warp denominators (same as v1 “exclude from avg duration”).

**Markers present** means: at least one usable warp-start in the gap (`following_warp` on FC, or borrowed).

**Multiple warps in one gap:** Use the **last** warp-start before the payout as the transition into the final in-site segment; earlier warp→end pairs in the same gap count as additional warp time (sum), with intervening in-site between end and next warp-start. If two warp-starts occur with no regroup/`combat_any` between them, the interval until the next warp-start is **entirely warp** (terminator = next warp-start).

**Source tag for the site row:** Tag from the **last** warp-start in the gap (`fc` vs `borrowed`). If no markers, `heuristic`.

**Break vs markers:** If markers are present, **do not** classify the whole gap as a break even if duration exceeds the break threshold. Break heuristic applies only when markers are absent.

Algorithm:

1. **Markers present:**  
   - Warp segments: each warp-start → next (regroup | `combat_any` | next warp-start | payout).  
   - In-site: time in the gap that is not in any warp segment.  
   - Source tag: `fc` if warp-start from FC, else `borrowed`.  
2. **No markers, gap > break threshold:** entire gap is **break** (excluded from averages). Source: `heuristic`.  
3. **No markers, gap ≤ break threshold:** entire gap is **in-site**. Source: `heuristic`.

#### Worked example (borrowed warp)

- Payout A at `20:00:00`, payout B at `20:08:00` (8 min gap).  
- FC log: no `Following in warp`; first combat at `20:02:30`.  
- Alt log: `Following … in warp` at `20:00:20`, then quiet.  
- Result: warp-start borrowed `20:00:20`; warp ends at FC first combat `20:02:30` → warp **130s**, in-site **330s**, source `borrowed`.

### Dead missiles

For each Listener with parsed logs in range:

```
missiles_per_cycle = launchers × ammo_per_launcher   // from Ammo settings
dead = max(0, reload_cycles × missiles_per_cycle − hit_count)
```

Fleet dead = sum of per-character dead.  
**Known bias:** ammo left in magazines never reloaded undercounts expended missiles; document in UI help.

### Persistence & EditionFocus shape

Store `enrichment_json` on `analytics_runs`. Re-enrich overwrites.

```ts
type EnrichmentSnapshot = {
  resolved_fc: string | null;
  listeners: string[];          // Listener headers used
  diagnostics: Diagnostic[];
  sites: {
    occurred_at: string;        // payout timestamp
    warp_seconds: number;
    in_site_seconds: number;
    is_break: boolean;
    source: "fc" | "borrowed" | "heuristic";
  }[];
  missiles: {
    listener: string;
    reload_cycles: number;
    hits: number;
    missiles_per_cycle: number;
    dead: number;
  }[];
  totals: {
    warp_seconds: number;
    in_site_seconds: number;
    avg_in_site_seconds: number | null;
    fleet_dead: number;
  };
};
```

Aggregates for Spawn/Overall: if a run has enrichment, use enriched in-site/warp fields; else fall back to v1 gap timing.

## RunDesk surface

- Auto-enrich after successful `analyze` (default on; optional settings toggle later)
- `run_desk_reenrich` / `amend({ op: "reenrich_run", run_id? })` → `EditionFocus` including `enrichment`
- Paste remains unchanged

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

1. Gamelog line parser (+ Listener header)  
2. Scan window / file selection  
3. FC resolution + warp borrow  
4. Site alignment (markers vs heuristic; multi-warp; break does not override markers)  
5. Dead-missile math  
6. Persist / overwrite enrichment  

## v2 stub (ESI wallet — roadmap only, out of v1.5 plan)

Do **not** include ESI work in the v1.5 implementation plan.

- OAuth2 SSO; User-Agent; compatibility date; Expires/ETag; 420/429 discipline  
- Scope: character wallet journal only  
- Paginate → same qualifying payout events as paste  
- Ingest into RunDesk; paste remains offline fallback  

## Follow-on after v1.5

1. Implement v1.5 per writing-plans  
2. Separate design + plan for v2 ESI journal  
3. Optional fleet-chat markers  
4. Hardening: incomplete-magazine missile bias, richer reload detection  
