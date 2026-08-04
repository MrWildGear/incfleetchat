import { utcHourFloorMs } from "./utcHour";

export type MissileStatInput = {
  reload_cycles: number;
  hits: number;
  missiles_per_cycle: number;
  launchers: number;
};

export type MissileRates = {
  reload_cycles: number;
  expended: number;
  hits: number;
  dead_volleys: number;
  dead_missiles: number;
  hit_pct: number | null;
  miss_pct: number | null;
};

export type TimedMissileSite = {
  occurred_at: string;
  missiles: readonly MissileStatInput[];
};

function ammoPerLauncher(m: MissileStatInput): number {
  if (m.launchers === 0) return 0;
  return Math.floor(m.missiles_per_cycle / m.launchers);
}

function ratesFromTotals(
  reload_cycles: number,
  expended: number,
  hits: number,
  dead_volleys: number,
  dead_missiles: number,
): MissileRates {
  return {
    reload_cycles,
    expended,
    hits,
    dead_volleys,
    dead_missiles,
    hit_pct: expended === 0 ? null : hits / expended,
    miss_pct: expended === 0 ? null : dead_volleys / expended,
  };
}

/** Per-MissileStat display derivation. Ignores stored dead. */
export function of(m: MissileStatInput): MissileRates {
  const expended = m.reload_cycles * ammoPerLauncher(m);
  const dead_volleys = Math.max(0, expended - m.hits);
  const dead_missiles = dead_volleys * m.launchers;
  return ratesFromTotals(
    m.reload_cycles,
    expended,
    m.hits,
    dead_volleys,
    dead_missiles,
  );
}

/**
 * Fleet / Site aggregate: sum row-derived counts, then Hit % / Miss %.
 * Empty → zeros + null rates.
 */
export function sum(rows: readonly MissileStatInput[]): MissileRates {
  let reload_cycles = 0;
  let expended = 0;
  let hits = 0;
  let dead_volleys = 0;
  let dead_missiles = 0;
  for (const m of rows) {
    const r = of(m);
    reload_cycles += r.reload_cycles;
    expended += r.expended;
    hits += r.hits;
    dead_volleys += r.dead_volleys;
    dead_missiles += r.dead_missiles;
  }
  return ratesFromTotals(
    reload_cycles,
    expended,
    hits,
    dead_volleys,
    dead_missiles,
  );
}

/** Activity from recomputed fields only (never stored dead). */
export function isActive(m: MissileStatInput): boolean {
  const r = of(m);
  return m.reload_cycles !== 0 || m.hits !== 0 || r.dead_missiles !== 0;
}

/** Filter preserving full row shape; order preserved. */
export function activeOnly<T extends MissileStatInput>(
  rows: readonly T[],
): T[] {
  return rows.filter(isActive);
}

/** Missile rates for enrichment Sites in the UTC hour of hourStartIso. */
export function forHour(
  hourStartIso: string,
  sites: readonly TimedMissileSite[],
): MissileRates {
  const hourMs = utcHourFloorMs(hourStartIso);
  const rows: MissileStatInput[] = [];
  for (const s of sites) {
    if (utcHourFloorMs(s.occurred_at) !== hourMs) continue;
    for (const m of s.missiles) rows.push(m);
  }
  return sum(rows);
}
