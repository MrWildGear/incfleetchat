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
