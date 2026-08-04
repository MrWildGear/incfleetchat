import { describe, expect, it } from "vitest";
import type {
  AnalyticsReport,
  EnrichmentSite,
  EnrichmentSnapshot,
  HourlyBucket,
  SiteDetail,
} from "./analyticsTypes";
import {
  joinAnalytics,
  joinHours,
  joinSites,
  utcHourFloorMs,
} from "./joinedResults";

function site(partial: Partial<SiteDetail> & Pick<SiteDetail, "occurred_at">): SiteDetail {
  return {
    amount_isk: 0,
    fleet_isk: 0,
    fleet_lp: 0,
    gap_seconds: null,
    duration_seconds: 60,
    is_break: false,
    counts_toward_avg: true,
    ...partial,
  };
}

function enrichSite(
  partial: Partial<EnrichmentSite> & Pick<EnrichmentSite, "occurred_at">,
): EnrichmentSite {
  return {
    approach_seconds: null,
    combat_to_payout_seconds: null,
    is_break: false,
    source: "fleet",
    missiles: [],
    ...partial,
  };
}

function hour(partial: Partial<HourlyBucket> & Pick<HourlyBucket, "hour_start">): HourlyBucket {
  return {
    total_isk: 0,
    total_lp: 0,
    sites: 0,
    avg_site_seconds: null,
    ...partial,
  };
}

function emptyTotals(): EnrichmentSnapshot["totals"] {
  return {
    approach_seconds: null,
    combat_to_payout_seconds: null,
    avg_combat_to_payout_seconds: null,
    fleet_dead: 0,
  };
}

describe("utcHourFloorMs", () => {
  it("floors to the start of the UTC hour", () => {
    expect(utcHourFloorMs("2026-08-03T14:37:22.000Z")).toBe(
      Date.parse("2026-08-03T14:00:00.000Z"),
    );
  });
});

describe("joinSites", () => {
  it("left-joins enrichment on payout time; misses stay null", () => {
    const wallet = [
      site({ occurred_at: "2026-08-03T14:05:00.000Z", duration_seconds: 90 }),
      site({ occurred_at: "2026-08-03T14:40:00.000Z" }),
    ];
    const enrichment = [
      enrichSite({
        occurred_at: "2026-08-03T14:05:00.000Z",
        combat_to_payout_seconds: 120,
        missiles: [
          {
            listener: "A",
            reload_cycles: 2,
            hits: 30,
            missiles_per_cycle: 156,
            launchers: 6,
            dead: 0,
          },
        ],
      }),
      enrichSite({
        occurred_at: "2026-08-03T15:00:00.000Z",
        combat_to_payout_seconds: 999,
      }),
    ];

    const rows = joinSites(wallet, enrichment);
    expect(rows).toHaveLength(2);
    expect(rows[0].site.duration_seconds).toBe(90);
    expect(rows[0].enrichment?.combat_to_payout_seconds).toBe(120);
    expect(rows[0].hitPct).toBe(30 / 52);
    expect(rows[0].missPct).toBe(22 / 52);
    expect(rows[1].enrichment).toBeNull();
    expect(rows[1].hitPct).toBeNull();
    expect(rows[1].missPct).toBeNull();
  });

  it("returns empty when wallet sites are empty", () => {
    expect(
      joinSites([], [enrichSite({ occurred_at: "2026-08-03T14:05:00.000Z" })]),
    ).toEqual([]);
  });
});

describe("joinHours", () => {
  const hourStart = "2026-08-03T14:00:00.000Z";

  it("returns null combat and rates when no enrichment sites match the hour", () => {
    const rows = joinHours(
      [hour({ hour_start: hourStart })],
      [
        enrichSite({
          occurred_at: "2026-08-03T15:10:00.000Z",
          combat_to_payout_seconds: 100,
        }),
      ],
    );
    expect(rows).toEqual([
      {
        bucket: hour({ hour_start: hourStart }),
        avgCombatToPayout: null,
        hitPct: null,
        missPct: null,
      },
    ]);
  });

  it("returns null combat when matching sites have only null combat", () => {
    const rows = joinHours(
      [hour({ hour_start: hourStart })],
      [
        enrichSite({
          occurred_at: "2026-08-03T14:20:00.000Z",
          combat_to_payout_seconds: null,
        }),
      ],
    );
    expect(rows[0].avgCombatToPayout).toBeNull();
  });

  it("averages non-null combat for enrichment sites in the UTC hour", () => {
    const rows = joinHours(
      [hour({ hour_start: hourStart })],
      [
        enrichSite({
          occurred_at: "2026-08-03T14:05:00.000Z",
          combat_to_payout_seconds: 100,
        }),
        enrichSite({
          occurred_at: "2026-08-03T14:55:00.000Z",
          combat_to_payout_seconds: 200,
        }),
        enrichSite({
          occurred_at: "2026-08-03T14:30:00.000Z",
          combat_to_payout_seconds: null,
        }),
        enrichSite({
          occurred_at: "2026-08-03T15:01:00.000Z",
          combat_to_payout_seconds: 999,
        }),
      ],
    );
    expect(rows[0].avgCombatToPayout).toBe(150);
  });

  it("sums missiles across sites in the hour then derives Hit/Miss %", () => {
    const rows = joinHours(
      [hour({ hour_start: hourStart })],
      [
        enrichSite({
          occurred_at: "2026-08-03T14:05:00.000Z",
          missiles: [
            {
              listener: "A",
              reload_cycles: 2,
              hits: 30,
              missiles_per_cycle: 156,
              launchers: 6,
              dead: 0,
            },
          ],
        }),
        enrichSite({
          occurred_at: "2026-08-03T14:40:00.000Z",
          missiles: [
            {
              listener: "B",
              reload_cycles: 1,
              hits: 10,
              missiles_per_cycle: 156,
              launchers: 6,
              dead: 0,
            },
          ],
        }),
        enrichSite({
          occurred_at: "2026-08-03T15:01:00.000Z",
          missiles: [
            {
              listener: "C",
              reload_cycles: 9,
              hits: 1,
              missiles_per_cycle: 156,
              launchers: 6,
              dead: 0,
            },
          ],
        }),
      ],
    );
    expect(rows[0].hitPct).toBe(40 / 78);
    expect(rows[0].missPct).toBe(38 / 78);
  });
});

describe("joinAnalytics", () => {
  it("returns empty sites/hours and null summary when report is null", () => {
    expect(joinAnalytics(null, null)).toEqual({
      sites: [],
      hours: [],
      avgCombatToPayout: null,
    });
  });

  it("passes through enrichment summary avg when present", () => {
    const report: AnalyticsReport = {
      session: {
        sites_ran: 0,
        active_site_seconds: 0,
        wallet_elapsed_seconds: 0,
        avg_site_seconds: null,
        character_liquid_isk: 0,
        fleet_liquid_isk: 0,
        net_lp: 0,
        lp_per_character_total: null,
        lp_value: 0,
        net_value: 0,
        liquid_isk_per_hour: 0,
        lp_value_per_hour: 0,
        net_per_hour: 0,
      },
      hourly: [],
      sites: [],
    };
    const enrichment: EnrichmentSnapshot = {
      resolved_fc: null,
      listeners: [],
      diagnostics: [],
      sites: [],
      missiles: [],
      totals: {
        ...emptyTotals(),
        avg_combat_to_payout_seconds: 42,
      },
    };
    expect(joinAnalytics(report, enrichment).avgCombatToPayout).toBe(42);
  });

  it("joins sites and hours together", () => {
    const report: AnalyticsReport = {
      session: {
        sites_ran: 1,
        active_site_seconds: 60,
        wallet_elapsed_seconds: 60,
        avg_site_seconds: 60,
        character_liquid_isk: 0,
        fleet_liquid_isk: 0,
        net_lp: 0,
        lp_per_character_total: null,
        lp_value: 0,
        net_value: 0,
        liquid_isk_per_hour: 0,
        lp_value_per_hour: 0,
        net_per_hour: 0,
      },
      hourly: [hour({ hour_start: "2026-08-03T14:00:00.000Z", sites: 1 })],
      sites: [site({ occurred_at: "2026-08-03T14:05:00.000Z" })],
    };
    const enrichment: EnrichmentSnapshot = {
      resolved_fc: null,
      listeners: [],
      diagnostics: [],
      sites: [
        enrichSite({
          occurred_at: "2026-08-03T14:05:00.000Z",
          combat_to_payout_seconds: 80,
        }),
      ],
      missiles: [],
      totals: {
        ...emptyTotals(),
        avg_combat_to_payout_seconds: 80,
      },
    };
    const joined = joinAnalytics(report, enrichment);
    expect(joined.sites).toHaveLength(1);
    expect(joined.sites[0].enrichment?.combat_to_payout_seconds).toBe(80);
    expect(joined.hours).toHaveLength(1);
    expect(joined.hours[0].avgCombatToPayout).toBe(80);
    expect(joined.avgCombatToPayout).toBe(80);
  });
});
