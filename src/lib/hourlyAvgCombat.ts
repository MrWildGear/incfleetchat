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
