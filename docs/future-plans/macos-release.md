# macOS release — future plan (stub)

**Status:** Deferred  
**Promotes into:** `.github/workflows/release.yml` matrix (not wired yet)  
**Depends on:** Windows release + updater path landing first  
**Related spec:** `docs/superpowers/specs/2026-08-03-version-release-updater-design.md`

## Intent

Add macOS builds to the IncFleetChat GitHub release pipeline (Apple Silicon and/or Intel), with updater artifacts suitable for published GitHub Releases.

## Out of scope for now

- Implementing runners, signing, or notarization
- Changing the Windows-only workflow

## When promoting this plan, cover

- [ ] Matrix entries: `macos-latest` with `--target aarch64-apple-darwin` and/or `x86_64-apple-darwin` (eve-wrench pattern)
- [ ] Apple code signing identity + notarization secrets
- [ ] Updater artifacts (`.app` / tar.gz + `.sig`) and `latest.json` platform keys for darwin
- [ ] Whether universal binary vs dual-arch releases
- [ ] Product name / DMG naming consistency with `VERSION`
- [ ] Manual test checklist: install → Publish → in-app update on macOS

## Notes

Reuse root `VERSION` + `sync-version` and the same draft→Publish gate as Windows. Do not invent a separate version scheme for macOS.
