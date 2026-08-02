# IncFleetChat Site Timer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use TDD per seam, then code-reviewer, then commit-work. Steps use checkbox syntax for tracking.

**Goal:** Desktop overlay that follows one Listener’s newest fleet log, tracks stacked site tags with 20-minute countdowns, persists Ran marks, and clears expired+ran entries.

**Architecture:** Rust core (watch → resolve → full re-parse → merge Ran → emit board). React dark Layout A via Zustand + Tauri events.

**Tech Stack:** Tauri v2, Rust, React, TypeScript, sqlx/SQLite, Tailwind, shadcn/ui, Zustand, notify, tauri-plugin-window-state.

## Global Constraints

- Pin by Listener character; tags exact `[0-9A-Za-z]`; stack; 20m fixed; clear only expired∧ran
- Dark theme only; FancyZones-friendly window + window-state restore
- No TanStack Router/Query

## Tasks

See parent plan phases 0–4. Execute TDD vertical slices: parser → site_id → board → commands → resolve → db → watch → IPC → UI.
