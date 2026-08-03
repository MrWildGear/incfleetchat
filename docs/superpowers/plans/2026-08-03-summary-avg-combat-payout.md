# Summary Avg Combat→payout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show **Avg combat→payout** in the Tools Summary panel (always when session is shown), while keeping the existing Gamelog enrichment row.

**Architecture:** UI-only. Read `focus?.enrichment?.totals.avg_combat_to_payout_seconds` in Summary; `formatDuration` already maps null/undefined to `—`. No backend or type changes.

**Tech Stack:** React Tools window (`ToolsApp.tsx`), Vitest / `tsc` for smoke.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-08-03-summary-avg-combat-payout-design.md`
- Do **not** remove Approach
- Do **not** change enrichment math or JSON
- Do **not** lift avg into wallet `session`
- Keep enrichment **Avg combat→payout** row unchanged
- Label exactly: `Avg combat→payout`
- Place Summary row immediately after **Avg site time**
- Conventional Commits

## File map

| File | Responsibility |
|------|----------------|
| `src/tools/ToolsApp.tsx` | Add Summary `Row` for avg combat→payout |

---

### Task 1: Summary Avg combat→payout row

**Files:**
- Modify: `src/tools/ToolsApp.tsx` (Summary aside, after Avg site time ~lines 737–740)

**Interfaces:**
- Consumes: `focus?.enrichment?.totals.avg_combat_to_payout_seconds` (`number | null | undefined`), local `formatDuration`
- Produces: Summary row always rendered inside the `session ? (…)` branch

- [ ] **Step 1: Confirm enrichment row still present**

In `src/tools/ToolsApp.tsx`, locate the Gamelog enrichment block and verify it already contains:

```tsx
                  <Row
                    label="Avg combat→payout"
                    value={formatDuration(focus.enrichment.totals.avg_combat_to_payout_seconds)}
                  />
```

Do not edit this block.

- [ ] **Step 2: Add Summary row after Avg site time**

Find:

```tsx
                  <Row
                    label="Avg site time"
                    value={formatDuration(session.avg_site_seconds)}
                  />
                  <Row
                    label="Liquid ISK/hr"
                    value={formatIskMoney(session.liquid_isk_per_hour)}
                  />
```

Replace with:

```tsx
                  <Row
                    label="Avg site time"
                    value={formatDuration(session.avg_site_seconds)}
                  />
                  <Row
                    label="Avg combat→payout"
                    value={formatDuration(
                      focus?.enrichment?.totals.avg_combat_to_payout_seconds,
                    )}
                  />
                  <Row
                    label="Liquid ISK/hr"
                    value={formatIskMoney(session.liquid_isk_per_hour)}
                  />
```

- [ ] **Step 3: Typecheck and unit tests**

```bash
npx tsc --noEmit
npm test
```

Expected: both PASS (no new test file required; this is a display-only wire-up).

- [ ] **Step 4: Manual checklist (Tools window)**

1. Open Tools with a focused run that has enrichment → Summary shows **Avg combat→payout** matching the enrichment block.
2. Open a scope with session but no enrichment → Summary shows `—` for that row; Approach rows (if any) unchanged.

- [ ] **Step 5: Commit**

```bash
git add src/tools/ToolsApp.tsx
git commit -m "feat(tools): show Avg combat→payout in Summary"
```

---

## Spec coverage

| Spec requirement | Task |
|------------------|------|
| Summary always shows Avg combat→payout when session present | 1 |
| Value from enrichment totals; else `—` | 1 |
| Keep enrichment row | 1 (Step 1) |
| After Avg site time | 1 |
| No Approach removal / no backend change | 1 (out of scope) |

## Placeholder scan

None.

## Type consistency

- Field: `avg_combat_to_payout_seconds` on enrichment totals (existing TS/Rust).
