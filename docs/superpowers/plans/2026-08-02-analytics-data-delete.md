# Analytics Data Delete Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the Tools Analytics Scope control delete a run, a spawn (+ all runs), or all analytics data, with confirm dialogs and EditionFocus snapshots.

**Architecture:** Hard SQL deletes on `Db` (`delete_run` / `delete_spawn` / `clear_all_analytics`); `RunDesk` methods update in-memory `scope` + `sealed_run_id` then return `snapshot()`; three dedicated Tauri IPC commands (not `AmendOp`); one Scope-driven Delete button in `ToolsApp.tsx`.

**Tech Stack:** Existing Rust + sqlx SQLite, Tauri v2 invoke, React Tools window. Tests: `#[tokio::test]` + tempfile (same as `db.rs` / `run_desk.rs`).

**Spec:** [docs/superpowers/specs/2026-08-02-analytics-data-delete-design.md](../specs/2026-08-02-analytics-data-delete-design.md)

## Global Constraints

- Delete acts on **current Scope** only (Overall = clear all)
- Confirm every destructive action; disable Delete when Overall + empty catalog
- Last run of a spawn → auto-delete empty spawn row
- Always return `EditionFocus`; trays (wallet/Manifest) unchanged by delete
- Clear `sealed_run_id` whenever that run no longer exists (delete_run match, delete_spawn if sealed run in spawn, always clear_all)
- Unknown id → `Err(String)`; catalog/scope unchanged
- Multi-statement deletes in a **transaction**; delete runs before spawn (FK)
- No soft-delete, multi-select, export-before-delete, or AmendOp overload

## File map

| File | Responsibility |
|------|----------------|
| `src-tauri/src/db.rs` | `delete_run`, `delete_spawn`, `clear_all_analytics` + unit tests |
| `src-tauri/src/run_desk.rs` | `delete_run` / `delete_spawn` / `clear_all` → scope + sealed + snapshot |
| `src-tauri/src/commands.rs` | Register three IPC commands |
| `src/tools/ToolsApp.tsx` | Delete button, confirm copy, invoke |

---

### Task 1: Db delete helpers (TDD)

**Files:**
- Modify: `src-tauri/src/db.rs` (add methods after `save_run` / near other analytics CRUD; extend `#[cfg(test)]` module)
- Test: same file `mod tests`

**Interfaces:**
- Consumes: existing `upsert_spawn`, `save_run`, `load_catalog`, `RunSettings`, `build_report`
- Produces:
  - `pub async fn delete_run(&self, run_id: &str) -> Result<DeleteRunOutcome, String>`
  - `pub async fn delete_spawn(&self, constellation: &str) -> Result<(), String>`
  - `pub async fn clear_all_analytics(&self) -> Result<(), String>`
  - `pub struct DeleteRunOutcome { pub constellation: String, pub spawn_removed: bool }`

- [ ] **Step 1: Write the failing tests**

Append to `db.rs` `mod tests` (reuse `build_report` + `upsert_spawn` + `save_run` pattern from `enrichment_json_round_trips_on_saved_run`):

```rust
#[derive(Debug, PartialEq, Eq)]
pub struct DeleteRunOutcome {
    pub constellation: String,
    pub spawn_removed: bool,
}

// --- place struct next to Db methods in non-test code; tests below ---

#[tokio::test]
async fn delete_run_removes_row_and_keeps_spawn_when_siblings_remain() {
    let dir = tempdir().unwrap();
    let db = Db::open(&dir.path().join("app.db")).await.unwrap();
    let settings = RunSettings::default();
    let report = build_report(&[], &settings);
    db.upsert_spawn("4MY-AB", None).await.unwrap();
    db.save_run("run-a", "4MY-AB", &settings, "", "", &report)
        .await
        .unwrap();
    db.save_run("run-b", "4MY-AB", &settings, "", "", &report)
        .await
        .unwrap();

    let out = db.delete_run("run-a").await.unwrap();
    assert_eq!(
        out,
        DeleteRunOutcome {
            constellation: "4MY-AB".into(),
            spawn_removed: false,
        }
    );
    let cat = db.load_catalog().await.unwrap();
    assert_eq!(cat.runs.len(), 1);
    assert_eq!(cat.runs[0].run_id, "run-b");
    assert_eq!(cat.spawns.len(), 1);
    assert_eq!(cat.spawns[0].run_count, 1);
}

#[tokio::test]
async fn delete_run_last_run_removes_empty_spawn() {
    let dir = tempdir().unwrap();
    let db = Db::open(&dir.path().join("app.db")).await.unwrap();
    let settings = RunSettings::default();
    let report = build_report(&[], &settings);
    db.upsert_spawn("4MY-AB", None).await.unwrap();
    db.save_run("run-only", "4MY-AB", &settings, "", "", &report)
        .await
        .unwrap();

    let out = db.delete_run("run-only").await.unwrap();
    assert!(out.spawn_removed);
    let cat = db.load_catalog().await.unwrap();
    assert!(cat.runs.is_empty());
    assert!(cat.spawns.is_empty());
}

#[tokio::test]
async fn delete_run_unknown_id_errors() {
    let dir = tempdir().unwrap();
    let db = Db::open(&dir.path().join("app.db")).await.unwrap();
    let err = db.delete_run("missing").await.unwrap_err();
    assert!(err.contains("missing") || err.contains("not found"));
}

#[tokio::test]
async fn delete_spawn_removes_runs_and_spawn() {
    let dir = tempdir().unwrap();
    let db = Db::open(&dir.path().join("app.db")).await.unwrap();
    let settings = RunSettings::default();
    let report = build_report(&[], &settings);
    db.upsert_spawn("4MY-AB", None).await.unwrap();
    db.upsert_spawn("OTHER", None).await.unwrap();
    db.save_run("r1", "4MY-AB", &settings, "", "", &report)
        .await
        .unwrap();
    db.save_run("r2", "4MY-AB", &settings, "", "", &report)
        .await
        .unwrap();
    db.save_run("r3", "OTHER", &settings, "", "", &report)
        .await
        .unwrap();

    db.delete_spawn("4MY-AB").await.unwrap();
    let cat = db.load_catalog().await.unwrap();
    assert_eq!(cat.spawns.len(), 1);
    assert_eq!(cat.spawns[0].constellation, "OTHER");
    assert_eq!(cat.runs.len(), 1);
    assert_eq!(cat.runs[0].run_id, "r3");
}

#[tokio::test]
async fn delete_spawn_unknown_errors() {
    let dir = tempdir().unwrap();
    let db = Db::open(&dir.path().join("app.db")).await.unwrap();
    let err = db.delete_spawn("NOPE").await.unwrap_err();
    assert!(err.contains("NOPE") || err.contains("not found"));
}

#[tokio::test]
async fn clear_all_analytics_empties_both_tables() {
    let dir = tempdir().unwrap();
    let db = Db::open(&dir.path().join("app.db")).await.unwrap();
    let settings = RunSettings::default();
    let report = build_report(&[], &settings);
    db.upsert_spawn("4MY-AB", None).await.unwrap();
    db.save_run("r1", "4MY-AB", &settings, "", "", &report)
        .await
        .unwrap();

    db.clear_all_analytics().await.unwrap();
    let cat = db.load_catalog().await.unwrap();
    assert!(cat.runs.is_empty());
    assert!(cat.spawns.is_empty());
}
```

Add `use crate::timing::{build_report, RunSettings};` at top of tests if not already imported in each test.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml delete_run_removes -- --nocapture`

Expected: FAIL (method `delete_run` not found / similar)

- [ ] **Step 3: Implement Db methods**

Add near analytics CRUD in `db.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteRunOutcome {
    pub constellation: String,
    pub spawn_removed: bool,
}

impl Db {
    pub async fn delete_run(&self, run_id: &str) -> Result<DeleteRunOutcome, String> {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let row: Option<(String,)> =
            sqlx::query_as("SELECT constellation FROM analytics_runs WHERE run_id = ?")
                .bind(run_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        let constellation = row
            .map(|(c,)| c)
            .ok_or_else(|| format!("Run {run_id} not found"))?;

        sqlx::query("DELETE FROM analytics_runs WHERE run_id = ?")
            .bind(run_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        let (remaining,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM analytics_runs WHERE constellation = ?")
                .bind(&constellation)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;

        let spawn_removed = if remaining == 0 {
            sqlx::query("DELETE FROM spawns WHERE constellation = ?")
                .bind(&constellation)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            true
        } else {
            false
        };

        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(DeleteRunOutcome {
            constellation,
            spawn_removed,
        })
    }

    pub async fn delete_spawn(&self, constellation: &str) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let (exists,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM spawns WHERE constellation = ?")
                .bind(constellation)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        if exists == 0 {
            return Err(format!("Spawn {constellation} not found"));
        }

        sqlx::query("DELETE FROM analytics_runs WHERE constellation = ?")
            .bind(constellation)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM spawns WHERE constellation = ?")
            .bind(constellation)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn clear_all_analytics(&self) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM analytics_runs")
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM spawns")
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(())
    }
}
```

Note: do **not** recreate FK with `ON DELETE CASCADE` in this task (YAGNI; ordered deletes suffice).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib delete_run_ delete_spawn_ clear_all_analytics_ -- --nocapture`

Expected: PASS (all six)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/db.rs
git commit -m "$(cat <<'EOF'
feat(analytics): add Db helpers to delete runs, spawns, and all analytics

EOF
)"
```

---

### Task 2: RunDesk delete methods (TDD)

**Files:**
- Modify: `src-tauri/src/run_desk.rs`
- Test: `src-tauri/src/run_desk.rs` `mod tests`

**Interfaces:**
- Consumes: `Db::delete_run`, `Db::delete_spawn`, `Db::clear_all_analytics`, `DeleteRunOutcome`, existing `snapshot` / `paste` / `analyze` / `focus` / `amend`
- Produces:
  - `pub async fn delete_run(&self, run_id: &str) -> Result<EditionFocus, String>`
  - `pub async fn delete_spawn(&self, constellation: &str) -> Result<EditionFocus, String>`
  - `pub async fn clear_all(&self) -> Result<EditionFocus, String>`

- [ ] **Step 1: Write the failing tests**

Append to `run_desk.rs` tests. Prefer seeding via `paste` + `analyze` (same as `analyze_saves_run_and_spawn_aggregate`), or insert via `db` then wrap in `RunDesk::new` — either is fine; analyze path exercises sealed_run_id.

```rust
fn sample_manifest() -> &'static str {
    "\
New Null-Sec Incursion: 4MY-AB - Immensea
Constellation
4MY-AB
Region
Immensea
"
}

fn sample_wallet() -> &'static str {
    "\
2026.07.29 23:08\tCorporate Reward Payout\t15,000,000 ISK\t0 ISK\tx\n\
2026.07.29 23:14\tCorporate Reward Payout\t15,000,000 ISK\t0 ISK\ty\n"
}

async fn seed_one_run(desk: &RunDesk) -> EditionFocus {
    desk.paste(Tray::Manifest, sample_manifest()).await.unwrap();
    desk.amend(AmendOp::SetSessionSettings {
        settings: RunSettings {
            space: SpaceBand::LowNull,
            fleet_size: 15,
            expected_isk: 15_000_000,
            lp_per_char: 2_000,
            isk_per_lp: 1400.0,
            break_threshold_minutes: 25,
            run_start: None,
        },
    })
    .await
    .unwrap();
    desk.paste(Tray::Wallet, sample_wallet()).await.unwrap();
    desk.analyze().await.unwrap()
}

#[tokio::test]
async fn delete_run_updates_catalog_scope_and_clears_sealed() {
    let dir = tempdir().unwrap();
    let db = Db::open(&dir.path().join("t.db")).await.unwrap();
    let desk = RunDesk::new(db);
    let focus = seed_one_run(&desk).await;
    let run_id = focus.catalog.runs[0].run_id.clone();
    assert_eq!(focus.sealed_run_id.as_deref(), Some(run_id.as_str()));

    desk.focus(ReportScope::Run {
        run_id: run_id.clone(),
    })
    .await
    .unwrap();

    let after = desk.delete_run(&run_id).await.unwrap();
    assert!(after.catalog.runs.is_empty());
    assert!(after.catalog.spawns.is_empty());
    assert_eq!(after.scope, ReportScope::Overall);
    assert!(after.sealed_run_id.is_none());
    assert_eq!(
        after
            .staging_spawn
            .as_ref()
            .and_then(|s| s.constellation.as_deref()),
        Some("4MY-AB")
    );
}

#[tokio::test]
async fn delete_run_with_sibling_focuses_parent_spawn() {
    let dir = tempdir().unwrap();
    let db = Db::open(&dir.path().join("t.db")).await.unwrap();
    // Two runs same spawn: analyze twice with wallet re-paste
    let desk = RunDesk::new(db);
    let first = seed_one_run(&desk).await;
    let run_a = first.catalog.runs[0].run_id.clone();
    // Re-stage wallet for second analyze (manifest still staged)
    desk.paste(Tray::Wallet, sample_wallet()).await.unwrap();
    let second = desk.analyze().await.unwrap();
    let run_b = second
        .catalog
        .runs
        .iter()
        .find(|r| r.run_id != run_a)
        .unwrap()
        .run_id
        .clone();

    let after = desk.delete_run(&run_a).await.unwrap();
    assert_eq!(after.catalog.runs.len(), 1);
    assert_eq!(after.catalog.runs[0].run_id, run_b);
    assert_eq!(
        after.scope,
        ReportScope::Spawn {
            constellation: "4MY-AB".into()
        }
    );
    // sealed was run_b (last analyze); still present
    assert_eq!(after.sealed_run_id.as_deref(), Some(run_b.as_str()));
}

#[tokio::test]
async fn delete_spawn_clears_sealed_when_sealed_in_spawn() {
    let dir = tempdir().unwrap();
    let db = Db::open(&dir.path().join("t.db")).await.unwrap();
    let desk = RunDesk::new(db);
    let focus = seed_one_run(&desk).await;
    assert!(focus.sealed_run_id.is_some());

    let after = desk.delete_spawn("4MY-AB").await.unwrap();
    assert!(after.catalog.runs.is_empty());
    assert!(after.catalog.spawns.is_empty());
    assert_eq!(after.scope, ReportScope::Overall);
    assert!(after.sealed_run_id.is_none());
}

#[tokio::test]
async fn clear_all_empties_catalog_and_clears_sealed_keeps_trays() {
    let dir = tempdir().unwrap();
    let db = Db::open(&dir.path().join("t.db")).await.unwrap();
    let desk = RunDesk::new(db);
    let _ = seed_one_run(&desk).await;
    // Put something in wallet tray after analyze
    desk.paste(Tray::Wallet, sample_wallet()).await.unwrap();
    let before = desk.open().await.unwrap();
    assert!(before.trays.pending_sites >= 1);
    assert!(before.sealed_run_id.is_some());

    let after = desk.clear_all().await.unwrap();
    assert!(after.catalog.runs.is_empty());
    assert!(after.catalog.spawns.is_empty());
    assert_eq!(after.scope, ReportScope::Overall);
    assert!(after.sealed_run_id.is_none());
    assert!(after.trays.pending_sites >= 1);
}

#[tokio::test]
async fn delete_run_unknown_leaves_state() {
    let dir = tempdir().unwrap();
    let db = Db::open(&dir.path().join("t.db")).await.unwrap();
    let desk = RunDesk::new(db);
    let _ = seed_one_run(&desk).await;
    let before = desk.open().await.unwrap();
    let err = desk.delete_run("nope").await.unwrap_err();
    assert!(!err.is_empty());
    let after = desk.open().await.unwrap();
    assert_eq!(after.catalog.runs.len(), before.catalog.runs.len());
    assert_eq!(after.scope, before.scope);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml delete_run_updates_catalog -- --nocapture`

Expected: FAIL (`delete_run` method missing on `RunDesk`)

- [ ] **Step 3: Implement RunDesk methods**

Add on `impl RunDesk`:

```rust
pub async fn delete_run(&self, run_id: &str) -> Result<EditionFocus, String> {
    let outcome = self.db.delete_run(run_id).await?;
    {
        let mut st = self.inner.lock();
        if st.sealed_run_id.as_deref() == Some(run_id) {
            st.sealed_run_id = None;
        }
        st.scope = if outcome.spawn_removed {
            ReportScope::Overall
        } else {
            ReportScope::Spawn {
                constellation: outcome.constellation,
            }
        };
    }
    self.snapshot().await
}

pub async fn delete_spawn(&self, constellation: &str) -> Result<EditionFocus, String> {
    // Capture sealed id before DB wipe so we can decide clearance.
    let sealed = self.inner.lock().sealed_run_id.clone();
    let sealed_in_spawn = if let Some(ref id) = sealed {
        self.db
            .load_catalog()
            .await
            .map_err(|e| e.to_string())?
            .runs
            .iter()
            .any(|r| r.run_id == *id && r.constellation == constellation)
    } else {
        false
    };

    self.db.delete_spawn(constellation).await?;
    {
        let mut st = self.inner.lock();
        if sealed_in_spawn {
            st.sealed_run_id = None;
        }
        st.scope = ReportScope::Overall;
    }
    self.snapshot().await
}

pub async fn clear_all(&self) -> Result<EditionFocus, String> {
    self.db.clear_all_analytics().await?;
    {
        let mut st = self.inner.lock();
        st.sealed_run_id = None;
        st.scope = ReportScope::Overall;
    }
    self.snapshot().await
}
```

If DB delete fails, do not mutate `scope` / `sealed_run_id` (call DB before lock mutation — already true above).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib delete_run_ delete_spawn_ clear_all_ -- --nocapture`

Expected: PASS (db + run_desk delete tests)

Also run: `cargo test --manifest-path src-tauri/Cargo.toml --lib run_desk -- --skip live_hamilton`

Expected: existing run_desk tests still PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/run_desk.rs
git commit -m "$(cat <<'EOF'
feat(analytics): RunDesk delete_run, delete_spawn, and clear_all

EOF
)"
```

---

### Task 3: Wire Tauri IPC commands

**Files:**
- Modify: `src-tauri/src/commands.rs` (handlers + `generate_handler!`)

**Interfaces:**
- Consumes: `RunDesk::delete_run`, `delete_spawn`, `clear_all`
- Produces: IPC
  - `run_desk_delete_run(run_id: String) -> EditionFocus`
  - `run_desk_delete_spawn(constellation: String) -> EditionFocus`
  - `run_desk_clear_all() -> EditionFocus`

Frontend invoke args use camelCase (`runId`) matching existing `run_desk_reenrich` / Tauri serde.

- [ ] **Step 1: Add command handlers**

Next to other `run_desk_*` commands:

```rust
#[tauri::command]
async fn run_desk_delete_run(
    desk: State<'_, Arc<RunDesk>>,
    run_id: String,
) -> Result<EditionFocus, String> {
    desk.delete_run(&run_id).await
}

#[tauri::command]
async fn run_desk_delete_spawn(
    desk: State<'_, Arc<RunDesk>>,
    constellation: String,
) -> Result<EditionFocus, String> {
    desk.delete_spawn(&constellation).await
}

#[tauri::command]
async fn run_desk_clear_all(desk: State<'_, Arc<RunDesk>>) -> Result<EditionFocus, String> {
    desk.clear_all().await
}
```

Register in `generate_handler![...]`:

```rust
run_desk_delete_run,
run_desk_delete_spawn,
run_desk_clear_all,
```

- [ ] **Step 2: Compile-check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: success (warnings OK if pre-existing)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "$(cat <<'EOF'
feat(analytics): expose delete IPC for run, spawn, and clear all

EOF
)"
```

---

### Task 4: Tools UI Delete button + confirm

**Files:**
- Modify: `src/tools/ToolsApp.tsx` (Scope row ~269–305)

**Interfaces:**
- Consumes: `EditionFocus`, `ReportScope`, catalog counts; IPC from Task 3
- Produces: Scope-driven Delete control with confirm; disabled when Overall + empty catalog

- [ ] **Step 1: Add delete helpers + button beside Scope**

Inside the Analytics tab Scope row (`flex flex-wrap items-end gap-2`), after the Scope `<label>` / `<select>`, add:

```tsx
async function onDeleteScope() {
  if (!focus) return;
  const catalog = focus.catalog;
  const scope = focus.scope;

  if (scope.kind === "run") {
    const ok = window.confirm(
      `Delete run ${scope.run_id}? This cannot be undone.`,
    );
    if (!ok) return;
    try {
      const f = await invoke<EditionFocus>("run_desk_delete_run", {
        runId: scope.run_id,
      });
      applyFocus(f);
    } catch (e) {
      setError(String(e));
    }
    return;
  }

  if (scope.kind === "spawn") {
    const n =
      catalog.spawns.find((s) => s.constellation === scope.constellation)
        ?.run_count ??
      catalog.runs.filter((r) => r.constellation === scope.constellation)
        .length;
    const ok = window.confirm(
      `Delete spawn ${scope.constellation} and ${n} runs? This cannot be undone.`,
    );
    if (!ok) return;
    try {
      const f = await invoke<EditionFocus>("run_desk_delete_spawn", {
        constellation: scope.constellation,
      });
      applyFocus(f);
    } catch (e) {
      setError(String(e));
    }
    return;
  }

  // overall
  const n = catalog.runs.length;
  const m = catalog.spawns.length;
  if (n === 0 && m === 0) return; // button should already be disabled
  const ok = window.confirm(
    `Delete ALL analytics data (${n} runs, ${m} spawns)? This cannot be undone.`,
  );
  if (!ok) return;
  try {
    const f = await invoke<EditionFocus>("run_desk_clear_all");
    applyFocus(f);
  } catch (e) {
    setError(String(e));
  }
}

const deleteDisabled =
  !focus ||
  (focus.scope.kind === "overall" &&
    focus.catalog.runs.length === 0 &&
    focus.catalog.spawns.length === 0);

const deleteLabel =
  focus?.scope.kind === "run"
    ? "Delete run…"
    : focus?.scope.kind === "spawn"
      ? "Delete spawn…"
      : "Clear all analytics…";
```

Button JSX (match existing Tools button classes — look for nearby secondary buttons; if none, use border style consistent with the Scope select):

```tsx
<button
  type="button"
  disabled={deleteDisabled}
  className="rounded border border-border bg-surface-raised px-2 py-1 text-xs text-fg disabled:opacity-40"
  onClick={() => void onDeleteScope()}
>
  {deleteLabel}
</button>
```

Place `onDeleteScope` / derived labels near other handlers (`setScope`, `analyze`, etc.). Prefer `useCallback` only if the file already wraps similar handlers that way — otherwise plain `async function` is fine (match file style).

- [ ] **Step 2: Typecheck / build frontend**

Run: `npm test` then `npm run build`

Expected: PASS (existing vitest + tsc)

Manual smoke (optional in this session): open Tools → Analyze a paste → Scope run → Delete run… → confirm → catalog empties; Overall with empty catalog → button disabled.

- [ ] **Step 3: Commit**

```bash
git add src/tools/ToolsApp.tsx
git commit -m "$(cat <<'EOF'
feat(analytics): Scope-driven Delete with confirm dialogs

EOF
)"
```

---

### Task 5: Final verification + docs commit

**Files:**
- Docs already written: spec + this plan (commit if untracked)

- [ ] **Step 1: Full Rust + frontend gate**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib -- --skip live_hamilton
npm test
npm run build
```

Expected: all green (skip known brittle live chatlog test)

- [ ] **Step 2: Commit design + plan if not already committed**

```bash
git add docs/superpowers/specs/2026-08-02-analytics-data-delete-design.md \
        docs/superpowers/plans/2026-08-02-analytics-data-delete.md
git commit -m "$(cat <<'EOF'
docs(analytics): spec and plan for analytics data delete

EOF
)"
```

(If docs were committed earlier, skip.)

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| delete_run / delete_spawn / clear_all IPC | 3 |
| Hard delete + transaction + runs-before-spawn | 1 |
| Last run removes empty spawn | 1, 2 |
| sealed_run_id clearance rules | 2 |
| Scope update + EditionFocus | 2 |
| Unknown id → Err, no mutation | 1, 2 |
| Trays unchanged | 2 |
| Delete button labels by scope | 4 |
| Confirm copy with counts from catalog | 4 |
| Disable Delete when Overall empty | 4 |
| Not AmendOp | 3 |

## Self-review notes

- No placeholders; signatures consistent (`DeleteRunOutcome`, `clear_all` on desk vs `clear_all_analytics` on db).
- Tauri arg: Rust `run_id` ↔ JS `runId` (same as `run_desk_reenrich`).
- UI is manual-ok for label/confirm; no vitest for `window.confirm` (matches prior analytics UI practice).
