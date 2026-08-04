const countFormatter = new Intl.NumberFormat("en-US", {
  maximumFractionDigits: 0,
});

/** Formats ISK / liquid / net-value figures as "$#,###" (rounded, comma-grouped). */
export function formatIskMoney(n: number): string {
  const rounded = Math.round(n);
  const sign = rounded < 0 ? "-" : "";
  return `${sign}$${countFormatter.format(Math.abs(rounded))}`;
}

/** Formats plain counts (missiles, dead, hits, etc.) as "#,###". */
export function formatCount(n: number): string {
  return countFormatter.format(Math.round(n));
}

/** Formats Loyalty Points as "#,###" — no currency symbol. */
export function formatLp(n: number): string {
  return countFormatter.format(Math.round(n));
}

/** Hit % / Miss % display — one decimal, or em dash when null/non-finite. */
export function formatPercent(rate: number | null): string {
  if (rate == null || !Number.isFinite(rate)) return "—";
  return `${(rate * 100).toFixed(1)}%`;
}
