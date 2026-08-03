# Linux release — future plan (stub)

**Status:** Deferred  
**Promotes into:** `.github/workflows/release.yml` matrix (not wired yet)  
**Depends on:** Windows release + updater path landing first  
**Related spec:** `docs/superpowers/specs/2026-08-03-version-release-updater-design.md`

## Intent

Add Linux builds to the IncFleetChat GitHub release pipeline so Linux users can install from Releases and (optionally) use the Tauri updater.

## Out of scope for now

- Implementing Ubuntu deps, package formats, or CI matrix rows
- Changing the Windows-only workflow

## When promoting this plan, cover

- [ ] Runner (e.g. `ubuntu-22.04`) and WebKitGTK / appindicator / patchelf deps (eve-wrench pattern)
- [ ] Bundle targets: AppImage vs `.deb` / `.rpm` — pick one primary for updater consistency
- [ ] Updater artifacts + `.sig` and `latest.json` platform keys for linux
- [ ] Asset naming aligned with `VERSION`
- [ ] Manual test checklist: install → Publish → in-app update on Linux

## Notes

Reuse root `VERSION` + `sync-version` and the same draft→Publish gate as Windows. Portable Windows exe has no Linux analogue; prefer one installer family for updater, same rule as NSIS-only on Windows.

Linux builds pull `glib` 0.18.5 via webkit2gtk (RUSTSEC-2024-0429). Accepted until Tauri 3 — see `docs/security/RUSTSEC-2024-0429.md`.
