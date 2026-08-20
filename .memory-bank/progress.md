# Progress

Append-only log of meaningful completed work.

---
## 2026-08-04

### Scaffold agent docs + Memory Bank

- Ran dry-run for `/agent-folder-init` + Memory Bank; confirmed no prior `.agents/`, `.memory-bank/`, `AGENTS.md`, or `.editorconfig`.
- Write pass created:
  - `.agents/` (README, memory/, sessions/ + TEMPLATE)
  - `AGENTS.md`, `.editorconfig`
  - `.memory-bank/` baseline (`STATUS`, `plan`, `checklist`, `RESEARCH`, `BACKLOG`, `progress`, `notes/`, `plans/`)
- Decision: track scaffold in git; do not add to `.gitignore` by default.
- Decision: skip Claude/Cursor platform entry files until explicitly requested.
- Note: repo has no `scripts/scaffold.py`; `memory-bank` CLI not on PATH — files written manually.

### Populate project memory / state

- Wrote `.agents/memory/architecture.md`, `entities.md`, `deployment.md` from README, CONTEXT, code layout, ADR, release docs.
- Actualized `.memory-bank/STATUS.md` for v0.1.3 product surfaces, decisions, risks.
- Updated checklist, plan, backlog (I-005–I-007), RESEARCH with current seams and deferred OS releases.
- Note: `notes/2026-08-04_project-state-snapshot.md`.

### AGENTS conventions

- Expanded `AGENTS.md` with CONTEXT language rules, overlay/Tools seams, Phase dual adapters, missileRates ownership, settings/enrichment boundaries, version/release notes.
- Closed checklist item + backlog I-001.

### Analytics seams note

- Promoted `.agents/memory/seams.md` (ownership map, Tools/Rust layering, hard rules).
- Bank note: `notes/2026-08-04_analytics-seams.md`.
- Closed remaining knowledge-capture checklist item.

### Optional tooling follow-up

- Confirmed optional private-session gitignore is already applied in `.gitignore`.
- Attempted Memory Bank CLI/hook install path; blocked in current shell because `python`/`python3` and `bash` are unavailable.

---
## 2026-08-19

### Wallet-ledger cleanup on scope delete (INC Manifest)

- Fixed orphaned `wallet_imported_payouts` keys: deleting a Spawn/Overall scope left dedup keys behind, so duplicate-detection kept skipping them on future imports.
- Self-contained fix in `src-tauri/src/db.rs`; `run_desk.rs` unchanged (wrappers already call the three db methods).
- Added reader `all_imported_wallet_keys()` right after `record_imported_wallet_keys` — lets tests assert ledger contents without raw SQL inserts; seeds via existing `record_imported_wallet_keys(run_id, &keys)`.
- TDD order: wrote 3 tests first (confirmed red), then implemented:
  - `delete_run`: add `DELETE FROM wallet_imported_payouts WHERE run_id = ?` after the analytics_runs delete.
  - `delete_spawn`: Option A subquery (`WHERE run_id IN (SELECT run_id FROM analytics_runs WHERE constellation = ?)`) placed **before** the runs delete so the subquery still sees them; then runs + spawn as before.
  - `clear_all_analytics`: add `DELETE FROM wallet_imported_payouts` alongside existing deletes.
- Full lib suite green (107 passed). One pre-existing unrelated failure: `resolve::tests::live_hamilton_log_parses_two_ones_when_present` reads a live EVE chat log from `Documents/EVE/logs/`, fails identically on clean tree — not touched by this change.
- No commit/PR made (repo working rules).
- Updated plan log in `docs/superpowers/plans/2026-08-02-analytics-data-delete.md`.

### Tooling installed (Cursor)

- Installed Python 3.12 and jq on Windows.
- Installed Memory Bank from local bundle and completed Cursor adapter wiring.
- Verified Cursor artifacts: `~/.cursor/skills/memory-bank`, `~/.cursor/hooks.json`, `~/.cursor/commands/*.md`, `~/.cursor/AGENTS.md`, `~/.cursor/memory-bank-user-rules.md`.
- Note: install required UTF-8 Python mode in this environment (`PYTHONUTF8=1`) and explicit paths in Git Bash.
