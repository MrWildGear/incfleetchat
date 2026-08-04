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

export type HourMissileRates = {
  hitPct: number | null;
  missPct: number | null;
};

/** Fleet Hit % / Miss % for enrichment sites in the UTC hour (sum then derive). */
export function missileRatesForHour(
  hourStartIso: string,
  sites: readonly {
    occurred_at: string;
    missiles: readonly {
      reload_cycles: number;
      hits: number;
      missiles_per_cycle: number;
      dead: number;
    }[];
  }[],
): HourMissileRates {
  const hourMs = utcHourFloorMs(hourStartIso);
  let expended = 0;
  let hits = 0;
  let dead = 0;
  for (const s of sites) {
    if (utcHourFloorMs(s.occurred_at) !== hourMs) continue;
    for (const m of s.missiles) {
      expended += m.reload_cycles * m.missiles_per_cycle;
      hits += m.hits;
      dead += m.dead;
    }
  }
  if (expended === 0) return { hitPct: null, missPct: null };
  return { hitPct: hits / expended, missPct: dead / expended };
}
