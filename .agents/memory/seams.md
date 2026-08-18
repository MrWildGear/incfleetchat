---
last_verified: 2026-08-04
---

# Current seams (Tools / analytics)

Snapshot after the 0.1.x Tools refactors (RunDesk client, Joined Results, EnrichmentInputs, missileRates, settings split). Update `last_verified` when the next analytics refactor lands.

## Ownership map

| Concern | Owns it | Does not own |
|---------|---------|--------------|
| Overlay Board | `AppState` / `board` / `watch` + Zustand `store.ts` | Tools settings, RunDesk |
| Overlay Phase display | UI `derivePhase` | Authoritative Clear gate |
| Overlay Clear gate | Rust Board (`clearable` on snapshot) | Client-only Phase |
| Overlay settings | `get/set_overlay_settings` | Tools prefs |
| Tools settings | `toolsSettings.ts` + DB columns | Board refresh |
| RunDesk session | Rust `run_desk` + `createRunDeskClient` / `useRunDesk` | Enrichment math, ammo UI |
| EditionFocus contract | Every `run_desk_*` command return | Nested report field catalog as the shell API |
| Wallet / Manifest parse → report | Analyze path inside RunDesk / wallet+spawn parse | Gamelog scan |
| Enrichment math | `enrichment.rs` (+ timing helpers) | RunDesk session, Tools settings blob |
| Enrichment orchestration | `enrichment_pipeline` + inputs bag | Persist-before-enrich “prelude” |
| Aggregate enrichment | Merge per-run snapshots for Spawn/Overall | Re-scan gamelogs |
| Joined Results | `joinedResults.ts` + `JoinedResultsView` | RunDesk paste/analyze |
| Missile Hit%/Miss%/expended | `missileRates.ts` only | Ad-hoc UI re-derives |
| Report field types (TS) | `analyticsInternals.ts` | Overlay `types.ts` |
| RunDesk wire types | `runDeskTypes.ts` | Internals dump into ToolsApp |
| Ammo planner | `AmmoPlannerTab` + `ammo.ts` | RunDesk |
| Durable ammo fit prefs | Tools settings (`ammo_launchers`, `ammo_per_launcher`) | localStorage |
| Ephemeral ammo stock/ship | Ammo tab local state | Tools settings |

## Frontend layering (Tools)

```
ToolsApp (shell + tabs)
├── useRunDesk → runDesk.ts client → run_desk_* commands → EditionFocus
├── JoinedResultsView → joinedResults + missileRates + formatAnalytics
└── AmmoPlannerTab → ammo.ts + Tools settings for fit prefs
```

Paste buffers for Manifest/wallet can live in `ToolsApp` UI state; sealed trays live on the desk after paste/analyze.

## Rust layering (Tools-related)

```
commands (Tauri)
├── overlay: board / settings / watch
└── tools:
    ├── tools settings
    ├── RunDesk (session + EditionFocus)
    ├── enrichment_pipeline(EnrichmentInputs)
    └── lookup_vanguard_payout
```

## Hard rules when touching analytics

1. Do not refresh the Board from Tools settings writes.
2. Pass `EnrichmentInputs` into analyze/reenrich — not full overlay/tools settings structs.
3. UI after any desk op renders from returned `EditionFocus`, not a second client cache of report truth.
4. Hit % / Miss % / expended / dead volleys → `missileRates` only.
5. Keep overlay `types.ts` separate from `analyticsInternals.ts` / `runDeskTypes.ts`.
6. Ammo planner stays outside RunDesk.

## Likely next refactor touchpoints

- Deeper split of `ToolsApp.tsx` (still holds paste buffers + enrich form fields)
- Further Rust extract if `run_desk` grows more amend/delete paths
- Any new Results metric → join module + rates module first, not view-local math
