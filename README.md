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

## Version

Edit the root `VERSION` file, then run:

```bash
npm run sync-version
```

Commit the updated `VERSION` and synced config files.

## Release (Windows)

1. Merge work to `master`, bump `VERSION`, sync, commit.
2. Push `master` to `origin`, then update the `release` branch to that commit (or merge) and push `release`.
3. Wait for the Release workflow; open the **draft** GitHub release; verify NSIS, `.sig`, `latest.json`, and `IncFleetChat_*_x64-portable.exe`.
4. **Publish** the release when ready. Installed (NSIS) apps then see updates via in-app updater.
5. Portable builds do **not** auto-update — download a new portable exe or use the NSIS install for updates.

One-time: generate signing keys with `npm run tauri signer generate`, put the public key in `tauri.conf.json`, and store the private key in Actions secrets (see design spec).
