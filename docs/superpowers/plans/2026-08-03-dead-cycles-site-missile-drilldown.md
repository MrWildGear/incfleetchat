# Dead Cycles, Hit/Miss %, and Site Missile Drill-down Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add dead-cycle and hit/miss % metrics to enrichment Summary and Listeners, persist per-site listener missile stats, and enable run-scoped site → totals → listeners drill-down.

**Architecture:** Keep storing base `MissileStat` counts only. Compute expended / dead_cycles / hit% / miss% in shared TS helpers. Extend `EnrichmentSite` with `missiles: MissileStat[]` filled during `enrich_run` using existing `(gap_start, payout]` event windows. Tools UI derives display values and adds a two-level drill-down on the per-site panel (run scope only).

**Tech Stack:** Rust (`enrichment`, `analytics_types`, `run_desk` / `db` test fixtures), React Tools (`ToolsApp.tsx`), Vitest, `cargo test`.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-08-03-dead-cycles-site-missile-drilldown-design.md`
- Do **not** change dead formula: `dead = max(0, expended − hits)`
- Do **not** store derived fields (`dead_cycles`, percentages) in JSON
- Dead cycles: `floor(dead / missiles_per_cycle)`
- Hit % / Miss %: vs expended; show `—` when expended = 0
- Site window: `(gap_start, occurred_at]` via existing `events_in_gap`
- Breaks and unalignable first sites: `missiles: []`
- Missile drill-down layers only when `scope.kind === "run"`
- Site sums need not equal run totals
- Fleet dead cycles = sum of per-listener `dead_cycles`
- Percent format: one decimal (e.g. `12.3%`)
- Conventional Commits
- TDD: red → green per task; confirm seams with user only if diverging from this plan’s seams

## File map

| File | Responsibility |
|------|----------------|
| `src/lib/missileDerived.ts` | Pure helpers: expended, deadCycles, hitRate, missRate, formatPercent, sumMissileStats |
| `src/lib/missileDerived.test.ts` | Vitest for helpers |
| `src/lib/analyticsTypes.ts` | Add `missiles` to `EnrichmentSite` |
| `src-tauri/src/analytics_types.rs` | Add `missiles` to `EnrichmentSite` |
| `src-tauri/src/enrichment.rs` | Per-site missile attribution in `enrich_run` |
| `src-tauri/src/run_desk.rs` | Update `EnrichmentSite` test helpers |
| `src-tauri/src/db.rs` | Update enrichment round-trip fixture |
| `src/tools/ToolsApp.tsx` | Summary rows, Listeners columns, site drill levels |

---

### Task 1: TypeScript missile derived helpers

**Files:**
- Create: `src/lib/missileDerived.ts`
- Create: `src/lib/missileDerived.test.ts`

**Interfaces:**
- Consumes: `MissileStat` from `./analyticsTypes`
- Produces:
  - `expended(m: Pick<MissileStat, "reload_cycles" | "missiles_per_cycle">): number`
  - `deadCycles(m: Pick<MissileStat, "dead" | "missiles_per_cycle">): number`
  - `hitRate(m: Pick<MissileStat, "hits" | "reload_cycles" | "missiles_per_cycle">): number | null`
  - `missRate(m: Pick<MissileStat, "dead" | "reload_cycles" | "missiles_per_cycle">): number | null`
  - `formatPercent(rate: number | null): string` — `—` if null; else one decimal + `%`
  - `sumMissileStats(rows: MissileStat[]): { reload_cycles, hits, dead, expended, dead_cycles }` — sums base fields and per-row deadCycles / expended; does not invent a blended `missiles_per_cycle`

- [ ] **Step 1: Write the failing tests**

Create `src/lib/missileDerived.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  deadCycles,
  expended,
  formatPercent,
  hitRate,
  missRate,
  sumMissileStats,
} from "./missileDerived";

describe("missileDerived", () => {
  it("expended is reload_cycles × missiles_per_cycle", () => {
    expect(expended({ reload_cycles: 2, missiles_per_cycle: 156 })).toBe(312);
  });

  it("deadCycles is floor(dead / missiles_per_cycle)", () => {
    expect(deadCycles({ dead: 212, missiles_per_cycle: 156 })).toBe(1);
    expect(deadCycles({ dead: 312, missiles_per_cycle: 156 })).toBe(2);
  });

  it("hitRate and missRate use expended; null when expended is 0", () => {
    expect(hitRate({ hits: 100, reload_cycles: 2, missiles_per_cycle: 156 })).toBeCloseTo(
      100 / 312,
    );
    expect(missRate({ dead: 212, reload_cycles: 2, missiles_per_cycle: 156 })).toBeCloseTo(
      212 / 312,
    );
    expect(hitRate({ hits: 0, reload_cycles: 0, missiles_per_cycle: 156 })).toBeNull();
    expect(missRate({ dead: 0, reload_cycles: 0, missiles_per_cycle: 156 })).toBeNull();
  });

  it("formatPercent shows one decimal or em dash", () => {
    expect(formatPercent(null)).toBe("—");
    expect(formatPercent(0.1234)).toBe("12.3%");
    expect(formatPercent(1)).toBe("100.0%");
  });

  it("sumMissileStats sums bases and per-listener dead cycles", () => {
    const sum = sumMissileStats([
      {
        listener: "A",
        reload_cycles: 2,
        hits: 100,
        missiles_per_cycle: 156,
        dead: 212,
      },
      {
        listener: "B",
        reload_cycles: 1,
        hits: 50,
        missiles_per_cycle: 100,
        dead: 50,
      },
    ]);
    expect(sum.reload_cycles).toBe(3);
    expect(sum.hits).toBe(150);
    expect(sum.dead).toBe(262);
    expect(sum.expended).toBe(312 + 100);
    expect(sum.dead_cycles).toBe(1 + 0); // floor(212/156)+floor(50/100)
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm test -- src/lib/missileDerived.test.ts`

Expected: FAIL (module / exports missing)

- [ ] **Step 3: Implement helpers**

Create `src/lib/missileDerived.ts`:

```ts
import type { MissileStat } from "./analyticsTypes";

export function expended(
  m: Pick<MissileStat, "reload_cycles" | "missiles_per_cycle">,
): number {
  return m.reload_cycles * m.missiles_per_cycle;
}

export function deadCycles(
  m: Pick<MissileStat, "dead" | "missiles_per_cycle">,
): number {
  if (m.missiles_per_cycle === 0) return 0;
  return Math.floor(m.dead / m.missiles_per_cycle);
}

export function hitRate(
  m: Pick<MissileStat, "hits" | "reload_cycles" | "missiles_per_cycle">,
): number | null {
  const e = expended(m);
  if (e === 0) return null;
  return m.hits / e;
}

export function missRate(
  m: Pick<MissileStat, "dead" | "reload_cycles" | "missiles_per_cycle">,
): number | null {
  const e = expended(m);
  if (e === 0) return null;
  return m.dead / e;
}

export function formatPercent(rate: number | null): string {
  if (rate == null || !Number.isFinite(rate)) return "—";
  return `${(rate * 100).toFixed(1)}%`;
}

export type MissileSum = {
  reload_cycles: number;
  hits: number;
  dead: number;
  expended: number;
  dead_cycles: number;
};

export function sumMissileStats(rows: MissileStat[]): MissileSum {
  return rows.reduce<MissileSum>(
    (acc, m) => ({
      reload_cycles: acc.reload_cycles + m.reload_cycles,
      hits: acc.hits + m.hits,
      dead: acc.dead + m.dead,
      expended: acc.expended + expended(m),
      dead_cycles: acc.dead_cycles + deadCycles(m),
    }),
    { reload_cycles: 0, hits: 0, dead: 0, expended: 0, dead_cycles: 0 },
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm test -- src/lib/missileDerived.test.ts`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/missileDerived.ts src/lib/missileDerived.test.ts
git commit -m "feat(analytics): add missile expended/dead-cycle/hit-rate helpers"
```

---

### Task 2: Schema — `EnrichmentSite.missiles`

**Files:**
- Modify: `src/lib/analyticsTypes.ts` (`EnrichmentSite`)
- Modify: `src-tauri/src/analytics_types.rs` (`EnrichmentSite`)
- Modify: `src-tauri/src/run_desk.rs` (`fn site` test helper ~708)
- Modify: `src-tauri/src/db.rs` (enrichment round-trip fixture ~659)
- Modify: `src-tauri/src/analytics_types.rs` tests if any construct `EnrichmentSite`
- Test: extend stale JSON test / add deserialize test that missing `missiles` fails

**Interfaces:**
- Consumes: existing `MissileStat`
- Produces: `EnrichmentSite.missiles: Vec<MissileStat>` / `MissileStat[]` (required field)

- [ ] **Step 1: Write failing Rust deserialize test**

In `src-tauri/src/analytics_types.rs` `mod tests`, add:

```rust
    #[test]
    fn site_without_missiles_field_fails_to_deserialize() {
        let json = r#"{
            "resolved_fc": null,
            "listeners": [],
            "diagnostics": [],
            "sites": [{
                "occurred_at": "2026-01-01T00:00:00Z",
                "approach_seconds": 10,
                "combat_to_payout_seconds": 20,
                "is_break": false,
                "source": "fleet"
            }],
            "missiles": [],
            "totals": {
                "approach_seconds": 10,
                "combat_to_payout_seconds": 20,
                "avg_combat_to_payout_seconds": 20.0,
                "fleet_dead": 0
            }
        }"#;
        assert!(serde_json::from_str::<EnrichmentSnapshot>(json).is_err());
    }
```

(After the field exists, this fails deserialize because `missiles` is missing on the site — that is the intended stale signal. Write the test **before** adding the field: today it may still fail for other reasons or succeed; after adding a required `missiles` field without default, the assert `is_err()` must hold.)

- [ ] **Step 2: Run the new test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml site_without_missiles_field_fails_to_deserialize -- --nocapture`

Expected: before the field exists, adjust expectations — if current JSON deserializes OK, the test should FAIL (`is_err` false). That is the red step.

- [ ] **Step 3: Add the field in Rust and TS; fix constructors**

`src-tauri/src/analytics_types.rs`:

```rust
pub struct EnrichmentSite {
    pub occurred_at: DateTime<Utc>,
    pub approach_seconds: Option<i64>,
    pub combat_to_payout_seconds: Option<i64>,
    pub is_break: bool,
    pub source: EnrichmentSource,
    pub missiles: Vec<MissileStat>,
}
```

`src/lib/analyticsTypes.ts`:

```ts
export type EnrichmentSite = {
  occurred_at: string;
  approach_seconds: number | null;
  combat_to_payout_seconds: number | null;
  is_break: boolean;
  source: EnrichmentSource;
  missiles: MissileStat[];
};
```

Update every Rust `EnrichmentSite { ... }` literal to include `missiles: vec![]` (or real rows in fixtures):

- `src-tauri/src/enrichment.rs` — the `sites.push(EnrichmentSite { ... })` in `enrich_run` (temporary `missiles: vec![]` until Task 3)
- `src-tauri/src/run_desk.rs` — `fn site(...)` → `missiles: vec![]`
- `src-tauri/src/db.rs` — round-trip fixture → `missiles: vec![]`

- [ ] **Step 4: Re-run deserialize test + compile tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml site_without_missiles_field_fails_to_deserialize
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Expected: PASS (fix any remaining missing-field compile errors first)

- [ ] **Step 5: Commit**

```bash
git add src/lib/analyticsTypes.ts src-tauri/src/analytics_types.rs src-tauri/src/enrichment.rs src-tauri/src/run_desk.rs src-tauri/src/db.rs
git commit -m "feat(enrichment): require per-site missiles on EnrichmentSite"
```

---

### Task 3: Per-site missile attribution in `enrich_run`

**Files:**
- Modify: `src-tauri/src/enrichment.rs`
- Test: same file `mod tests`

**Interfaces:**
- Consumes: `events_in_gap`, `MissileStat`, `missiles_per_cycle`, clipped `logs`
- Produces: each non-break site with `gap_start` gets `missiles` for **every** listener (parity with run-level `missile_stats`); breaks / no `gap_start` get `missiles: []`
- Run-level `missiles` unchanged (full clipped window)

- [ ] **Step 1: Write failing tests**

Append to `enrichment.rs` tests:

```rust
    #[test]
    fn site_missiles_count_only_events_in_gap() {
        // Site0 payout 20:10, site1 payout 20:20. run_start 20:00.
        // Gap1 = (20:10, 20:20]: reload+50 hits inside; reload+hits in gap0 must not appear on site1.
        let gunner = listener_log(
            "Gunner",
            vec![
                ev(20, 5, 0, GamelogEventKind::Reload),
                ev(20, 6, 0, GamelogEventKind::CombatHit),
                ev(20, 12, 0, GamelogEventKind::Reload),
            ]
            .into_iter()
            .chain((0..50).map(|i| ev(20, 15, i % 60, GamelogEventKind::CombatHit)))
            .collect(),
        );
        let snap = enrich_run(
            &[gunner],
            &[ts(20, 10, 0), ts(20, 20, 0)],
            &[Some(600), Some(600)],
            25,
            Some(ts(20, 0, 0)),
            156,
            None,
            None,
        );

        assert!(snap.sites[0].missiles.iter().any(|m| m.listener == "Gunner"));
        let s0 = snap.sites[0]
            .missiles
            .iter()
            .find(|m| m.listener == "Gunner")
            .unwrap();
        assert_eq!(s0.reload_cycles, 1);
        assert_eq!(s0.hits, 1);

        let s1 = snap.sites[1]
            .missiles
            .iter()
            .find(|m| m.listener == "Gunner")
            .unwrap();
        assert_eq!(s1.reload_cycles, 1);
        assert_eq!(s1.hits, 50);
        assert_eq!(s1.dead, 156 - 50);
    }

    #[test]
    fn break_site_has_empty_missiles_but_run_totals_keep_break_gap_events() {
        // Short site then long break (>25m). Reload during break gap still in run-level missiles.
        let gunner = listener_log(
            "Gunner",
            vec![
                ev(20, 5, 0, GamelogEventKind::Reload),
                ev(20, 40, 0, GamelogEventKind::Reload), // inside break gap after 20:10
            ],
        );
        let snap = enrich_run(
            &[gunner],
            &[ts(20, 10, 0), ts(21, 0, 0)],
            &[Some(600), None], // second site non-countable → break if gap > threshold
            25,
            Some(ts(20, 0, 0)),
            156,
            None,
            None,
        );

        assert!(snap.sites[1].is_break);
        assert!(snap.sites[1].missiles.is_empty());
        let run = snap.missiles.iter().find(|m| m.listener == "Gunner").unwrap();
        assert_eq!(run.reload_cycles, 2);
    }

    #[test]
    fn unalignable_first_site_has_empty_missiles() {
        let gunner = listener_log(
            "Gunner",
            vec![ev(20, 1, 0, GamelogEventKind::Reload)],
        );
        let snap = enrich_run(
            &[gunner],
            &[ts(20, 10, 0)],
            &[Some(600)],
            25,
            None, // no run_start → no gap_start
            156,
            None,
            None,
        );
        assert!(snap.sites[0].missiles.is_empty());
    }
```

- [ ] **Step 2: Run tests — expect FAIL**

Run: `cargo test --manifest-path src-tauri/Cargo.toml site_missiles_count_only_events_in_gap break_site_has_empty_missiles unalignable_first_site_has_empty_missiles`

Expected: FAIL (empty missiles / wrong counts)

- [ ] **Step 3: Implement attribution**

Refactor `missile_stats` to accept an optional gap, or add:

```rust
fn missile_stats_in_gap(
    logs: &[ListenerLog],
    missiles_per_cycle: u32,
    gap_start: DateTime<Utc>,
    gap_end: DateTime<Utc>,
) -> Vec<MissileStat> {
    logs.iter()
        .map(|l| {
            let events = events_in_gap(l, gap_start, gap_end);
            let reload_cycles = events
                .iter()
                .filter(|e| e.kind == GamelogEventKind::Reload)
                .count() as u32;
            let hits = events
                .iter()
                .filter(|e| e.kind == GamelogEventKind::CombatHit)
                .count() as u32;
            let expended = reload_cycles.saturating_mul(missiles_per_cycle);
            let dead = expended.saturating_sub(hits);
            MissileStat {
                listener: l.listener.clone(),
                reload_cycles,
                hits,
                missiles_per_cycle,
                dead,
            }
        })
        .collect()
}
```

In the `enrich_run` site loop, when pushing `EnrichmentSite`:

```rust
        let site_missiles = match gap_start {
            Some(gs) if !is_break => missile_stats_in_gap(logs, missiles_per_cycle, gs, occurred_at),
            _ => Vec::new(),
        };

        sites.push(EnrichmentSite {
            occurred_at,
            approach_seconds,
            combat_to_payout_seconds,
            is_break,
            source,
            missiles: site_missiles,
        });
```

Keep run-level `let missiles = missile_stats(logs, missiles_per_cycle);` as today.

- [ ] **Step 4: Run tests — expect PASS**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib enrichment::`

Expected: PASS (fix any flaky break detection if gap seconds differ — adjust times so second gap > 25 minutes)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/enrichment.rs
git commit -m "feat(enrichment): attribute missile stats per site gap"
```

---

### Task 4: Enrichment Summary + Listeners UI

**Files:**
- Modify: `src/tools/ToolsApp.tsx`

**Interfaces:**
- Consumes: `sumMissileStats`, `deadCycles`, `hitRate`, `missRate`, `formatPercent` from `missileDerived`; `formatCount`; `focus.enrichment.missiles`
- Produces: Summary rows Expended → Hits → Dead missiles → Dead cycles → Hit % → Miss %; Listeners columns updated

- [ ] **Step 1: Add imports and fleet sum memo**

Near other imports:

```tsx
import {
  deadCycles,
  formatPercent,
  hitRate,
  missRate,
  sumMissileStats,
} from "../lib/missileDerived";
```

Inside `ToolsApp`, after `visibleMissiles`:

```tsx
  const fleetMissileSum = useMemo(
    () => sumMissileStats(focus?.enrichment?.missiles ?? []),
    [focus?.enrichment?.missiles],
  );
```

- [ ] **Step 2: Replace Dead missiles-only enrichment block**

Replace the single Dead missiles `Row` (+ keep the note) with:

```tsx
                  <Row
                    label="Expended"
                    value={formatCount(fleetMissileSum.expended)}
                  />
                  <Row
                    label="Hits"
                    value={formatCount(fleetMissileSum.hits)}
                  />
                  <Row
                    label="Dead missiles"
                    value={formatCount(focus.enrichment.totals.fleet_dead)}
                  />
                  <Row
                    label="Dead cycles"
                    value={formatCount(fleetMissileSum.dead_cycles)}
                  />
                  <Row
                    label="Hit %"
                    value={formatPercent(
                      fleetMissileSum.expended === 0
                        ? null
                        : fleetMissileSum.hits / fleetMissileSum.expended,
                    )}
                  />
                  <Row
                    label="Miss %"
                    value={formatPercent(
                      fleetMissileSum.expended === 0
                        ? null
                        : fleetMissileSum.dead / fleetMissileSum.expended,
                    )}
                  />
                  <p className="text-[10px] text-muted">
                    Incomplete magazines can undercount dead missiles.
                  </p>
```

- [ ] **Step 3: Update Listeners table headers and cells**

Header row:

```tsx
                      <th className="px-2 py-1">Listener</th>
                      <th className="px-2 py-1">Reload cycles</th>
                      <th className="px-2 py-1">Hits</th>
                      <th className="px-2 py-1">Missiles/cycle</th>
                      <th className="px-2 py-1">Dead cycles</th>
                      <th className="px-2 py-1">Dead missiles</th>
                      <th className="px-2 py-1">Hit %</th>
                      <th className="px-2 py-1">Miss %</th>
```

Body cells after missiles/cycle:

```tsx
                        <td className="px-2 py-1">{formatCount(deadCycles(m))}</td>
                        <td className="px-2 py-1">{formatCount(m.dead)}</td>
                        <td className="px-2 py-1">{formatPercent(hitRate(m))}</td>
                        <td className="px-2 py-1">{formatPercent(missRate(m))}</td>
```

- [ ] **Step 4: Typecheck**

Run: `npx tsc --noEmit`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/tools/ToolsApp.tsx
git commit -m "feat(tools): show expended, dead cycles, and hit/miss % for missiles"
```

---

### Task 5: Per-site drill-down levels (run scope)

**Files:**
- Modify: `src/tools/ToolsApp.tsx`

**Interfaces:**
- Consumes: `focus.scope.kind`, `enrichmentByTime`, `EnrichmentSite.missiles`, `sumMissileStats`, `hasMissileActivity`, helpers from Task 1
- Produces: drill state `siteDrill: null | { level: 1 | 2; occurredAt: string }`

- [ ] **Step 1: Add drill state**

```tsx
  const [siteDrill, setSiteDrill] = useState<null | {
    level: 1 | 2;
    occurredAt: string;
  }>(null);
```

When closing the drilldown panel or changing scope, clear it:

```tsx
  // wherever setShowDrilldown(false) / setScope succeeds — also setSiteDrill(null)
```

- [ ] **Step 2: Replace per-site panel body with level switching**

Keep the existing panel shell. Inside the scroll area:

**Level 0** (when `siteDrill === null`): existing table. For each row:

```tsx
const canDrill =
  focus.scope.kind === "run" &&
  !!e &&
  e.missiles.some(hasMissileActivity);

// on <tr>:
className={...}
onClick={canDrill ? () => setSiteDrill({ level: 1, occurredAt: s.occurred_at }) : undefined}
style={canDrill ? { cursor: "pointer" } : undefined}
```

Do **not** enable click-through when `scope.kind !== "run"` even if aggregated sites carry `missiles` arrays.

**Level 1** (`siteDrill.level === 1`):

```tsx
const siteEnrich = focus.enrichment?.sites.find(
  (x) => x.occurred_at === siteDrill.occurredAt,
);
const siteSum = sumMissileStats(siteEnrich?.missiles ?? []);
```

Show header actions: Back (`setSiteDrill(null)`), and “Listeners” (`setSiteDrill({ ...siteDrill, level: 2 })`).

Rows/labels: Reload cycles, Hits, Dead cycles, Dead missiles, Hit %, Miss % using `siteSum` (Hit/Miss from `siteSum.hits / siteSum.expended` with null when expended 0).

**Level 2:** table like Listeners over `(siteEnrich?.missiles ?? []).filter(hasMissileActivity)`; Back sets `level: 1`.

- [ ] **Step 3: Typecheck + unit tests still green**

Run:

```bash
npx tsc --noEmit
npm test
```

Expected: PASS

- [ ] **Step 4: Manual smoke (when Tauri available)**

1. Open Tools → Analyze/Re-enrich a run with gamelogs.
2. Confirm Summary expended/hits/dead cycles/%.
3. Listeners columns include Dead cycles + %.
4. Per-site: click a combat site → totals → listeners; Back works; break rows not clickable.
5. Switch to Overall: sites not clickable for missiles.

- [ ] **Step 5: Commit**

```bash
git add src/tools/ToolsApp.tsx
git commit -m "feat(tools): add run-scoped site missile drill-down layers"
```

---

### Task 6: Spec coverage self-check + CI gate

**Files:** none new (verification only)

- [ ] **Step 1: Map spec → tasks**

| Spec requirement | Task |
|------------------|------|
| dead_cycles formula + helpers | 1 |
| hit/miss % + format | 1, 4 |
| EnrichmentSite.missiles schema / stale | 2 |
| Per-site gap attribution | 3 |
| Breaks / unalignable empty | 3 |
| Summary expended→… | 4 |
| Listeners columns | 4 |
| Site levels 0–2 run-only | 5 |
| Aggregate no click-through | 5 |

- [ ] **Step 2: Run full frontend + Rust lib tests**

```bash
npm test
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Expected: PASS

- [ ] **Step 3: Commit only if Step 2 found fixes**

If fixes were needed, commit them with Conventional Commits; otherwise no empty commit.

---

## Self-review (author)

1. **Spec coverage:** All goals in the design doc map to Tasks 1–5; Task 6 is verification.
2. **Placeholders:** None — concrete code, commands, and assertions.
3. **Types:** `EnrichmentSite.missiles: MissileStat[]` / `Vec<MissileStat>` consistent across TS/Rust/UI.
4. **Click rule:** Plan uses `missiles.some(hasMissileActivity)` so all-zero listener rows (if emitted) do not open a useless drill; breaks/unalignable stay `[]`.
