# Dual adapters for Site Phase

Phase and Clearable are the same rules in two places on purpose: Rust derives them when building and mutating the Board (and gates Clear on the snapshot’s Clearable), while the overlay UI re-derives from `expires_at`, Ran, and the local clock so Clear unlocks without waiting for a board refresh. Wire `phase` / `clearable` may be stale; display must not treat them as live. We reject collapsing to client-only authority (Rust would trust the UI for Clear) and rejecting Rust-only Phase with ~1s board ticks (extra Tauri traffic for snappy Clear).

## Status

accepted

## Considered Options

- **Dual adapters (chosen):** Rust authoritative for Clear mutations; client `derivePhase` for display and ready-count UX; periodic `refresh_board` catches the snapshot up.
- **Client-only Phase:** single derivation in the UI; Rust trusts the client for Clear — rejected (mutation gate must stay on the core).
- **Rust-only Phase with high-frequency refreshes:** one derivation in `board`; UI waits on ticks — rejected (lags Clear unlock or forces noisy ~1s board traffic).
