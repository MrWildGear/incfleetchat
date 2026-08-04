# IncFleetChat

Local Tauri app for EVE Online incursion fleets: a site-timer overlay driven by fleet chat, plus a Tools window for run analytics and ammo planning.

## Language

### Overlay

**Listener**:
The fleet-chat character whose newest `Fleet_*.txt` drives the live site timer board.
_Avoid_: character (when you mean the chatlog selector), pilot

**Site tag**:
A single-character site label posted in fleet chat that starts a 20-minute timer row on the overlay.
_Avoid_: site (when you mean the wallet payout site), tag alone

### Run analytics

**Spawn**:
An INC constellation entry (join key: constellation id) that groups many runs.
_Avoid_: session, constellation (when you mean the spawn record)

**Run**:
One Analyze commit: sealed wallet text, settings snapshot, site events, and optional enrichment for a single desk analyze.
_Avoid_: session, report (the computed view is not the run)

**Site** (analytics):
A qualifying Corporate Reward Payout row on the run timeline, with gap/break/duration fields from wallet timing.
_Avoid_: site tag, payout (the raw journal line)

**Break**:
A gap above the configured threshold, excluded from timing averages.
_Avoid_: pause, downtime

**RunDesk**:
The Tools analytics document session: trays, analyze, focus scope (Overall / Spawn / Run), amend, reenrich, and deletes — not enrichment math itself.
_Avoid_: ToolsApp, analytics service, session manager

**EditionFocus**:
The always-returned RunDesk snapshot the UI renders after an operation (trays, catalog, scope, report, enrichment, diagnostics).
_Avoid_: focus state, view model, board (overlay term)

**Tray**:
A staging slot for pasted text before Analyze — Manifest or wallet journal (wallet may append in batches).
_Avoid_: buffer, clipboard, input

**Report scope**:
Which Runs the current EditionFocus report covers — overall, one Spawn, or one Run.
_Avoid_: filter, view, tab

### Enrichment

**Enrichment**:
Gamelog-derived fleet timing and missile stats aligned onto a run’s site timeline (approach, combat-to-payout, missiles, resolved FC).
_Avoid_: analytics (wallet report), gamelog parse alone

**Enrichment snapshot**:
The persisted enrichment document for one run, or the merged document for a Spawn/Overall scope.
_Avoid_: enrichment report, enrichment JSON

**Enrichment pipeline**:
The orchestration that loads settings and gamelogs, runs enrichment, and persists or loads snapshots for the desk — distinct from pure enrichment math.
_Avoid_: enrichment (the math), RunDesk (owns session, not this job)

**Aggregate enrichment**:
Merging per-run enrichment snapshots into one snapshot for Spawn or Overall focus, without re-scanning gamelogs.
_Avoid_: merge reports (wallet timing merge), overall stats

### Missile rates

**MissileStat**:
Per-listener missile counts persisted on an enrichment snapshot or site (`reload_cycles`, `hits`, `missiles_per_cycle`, `launchers`, `dead`).
_Avoid_: missile row, ammo stat

**Expended**:
Volleys fired from reloads: `reload_cycles × ammo_per_launcher` (ammo per launcher = floor of missiles_per_cycle / launchers).
_Avoid_: shots, cycles fired

**Dead volleys**:
Unused volleys after hits: `max(0, expended − hits)`.
_Avoid_: dead cycles

**Dead missiles**:
Missiles in those unused volleys: `dead_volleys × launchers`. Display always recomputes this; the persisted `MissileStat.dead` is write-time only, not a second display source of truth.
_Avoid_: fleet_dead (that's the enrichment totals field)

**Hit %**:
`hits / expended` when expended > 0; otherwise undefined.
_Avoid_: accuracy, hit rate (when meaning the UI label)

**Miss %**:
`dead_volleys / expended` when expended > 0; otherwise undefined.
_Avoid_: miss rate (when meaning the UI label)

**Missile rates**:
The display module that owns expended, dead volleys, dead missiles, Hit %, Miss %, sums, hour aggregation, and activity filtering for `MissileStat` rows. Callers must not re-derive Hit % / Miss % themselves.
_Avoid_: missileDerived, missile helpers

### Ammo

**Ammo planner**:
The Tools Ammo tab math for stock, launchers, and load-into-ship — outside RunDesk.
_Avoid_: RunDesk (ammo does not go through it)
