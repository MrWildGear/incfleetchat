---
last_verified: 2026-08-04
---

# Entities

Canonical glossary lives in root **`CONTEXT.md`**. This file is a short index for agents; prefer CONTEXT wording and “Avoid” notes.

## Overlay

| Term | Meaning |
|------|---------|
| Listener | Character whose newest `Fleet_*.txt` the overlay follows |
| Site (overlay) | Tagged fleet-chat post on the Board until Cleared |
| Tag | Single char `0-9` / `a-z` |
| Board | Visible Sites + status metadata for active fleet log |
| Ran | Local mark that Site was run (does not remove from Board) |
| Phase | `active` \| `overdue` \| `ready` (vs expiry + Ran) |
| Clearable | Clear allowed iff Phase is `ready` |
| Clear | Remove Clearable Site(s) from Board |

## Run analytics

| Term | Meaning |
|------|---------|
| Spawn | INC constellation entry (join key: constellation id) |
| Run | One Analyze commit (wallet, settings snapshot, sites, optional enrichment) |
| Site (analytics) | Qualifying Corporate Reward Payout row on run timeline |
| Break | Gap above threshold, excluded from timing averages |
| RunDesk | Tools analytics document session (not enrichment math) |
| EditionFocus | Always-returned RunDesk snapshot for UI after an op |
| Tray | Staging slot: Manifest or wallet journal |
| Report scope | Overall / Spawn / Run |

## Enrichment & missiles

| Term | Meaning |
|------|---------|
| Enrichment | Gamelog fleet timing + missile stats on site timeline |
| Enrichment snapshot | Persisted (or merged) enrichment document |
| Enrichment pipeline | Scan → enrich → persist/load orchestration |
| Enrichment inputs | Gamelogs dir, FC, launchers, ammo per launcher |
| MissileStat | Per-listener counts on snapshot/site |
| Missile rates | Display module for expended / dead / Hit% / Miss% |
| Joined Results | Wallet report × enrichment Results surface |

## Settings & ammo

| Term | Meaning |
|------|---------|
| Overlay settings | Listener, chatlogs, always-on-top |
| Tools settings | Gamelogs, FC, ammo launchers / per launcher |
| Ammo planner | Tools Ammo tab (outside RunDesk) |

## Persistence keys (informal)

- Overlay marks: `site_id` in `ran_marks` / `cleared_marks`
- Spawn PK: `constellation`
- Run PK: `run_id` (FK → spawn constellation)
