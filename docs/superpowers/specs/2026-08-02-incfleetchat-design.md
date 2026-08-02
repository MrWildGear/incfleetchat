# IncFleetChat Site Timer — Design

**Date:** 2026-08-02  
**Status:** Approved

## Problem

During EVE Online fleet ops, site tags (`0-9`, `a-z`) are posted in fleet chat. Pilots need to know when 20 minutes have passed since each tag’s message timestamp, mark sites they have run, and clear finished entries.

## Goals

- Follow **one** Listener’s most recent fleet chatlog among many simultaneous `Fleet_*.txt` files (~17 on a multi-box machine).
- Show each tag as a Discord-style row: base message + reply with countdown to expiry.
- Local **Ran** mark; **Clear** only when expired **and** ran (per-site and bulk).
- Persist Ran marks across restarts; FancyZones-friendly window with position restore and optional always-on-top.

## Non-goals (v1)

- Posting into EVE chat
- Configurable timer duration
- Multi-Listener follow
- Light theme
- Full chat transcript UI
- TanStack Router / TanStack Query

## Architecture

**Approach:** Event-driven Rust core, thin React shell.

```
EVE Chatlogs → notify watch → resolve Listener file → full re-parse
    → merge SQLite Ran → Board snapshot → Tauri event → Zustand → UI
UI actions → mark_ran / clear_* / settings → Rust core
```

### Tech stack

| Layer | Choice |
|-------|--------|
| Shell | Tauri v2 (Rust) |
| UI | React + TypeScript + Vite |
| Style | Tailwind CSS + shadcn/ui, dark only |
| State | Zustand |
| DB | SQLite via sqlx |
| FS watch | `notify` (debounced) |
| Window | `tauri-plugin-window-state` |

## Product rules

1. **Log selection:** Newest `Fleet_*.txt` whose header **Listener** matches the configured character.
2. **Tags:** Message body is exactly one character in `[0-9A-Za-z]` (trim; store lowercase). Stack duplicates as separate entries.
3. **Timer:** `expires_at = posted_at + 20 minutes`. Log timestamps treated as UTC.
4. **Phases:** `active` (not expired), `overdue` (expired, not clearable), `ready` (expired ∧ ran).
5. **Ran:** Local only; does not remove the row.
6. **Clear:** Only when `clearable` (`ready`). Per-site and bulk “Clear ready”.
7. **Overdue without Ran:** Stays visible with overdue styling until Ran → Clear.
8. **Sort:** Soonest expiry first (overdue at top).
9. **New fleet file:** Fresh live board; old Ran rows remain in DB but are not shown.
10. **Default chatlogs path:** `%USERPROFILE%\Documents\EVE\logs\Chatlogs`.

## Core API (happy-path)

### Primary

- `get_board() → Board`
- `mark_ran(site_id) → Board`
- `clear_ready() → Board`
- `clear_site(site_id) → Board`
- Event: `board-updated` with `Board` payload

### Secondary (settings)

- `get_settings() → AppSettings`
- `set_settings(patch) → AppSettings`
- `list_characters() → string[]` (scan Listener headers)
- `set_always_on_top(bool)`

### Board shape

```ts
type SitePhase = "active" | "overdue" | "ready";

type SiteRow = {
  id: string;
  tag: string;
  speaker: string;
  posted_at: string;   // ISO UTC
  expires_at: string;  // ISO UTC
  ran: boolean;
  phase: SitePhase;
  clearable: boolean;
};

type BoardStatus =
  | { kind: "watching"; character: string; log_name: string }
  | { kind: "no_character" }
  | { kind: "waiting_for_log"; character: string }
  | { kind: "error"; message: string };

type Board = {
  status: BoardStatus;
  sites: SiteRow[];
  ready_count: number;
  updated_at: string;
};
```

UI ticks countdowns locally from `expires_at` (no 1 Hz IPC).

## Data model

### `site_id`

Stable key: hash of `(fleet_log_id, timestamp, speaker, tag, line_ordinal)` so stacked tags and re-parses stay distinct.

### SQLite

- `settings` — character, chatlogs_dir, always_on_top
- `ran_marks` — site_id, marked_at

Window geometry is owned by `tauri-plugin-window-state`.

### Parse format

```
[ YYYY.MM.DD HH:MM:SS ] Speaker > body
```

Header block provides `Listener:` for character matching. Filename pattern `Fleet_{date}_{time}_{charId}.txt`.

## UI (Layout A)

Dark zinc/slate surface; amber active countdown, red overdue, green ran/ready.

- Header: character / status · Pin · Settings · Clear ready
- Rows: speaker + time, tag body, indented reply with countdown, Ran reaction, Clear when clearable
- First run: character dropdown from `list_characters`

## Errors

- Soft no-op on unknown id / non-clearable clear; return updated board
- Debounce watch events ~100–200ms then full re-parse
- Status kinds cover no character, waiting for log, and I/O errors

## Test seams

1. Parser — log text → site candidates  
2. Board merge — candidates + Ran + now → sorted Board  
3. Commands — mark_ran / clear_site / clear_ready rules  
4. Listener resolve — folder fixture + character → correct file  

## Window

Normal overlapped window (FancyZones-compatible). Optional always-on-top. Restore position/size between launches.
