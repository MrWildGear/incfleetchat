# Run Analytics & Ammo Tools — Design

**Date:** 2026-08-02  
**Status:** Approved  
**App:** IncFleetChat (Tauri v2 + React)

## Problem

Incursion FCs track Vanguard site clears in a Google Sheet: wallet journal payouts drive site timing, hourly ISK/LP, and session rates. A separate sheet block plans missile load per ship. The sheet cannot use gamelogs for warp vs in-site splits or dead-missile stats, and the in-game wallet UI caps at ~100 rows per copy.

We want the same post-run analytics and pre-run ammo planning inside IncFleetChat, with a path to richer timing later.

## Goals (v1)

- Replace the sheet’s **wallet-based** dashboard: hourly buckets, session summary, LP valuation, optional per-site drill-down.
- **Paste + append** wallet journal text (“Import more”) to work around the ~100-row in-game limit.
- Identify sites as **Corporate Reward Payout ∧ expected ISK** (Uni Vanguard table preset from space + fleet size, with editable override).
- Site timing from inter-payout gaps; **break** gaps above a threshold excluded from averages; optional manual **run start** for the first site.
- Tag runs with an **INC spawn** (parse Kundalini Manifest–style paste; **constellation** is the join key).
- Each **Analyze** saves a **run**; views: **Overall** / **spawn** / **single run**.
- Separate **Ammo** planner: launchers × ammo/launcher → missiles per cycle; copy **Load into Ship**.
- Open from the timer overlay as a **Tools** window (tabs: Analytics | Ammo).

## Non-goals (v1)

- ESI wallet journal (paste-shaped seam reserved for later)
- Gamelog fusion (warp time, FC without “Following in warp”, dead missiles) — **v1.5**
- Fleet-chat session markers (“start session”, “break”)
- Cross-run wallet dedupe
- Curated constellation dropdown / live INC APIs
- Changing the site-timer board behavior

## Decisions (grilled)

| Topic | Choice |
|-------|--------|
| Scope | Wallet analytics + ammo in v1; gamelog enrichment later |
| Wallet ingress | Paste + append only; ESI later on same event model |
| Site filter | Ref type Corporate Reward Payout **and** expected amount |
| Expected amount | Uni Vanguard preset (space × fleet size) + editable override |
| Timing | Inter-payout gaps; break threshold; optional run start; no fleet-chat markers |
| Shell | One Tools window, tabs Analytics \| Ammo |
| Analytics views | Sheet parity (hourly + summary) + optional per-site drill-down |
| Ammo math | `missiles/cycle = launchers × ammo/launcher`; `load = ceil(stock / ships)`; sites from load ÷ (cycle × reload/site) |
| Persistence | Save each Analyze as a run; spawn aggregates; last staging restore |
| Spawn entry | Paste Manifest → parse; constellation join key; store region/systems/meta |
| Run vs spawn | Each Analyze = one run; spawn aggregates many runs; dedupe within run by timestamp+amount |
| Architecture | Hybrid: Rust parse/timing/DB; React Tools UI + TS ammo |
| Module interface | **RunDesk** (document session) + always-return **EditionFocus** snapshot |

## Architecture

```
Timer overlay ──Open Tools──► Tools window (Analytics | Ammo)
                                 │
                    paste / analyze / focus
                                 ▼
                    Rust RunDesk (trays → commit → query)
                                 │
                    SQLite: spawns, runs, site_events
```

Ammo calculator is pure TypeScript in the Ammo tab (settings-persisted defaults). It does not go through RunDesk.

### v1.5 seam (reserved)

Gamelogs under `%USERPROFILE%\Documents\EVE\logs\Gamelogs`, newest→oldest, stop before oldest wallet timestamp, include ~1 local-day older files (UTC vs local filename skew). FC character lacks “Following … in warp”; pull warp intervals from other listeners. Dead missiles ≈ reload cycles × missiles/cycle − hit lines. Enrichment attaches to the same run timeline without new Analyze UX.

## Domain model

### Spawn

- Join key: constellation (e.g. `4MY-AB`)
- Fields from Manifest parse (editable): region, security, sov holder, staging, HQ, assault systems, vanguard systems, announced-at
- Has many runs

### Run

- Belongs to spawn (constellation required before Analyze; prompt if missing)
- Settings snapshot: space, fleet size, expected_isk, lp_per_char, isk_per_lp, break_threshold, optional run_start
- Raw wallet text (restore / re-analyze)
- Parsed site events + computed summary, hourly buckets

### Site event

- Qualifying wallet row only
- timestamp, isk, fleet_lp (= lp_per_char × fleet_size), gap, is_break, duration contribution

### Payout presets

Embedded Uni Vanguard table (highsec / low-null by fleet size) → suggested expected_isk and lp_per_char. User override wins.

Reference (from Eve Uni; verify if CCP changes numbers):

- Low/null 5–15: 15,000,000 ISK / 2,000 LP
- Low/null 16: 13,875,000 / 1,850; 17: 12,674,000 / 1,690
- Highsec 5–10: 10,395,000 / 1,400; then stepped dilution for 11–13

### Ammo profile (settings)

ammo_stock, launchers, ammo_per_launcher, ship_count, reload_per_site → load_into_ship, sites_capacity

## RunDesk interface

Document-session API. Every mutating or query call returns **EditionFocus** (Design-1 snapshot habit) so the UI never forgets a refetch.

```ts
type Tray = "manifest" | "wallet";

type Scope =
  | { kind: "overall" }
  | { kind: "spawn"; constellation: string }
  | { kind: "run"; runId: string };

type Amend =
  | { op: "clearWalletTray" }
  | { op: "setSessionSettings"; settings: RunSettings }
  | { op: "reopenTrays" }
  | { op: "openRun"; runId: string };

type EditionFocus = {
  trays: { manifest: "empty" | "staged"; walletBatches: number; pendingSites: number };
  spawn: SpawnSummary | null;
  catalog: { spawns: SpawnSummary[]; runs: RunSummary[] };
  scope: Scope;
  report: {
    session: SessionSummary;
    hourly: HourlyBucket[];
    sites: SiteDetail[];
  } | null;
  diagnostics: Diagnostic[];
};

interface RunDesk {
  open(): Promise<EditionFocus>;
  paste(tray: Tray, text: string): Promise<EditionFocus>;
  analyze(): Promise<EditionFocus>;
  focus(scope: Scope): Promise<EditionFocus>;
  amend(op: Amend): Promise<EditionFocus>;
}
```

Happy path: `open` → `paste(manifest)` → `paste(wallet)` × N → `amend(setSessionSettings)` as needed → `analyze` → `focus(overall | spawn | run)`.

Tauri: `run_desk_open`, `run_desk_paste`, `run_desk_analyze`, `run_desk_focus`, `run_desk_amend`.

## Tools UI

- Window ~900×640, resizable; single instance (focus if already open)
- **Analytics:** scope selector, Manifest paste + parsed meta, session knobs, wallet textarea + Import more / Clear, Analyze, hourly table, summary panel, drill-down toggle
- **Ammo:** inputs, derived missiles/cycle, Load into Ship + Copy, sites capacity

## Errors & edges

- Unparseable lines → skip + diagnostic count
- Zero qualifying payouts → do not save run
- Partial Manifest → editable; constellation required to Analyze
- Within-run dedupe on (timestamp, amount)
- First site without run_start → excluded from avg duration (ISK/LP still count)
- Break gaps excluded from time spent / avg site time
- Expected ISK mismatch → diagnostics on how many lines matched

## Testing seams (for TDD)

1. Wallet parser (ref type + amount)
2. Timing engine (gaps, breaks, run_start, hourly, summary)
3. Payout table lookup
4. Manifest parser
5. RunDesk persistence/aggregates/dedupe
6. Ammo math (TS)

## Implementation notes

- Reuse existing dark zinc styling; new window label e.g. `tools`
- SQLite migrations alongside current settings/ran_marks
- Default isk_per_lp = 1400; default break threshold ~25 minutes (tunable)
- Default ammo: 6 launchers × 26 = 156 missiles/cycle (editable)

## Follow-on (ordered)

1. v1.5 Gamelog enrichment (warp vs site, multi-listener follow, dead missiles)
2. ESI wallet journal feed into the same tray/event model
3. Optional fleet-chat session markers
