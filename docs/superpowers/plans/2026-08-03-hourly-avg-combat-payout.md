# Hourly Avg Combat→payout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show **Avg combat→payout** to the right of **Avg site** in the Tools Results hour table, via a UI join over enrichment sites.

**Architecture:** Pure helper in `src/lib` averages non-null `combat_to_payout_seconds` for enrichment sites whose UTC hour floor matches the wallet `hour_start`. `ToolsApp` adds the column and calls the helper. No `HourlyBucket` / report JSON changes.

**Tech Stack:** TypeScript, Vitest, React Tools window (`ToolsApp.tsx`).

## Global Constraints

- Spec: `docs/superpowers/specs/2026-08-03-hourly-avg-combat-payout-design.md`
- Column header exactly: `Avg combat→payout`
- Place immediately right of **Avg site**
- Mean of non-null `combat_to_payout_seconds` for sites in that UTC hour; else `null` → UI `—`
- UTC hour floor of `occurred_at` (same idea as Rust `hour_floor`)
- UI-only — do **not** change `HourlyBucket` or wallet report schema
- Do **not** change Summary / enrichment / Approach / combat start rules
- TDD for the helper; Conventional Commits

## File map

| File | Responsibility |
|------|----------------|
| `src/lib/hourlyAvgCombat.ts` | UTC hour floor + avg combat for one hour |
| `src/lib/hourlyAvgCombat.test.ts` | Vitest coverage |
| `src/tools/ToolsApp.tsx` | Hour table header, cell, empty colspan |

---

### Task 1: Pure helper + tests

**Files:**
- Create: `src/lib/hourlyAvgCombat.ts`
- Create: `src/lib/hourlyAvgCombat.test.ts`

**Interfaces:**
- Consumes: enrichment-like sites `{ occurred_at: string; combat_to_payout_seconds: number | null }`
- Produces:
  - `utcHourFloorMs(iso: string): number`
  - `avgCombatToPayoutForHour(hourStartIso: string, sites: readonly { occurred_at: string; combat_to_payout_seconds: number | null }[]): number | null`

- [ ] **Step 1: Write failing tests**

Create `src/lib/hourlyAvgCombat.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  avgCombatToPayoutForHour,
  utcHourFloorMs,
} from "./hourlyAvgCombat";

describe("utcHourFloorMs", () => {
  it("floors to the start of the UTC hour", () => {
    expect(utcHourFloorMs("2026-08-03T14:37:22.000Z")).toBe(
      Date.parse("2026-08-03T14:00:00.000Z"),
    );
  });
});

describe("avgCombatToPayoutForHour", () => {
  const hour = "2026-08-03T14:00:00.000Z";

  it("returns null when no sites match the hour", () => {
    expect(
      avgCombatToPayoutForHour(hour, [
        {
          occurred_at: "2026-08-03T15:10:00.000Z",
          combat_to_payout_seconds: 100,
        },
      ]),
    ).toBeNull();
  });

  it("returns null when matching sites have only null combat", () => {
    expect(
      avgCombatToPayoutForHour(hour, [
        {
          occurred_at: "2026-08-03T14:20:00.000Z",
          combat_to_payout_seconds: null,
        },
      ]),
    ).toBeNull();
  });

  it("averages non-null combat for sites in the UTC hour", () => {
    expect(
      avgCombatToPayoutForHour(hour, [
        {
          occurred_at: "2026-08-03T14:05:00.000Z",
          combat_to_payout_seconds: 100,
        },
        {
          occurred_at: "2026-08-03T14:55:00.000Z",
          combat_to_payout_seconds: 200,
        },
        {
          occurred_at: "2026-08-03T14:30:00.000Z",
          combat_to_payout_seconds: null,
        },
        {
          occurred_at: "2026-08-03T15:01:00.000Z",
          combat_to_payout_seconds: 999,
        },
      ]),
    ).toBe(150);
  });

  it("returns null for empty sites", () => {
    expect(avgCombatToPayoutForHour(hour, [])).toBeNull();
  });
});
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
npx vitest run src/lib/hourlyAvgCombat.test.ts
```

Expected: FAIL (module / exports missing).

- [ ] **Step 3: Implement helper**

Create `src/lib/hourlyAvgCombat.ts`:

```ts
export function utcHourFloorMs(iso: string): number {
  const d = new Date(iso);
  return Date.UTC(
    d.getUTCFullYear(),
    d.getUTCMonth(),
    d.getUTCDate(),
    d.getUTCHours(),
    0,
    0,
    0,
  );
}

export function avgCombatToPayoutForHour(
  hourStartIso: string,
  sites: readonly {
    occurred_at: string;
    combat_to_payout_seconds: number | null;
  }[],
): number | null {
  const hourMs = utcHourFloorMs(hourStartIso);
  const values: number[] = [];
  for (const s of sites) {
    if (utcHourFloorMs(s.occurred_at) !== hourMs) continue;
    if (s.combat_to_payout_seconds == null) continue;
    values.push(s.combat_to_payout_seconds);
  }
  if (values.length === 0) return null;
  return values.reduce((a, b) => a + b, 0) / values.length;
}
```

- [ ] **Step 4: Run tests — expect PASS**

```bash
npx vitest run src/lib/hourlyAvgCombat.test.ts
```

Expected: PASS (4 describe cases / all its).

- [ ] **Step 5: Commit**

```bash
git add src/lib/hourlyAvgCombat.ts src/lib/hourlyAvgCombat.test.ts
git commit -m "feat(tools): avg combat→payout helper for hourly buckets"
```

On Windows PowerShell use a here-string for `-m`.

---

### Task 2: Wire hour table column

**Files:**
- Modify: `src/tools/ToolsApp.tsx` (hour table ~lines 690–720)

**Interfaces:**
- Consumes: `avgCombatToPayoutForHour` from `../lib/hourlyAvgCombat`; `focus?.enrichment?.sites ?? []`
- Produces: hour table column **Avg combat→payout** after **Avg site**

- [ ] **Step 1: Import helper**

At the top of `ToolsApp.tsx` with other lib imports, add:

```ts
import { avgCombatToPayoutForHour } from "../lib/hourlyAvgCombat";
```

- [ ] **Step 2: Add header and cells**

Find the hour table header:

```tsx
                    <th className="px-2 py-1">Hour</th>
                    <th className="px-2 py-1">Total ISK</th>
                    <th className="px-2 py-1">Total LP</th>
                    <th className="px-2 py-1">Sites</th>
                    <th className="px-2 py-1">Avg site</th>
```

Replace with:

```tsx
                    <th className="px-2 py-1">Hour</th>
                    <th className="px-2 py-1">Total ISK</th>
                    <th className="px-2 py-1">Total LP</th>
                    <th className="px-2 py-1">Sites</th>
                    <th className="px-2 py-1">Avg site</th>
                    <th className="px-2 py-1">Avg combat→payout</th>
```

In each hour row, after the Avg site `<td>`, add:

```tsx
                      <td className="px-2 py-1">
                        {formatDuration(
                          avgCombatToPayoutForHour(
                            h.hour_start,
                            focus?.enrichment?.sites ?? [],
                          ),
                        )}
                      </td>
```

Update empty-state colspan from `5` to `6`:

```tsx
                      <td colSpan={6} className="px-2 py-6 text-center text-muted">
```

- [ ] **Step 3: Typecheck and full Vitest**

```bash
npx tsc --noEmit
npm test
```

Expected: both PASS.

- [ ] **Step 4: Commit**

```bash
git add src/tools/ToolsApp.tsx
git commit -m "feat(tools): show Avg combat→payout in hourly Results"
```

---

## Spec coverage

| Spec requirement | Task |
|------------------|------|
| Column right of Avg site | 2 |
| Label `Avg combat→payout` | 2 |
| Mean non-null combat in UTC hour | 1 |
| `—` when none / no enrichment | 1–2 (`null` → `formatDuration`) |
| UI-only / no HourlyBucket change | 1–2 |
| Empty colspan | 2 |
| Vitest on helper | 1 |

## Placeholder scan

None.

## Type consistency

- `avgCombatToPayoutForHour(hourStartIso, sites) → number | null` Task 1 → Task 2
- Sites field: `combat_to_payout_seconds: number | null` (matches `EnrichmentSite`)
