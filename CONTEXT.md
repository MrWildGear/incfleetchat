# IncFleetChat

Local Tauri app for EVE Online Incursion fleets: overlay site timer from fleet chat, plus a Run desk for wallet/gamelog analytics.

## Language

### Overlay

**Site**:
A tagged fleet-chat entry (one character `0-9` / `a-z`) with a 20-minute expiry on the overlay board.
_Avoid_: row, timer entry

**Listener**:
The configured character whose fleet chatlog the overlay follows.
_Avoid_: pilot (when meaning the log header), character (when meaning the follow target)

### Run desk / enrichment

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
