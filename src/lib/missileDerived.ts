import type { MissileStat } from "./analyticsTypes";

/** Ammo per launcher at enrich time (= missiles_per_cycle / launchers). */
export function ammoPerLauncher(
  m: Pick<MissileStat, "missiles_per_cycle" | "launchers">,
): number {
  if (m.launchers === 0) return 0;
  return Math.floor(m.missiles_per_cycle / m.launchers);
}

/** expended = reload_cycles × ammo_per_launcher */
export function expended(
  m: Pick<MissileStat, "reload_cycles" | "missiles_per_cycle" | "launchers">,
): number {
  return m.reload_cycles * ammoPerLauncher(m);
}

/**
 * Unused volleys from reloads minus combat hit lines.
 * dead_volleys = max(0, expended − hits).
 */
export function deadVolleys(
  m: Pick<
    MissileStat,
    "reload_cycles" | "missiles_per_cycle" | "launchers" | "hits"
  >,
): number {
  return Math.max(0, expended(m) - m.hits);
}

/** dead_missiles = dead_volleys × launchers (missiles per volley). */
export function deadMissiles(
  m: Pick<
    MissileStat,
    "reload_cycles" | "missiles_per_cycle" | "launchers" | "hits"
  >,
): number {
  return deadVolleys(m) * m.launchers;
}

/** Hit % = hits / expended. */
export function hitRate(
  m: Pick<
    MissileStat,
    "hits" | "reload_cycles" | "missiles_per_cycle" | "launchers"
  >,
): number | null {
  const e = expended(m);
  if (e === 0) return null;
  return m.hits / e;
}

/** Miss % = dead_volleys / expended. */
export function missRate(
  m: Pick<
    MissileStat,
    "hits" | "reload_cycles" | "missiles_per_cycle" | "launchers"
  >,
): number | null {
  const e = expended(m);
  if (e === 0) return null;
  return deadVolleys(m) / e;
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
  dead_volleys: number;
};

export function sumMissileStats(rows: MissileStat[]): MissileSum {
  return rows.reduce<MissileSum>(
    (acc, m) => ({
      reload_cycles: acc.reload_cycles + m.reload_cycles,
      hits: acc.hits + m.hits,
      dead: acc.dead + deadMissiles(m),
      expended: acc.expended + expended(m),
      dead_volleys: acc.dead_volleys + deadVolleys(m),
    }),
    { reload_cycles: 0, hits: 0, dead: 0, expended: 0, dead_volleys: 0 },
  );
}
