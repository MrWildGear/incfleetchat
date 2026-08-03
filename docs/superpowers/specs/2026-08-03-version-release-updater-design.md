# Version, Release Pipeline & Updater — Design

**Date:** 2026-08-03  
**Status:** Approved  
**App:** IncFleetChat  
**Reference:** [eve-wrench `release.yml`](https://github.com/MrWildGear/eve-wrench-app/blob/main/.github/workflows/release.yml)

## Problem

Version is duplicated across `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` with no in-app display. There is no GitHub release pipeline, no portable/installer naming convention, and no in-app updater—users must uninstall and reinstall to get a new build.

## Goals

- Single source of truth for the app version, used for header display and release asset naming.
- Eve-wrench-shaped GitHub Actions release (draft release, git-cliff notes) for **Windows first**.
- Signed Tauri updater so installed users can update without reinstall.
- Public GitHub repo for Releases + updater endpoint.
- Plain stubs for macOS/Linux release work under `docs/future-plans/`.

## Non-goals

- Shipping macOS or Linux builds in this release workflow.
- In-app release notes / changelog UI.
- Skip-this-version, beta channel picker, or forced updates.
- Auto-update for portable `.exe` builds.
- Custom update CDN (GitHub Releases is enough).

## Decisions

| Topic | Choice |
|-------|--------|
| Version file | Root `VERSION` (one-line semver) + `scripts/sync-version.mjs` |
| Sync targets | `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` |
| Release trigger | Push to the `release` **branch** + `workflow_dispatch` |
| Release visibility | Draft first; human Publishes on GitHub (updater only sees published) |
| Platforms now | Windows only |
| Installer | NSIS only (updater path); no MSI as update family |
| Extra artifact | Portable `IncFleetChat_${version}_x64-portable.exe` |
| Changelog | `git-cliff` + `cliff.toml` (eve-wrench pattern) |
| Repo | Public `MrWildGear/incfleetchat` |
| Updater UX | Design C (see UI) |
| Prerelease | Hyphen in version (e.g. `0.2.0-preview.1`) ⇒ GitHub prerelease |

## Architecture

```text
VERSION ──sync-version──► package.json / Cargo.toml / tauri.conf.json
                                    │
                         tauri-action (Windows, NSIS + updater artifacts)
                                    │
                         draft GitHub Release + latest.json + .sig
                                    │
                         human Publishes
                                    │
                         installed app: plugin-updater → check / download / relaunch
```

### Version sync

- Edit `VERSION` only when bumping.
- `npm run sync-version` updates the three config files.
- Run locally after every bump (before commit) and again at the start of the release job so CI never builds a stale config.
- Runtime UI uses Tauri `getVersion()` from the packaged app, not a direct import of `VERSION`.

### Release workflow

`.github/workflows/release.yml`:

1. **changelog** — full-history checkout → `orhun/git-cliff-action` with `cliff.toml` → `## What's Changed` notes (fallback if empty).
2. **publish-tauri** — `windows-latest`; Node LTS + Rust; `npm install`; `npm run sync-version`; detect prerelease from version containing `-`; `tauri-apps/tauri-action@v0` with:
   - `tagName: v__VERSION__`
   - `releaseName: IncFleetChat v__VERSION__`
   - `releaseBody` from changelog
   - `releaseDraft: true`
   - `prerelease` from detect step
   - `includeUpdaterJson: true`
   - env: `GITHUB_TOKEN`, `TAURI_SIGNING_PRIVATE_KEY` (+ password if used)
   - NSIS-only installer for the updater-supported path
3. **Portable upload** — copy the built app binary from `src-tauri/target/release/*.exe` (not the NSIS setup installer under `bundle/nsis/`) → rename to `IncFleetChat_${version}_x64-portable.exe` → `gh release upload` onto the draft tag.

**One-time setup:** `tauri signer generate`; store private key (and password) in Actions secrets; embed public key in `tauri.conf.json` updater config. Create and push public repo `MrWildGear/incfleetchat`.

### Updater (runtime)

- Plugins: `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process` (relaunch).
- Config: `bundle.createUpdaterArtifacts: true`; `plugins.updater.pubkey`; endpoint  
  `https://github.com/MrWildGear/incfleetchat/releases/latest/download/latest.json`;  
  `windows.installMode: passive`.  
  Note: GitHub `/releases/latest` skips drafts and **prereleases**, so hyphenated versions do not drive in-app updates until a non-prerelease is published (intentional; no beta channel).
- Capabilities: allow updater + process relaunch on the main window.
- **Launch:** quiet `check()` after main window is ready; if update available → launch modal.
- **Settings:** manual Check for updates; same prompt path if available.
- **Update now:** download + verify + install → relaunch.
- **Later:** dismiss for session; Settings gear shows accent dot while update still pending.
- **Auto-check errors:** silent. **Manual-check errors:** result line in Settings. **Install fail:** progress modal Retry / Close.
- Portable builds: documented as no auto-update; NSIS install is the supported path.

## UI (Design C)

| Surface | Behavior |
|---------|----------|
| Main header | Muted `v{semver}` on title row after `IncFleetChat` (`text-[10px]` / `text-xs text-muted`); status line unchanged |
| Launch modal | Small centered panel: “Update available” / `{version} is ready.` / **Update now** / **Later** |
| Progress modal | Same shell: Updating… / Installing… / fail with Retry+Close; indeterminate bar; auto-relaunch on success |
| Settings About | Footer above Close: `ABOUT`, `IncFleetChat v{semver}`, **Check for updates**, short result line (`Up to date` / available / error) |
| Deferred signal | After Later: accent dot on Settings button until updated or session ends |
| Tools header | No version |

No in-app release notes, no skip-version, no persistent under-header banner.

## Deferred platforms

Plain stubs (not implemented in CI):

- `docs/future-plans/macos-release.md`
- `docs/future-plans/linux-release.md`

## Testing

| Seam | What |
|------|------|
| `sync-version` | Fixture `VERSION` → asserts three files updated |
| Frontend update UX | Pure helpers/hook: check results → UI states; Vitest, no network |
| cliff.toml | Present and usable by the action |

**Manual after first draft publish:** NSIS install, Publish release, update check, download, relaunch; portable download smoke.

## Success

- Bumping `VERSION` + sync yields one consistent version in configs, header, and release tag/assets.
- Push to `release` (or manual workflow) produces a draft Windows release with cliff notes, NSIS (+ sig / latest.json), and portable exe.
- After Publish, an installed build can check and apply the update without uninstall/reinstall.
- macOS/Linux remain documented only under `docs/future-plans/`.
