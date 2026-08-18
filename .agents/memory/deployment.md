---
last_verified: 2026-08-04
---

# Deployment & development

## Local develop

```bash
npm install
npm run tauri dev
```

Frontend-only Vite: `npm run dev` (no Rust/Tauri commands).

## Test

```bash
# Frontend
npm test

# Rust (from src-tauri)
cargo test --lib
```

## Version bump

1. Edit root `VERSION`
2. `npm run sync-version`
3. Commit `VERSION` + synced configs (`package.json`, Cargo.toml, tauri.conf, etc.)

Current version at last verify: **0.1.3**.

## Windows release (current pipeline)

1. Merge to `master`, bump `VERSION`, sync, commit.
2. Push `master`; update/push `release` branch to that commit.
3. Wait for `.github/workflows/release.yml`; open **draft** GitHub release.
4. Verify NSIS, `.sig`, `latest.json`, portable `IncFleetChat_*_x64-portable.exe`.
5. **Publish** when ready — NSIS installs get in-app updater; portable does **not** auto-update.

One-time signing: `npm run tauri signer generate`; public key in `tauri.conf.json`; private key in Actions secrets (see version-release design spec).

## Deferred platforms

- macOS / Linux release stubs: `docs/future-plans/macos-release.md`, `linux-release.md`
- Linux `glib` RUSTSEC-2024-0429 accepted until Tauri 3: `docs/security/RUSTSEC-2024-0429.md`

## First-run UX (overlay)

1. Settings → pick Listener (scanned from Chatlogs headers).
2. Join fleet; single-char tags appear on Board.
3. **Ran** when site run; **Clear** / **Clear ready** after timer + Ran.

## Agent / memory tooling

- Shared agent entry: `AGENTS.md`
- Session notes: `.agents/sessions/`
- Long-term bank: `.memory-bank/`
- Domain source of truth: `CONTEXT.md`
