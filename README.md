# IncFleetChat

Desktop overlay for EVE Online fleet site tags. Watches your Listener’s newest `Fleet_*.txt` chatlog, shows 20-minute countdowns, and lets you mark **Ran** / clear expired+ran sites.

## Stack

Tauri v2 (Rust) · React · TypeScript · SQLite (sqlx) · Tailwind · Zustand

## Develop

```bash
npm install
npm run tauri dev
```

## Test

```bash
cd src-tauri
cargo test --lib
```

## First run

1. Open Settings and pick your character (scanned from Chatlogs Listener headers).
2. Join a fleet — tags `0-9` / `a-z` posted as exact single-character messages appear on the board.
3. Use **Ran** when you’ve run a site; **Clear** / **Clear ready** after the timer expires.
