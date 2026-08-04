import type {
  AnalyticsReport,
  EnrichmentSite,
  EnrichmentSnapshot,
  HourlyBucket,
  SiteDetail,
} from "./analyticsInternals";
import { forHour, sum } from "./missileRates";
import { utcHourFloorMs } from "./utcHour";

export type JoinedSiteRow = {
  site: SiteDetail;
  enrichment: EnrichmentSite | null;
  hitPct: number | null;
  missPct: number | null;
};

export type JoinedHourRow = {
  bucket: HourlyBucket;
  avgCombatToPayout: number | null;
  hitPct: number | null;
  missPct: number | null;
};

export type JoinedResults = {
  sites: JoinedSiteRow[];
  hours: JoinedHourRow[];
  avgCombatToPayout: number | null;
};

export { utcHourFloorMs };

function enrichmentInHour(
  hourStartIso: string,
  enrichmentSites: readonly EnrichmentSite[],
): EnrichmentSite[] {
  const hourMs = utcHourFloorMs(hourStartIso);
  return enrichmentSites.filter(
    (s) => utcHourFloorMs(s.occurred_at) === hourMs,
  );
}

function avgCombatToPayout(
  sitesInHour: readonly EnrichmentSite[],
): number | null {
  const values: number[] = [];
  for (const s of sitesInHour) {
    if (s.combat_to_payout_seconds == null) continue;
    values.push(s.combat_to_payout_seconds);
  }
  if (values.length === 0) return null;
  return values.reduce((a, b) => a + b, 0) / values.length;
}

/** Wallet-driven left join on payout time. Enrichment-only sites are dropped. */
export function joinSites(
  walletSites: readonly SiteDetail[],
  enrichmentSites: readonly EnrichmentSite[],
): JoinedSiteRow[] {
  const byPayout = new Map<string, EnrichmentSite>();
  for (const e of enrichmentSites) byPayout.set(e.occurred_at, e);

  return walletSites.map((site) => {
    const enrichment = byPayout.get(site.occurred_at) ?? null;
    const rates = sum(enrichment?.missiles ?? []);
    return {
      site,
      enrichment,
      hitPct: rates.hit_pct,
      missPct: rates.miss_pct,
    };
  });
}

/** Wallet hourly buckets with enrichment combat avg and missile rates by UTC hour. */
export function joinHours(
  hourly: readonly HourlyBucket[],
  enrichmentSites: readonly EnrichmentSite[],
): JoinedHourRow[] {
  return hourly.map((bucket) => {
    const inHour = enrichmentInHour(bucket.hour_start, enrichmentSites);
    const rates = forHour(bucket.hour_start, enrichmentSites);
    return {
      bucket,
      avgCombatToPayout: avgCombatToPayout(inHour),
      hitPct: rates.hit_pct,
      missPct: rates.miss_pct,
    };
  });
}

/** Joined Results: wallet-driven sites/hours plus summary combat avg from Enrichment. */
export function joinAnalytics(
  report: AnalyticsReport | null,
  enrichment: EnrichmentSnapshot | null,
): JoinedResults {
  const enrichmentSites = enrichment?.sites ?? [];
  return {
    sites: joinSites(report?.sites ?? [], enrichmentSites),
    hours: joinHours(report?.hourly ?? [], enrichmentSites),
    avgCombatToPayout: enrichment?.totals.avg_combat_to_payout_seconds ?? null,
  };
}
