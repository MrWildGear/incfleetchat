# Analytics Data Delete — Design

**Date:** 2026-08-02  
**Status:** Approved (design dialogue)  
**Parent:** [2026-08-02-run-analytics-tools-design.md](./2026-08-02-run-analytics-tools-design.md)  
**App:** IncFleetChat Tools → Analytics

## Problem

Analyze only appends runs/spawns to SQLite. The Scope dropdown accumulates history with no way to remove a bad run, an old INC spawn, or all analytics data.

## Goals

- Delete the **currently selected scope**:
  - **Run** → one Analyze session  
  - **Spawn** → constellation + all its runs  
  - **Overall** → clear all analytics (`analytics_runs` + `spawns`)
- Confirm every destructive action with a dialog naming the target and counts.
- After deleting the last run of a spawn, remove the empty spawn row.
- Return `EditionFocus` after each op (RunDesk snapshot habit).

## Non-goals

- Soft-delete / undo window  
- Multi-select “Manage data” checklist  
- Export-before-delete  
- Clearing wallet/Manifest trays as part of delete  

## Decisions

| Topic | Choice |
|-------|--------|
| What to delete | Run, spawn (+runs), clear all |
| Selection | Current Scope drives Delete; Overall = clear all |
| Confirm | One confirm dialog per action |
| Empty spawn | Auto-delete when last run removed |
| IPC | Dedicated commands (not AmendOp overload) |

## API

```ts
run_desk_delete_run({ runId: string }): Promise<EditionFocus>
run_desk_delete_spawn({ constellation: string }): Promise<EditionFocus>
run_desk_clear_all(): Promise<EditionFocus>
```

### Semantics

**delete_run**
1. `DELETE FROM analytics_runs WHERE run_id = ?`  
2. If no runs remain for that constellation → `DELETE FROM spawns WHERE constellation = ?`  
3. If in-memory `sealed_run_id` equals the deleted run → clear it  
4. Set scope: parent spawn if it still exists, else Overall  
5. Return `EditionFocus`  

**delete_spawn**
1. Delete all runs for constellation, then spawn row (single transaction)  
2. If `sealed_run_id` belongs to a deleted run → clear it  
3. Set scope → Overall  
4. Return `EditionFocus`  

**clear_all**
1. Delete all runs, then all spawns (single transaction)  
2. Always clear `sealed_run_id`  
3. Set scope → Overall  
4. Staging trays (wallet/Manifest text) unchanged  
5. Return `EditionFocus`  

Unknown id → `Err(String)`; catalog/scope unchanged.

Each successful command updates RunDesk’s in-memory `scope` before building the snapshot (same pattern as `focus` / `analyze`).


## UI

- **Delete…** button beside Scope; label:
  - Run → “Delete run…”  
  - Spawn → “Delete spawn…”  
  - Overall → “Clear all analytics…”  
- `window.confirm` (or equivalent) with explicit copy, e.g.  
  - `Delete run {id}? This cannot be undone.`  
  - `Delete spawn {constellation} and {n} runs? This cannot be undone.`  
  - `Delete ALL analytics data ({n} runs, {m} spawns)? This cannot be undone.`  
- On confirm, invoke the matching command; apply returned `EditionFocus`.  
- Single entry point (no second Clear-all control).

## Persistence

Hard SQL delete in a **transaction** for multi-statement ops. Prefer delete runs before spawn to satisfy FK. Optional: add `ON DELETE CASCADE` on `analytics_runs.constellation` in a migration — not required if commands delete in order.

Enrichment / extra columns on the run row are removed with the run.

## Testing seams

1. **DB:** delete_run removes row; last run removes spawn; delete_spawn removes N runs + spawn; clear_all empties both  
2. **RunDesk:** each command returns focus + updated catalog; `sealed_run_id` cleared on delete_run (match), delete_spawn (if sealed run in spawn), and always on clear_all; in-memory scope updated  
3. **UI:** label by scope (manual OK); confirm then invoke. Confirm copy uses counts from the current `EditionFocus` catalog. When Overall + catalog empty: **disable** Delete (no confirm).

## Implementation notes

- `db.rs`: `delete_run`, `delete_spawn`, `clear_all_analytics`  
- `run_desk.rs` + `commands.rs`: wire three Tauri commands  
- `ToolsApp.tsx`: Delete button + confirm  

## Follow-on

- Multi-select prune UI if catalog grows large  
- Soft-delete / undo if mis-clicks become common  
