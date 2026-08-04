import { describe, expect, it } from "vitest";
import {
  avgCombatToPayoutForHour,
  missileRatesForHour,
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

describe("missileRatesForHour", () => {
  const hour = "2026-08-03T14:00:00.000Z";

  it("returns null rates when no missiles in the hour", () => {
    expect(
      missileRatesForHour(hour, [
        { occurred_at: "2026-08-03T15:10:00.000Z", missiles: [] },
      ]),
    ).toEqual({ hitPct: null, missPct: null });
  });

  it("sums missiles across sites in the hour then derives Hit/Miss %", () => {
    // expended = reload × ammo_per_launcher
    // Site A: expended 52, hits 30, dead_volleys 22
    // Site B: expended 26, hits 10, dead_volleys 16
    // Fleet: expended 78, hits 40, dead_volleys 38
    // hit% = hits/expended; miss% = dead_volleys/expended
    expect(
      missileRatesForHour(hour, [
        {
          occurred_at: "2026-08-03T14:05:00.000Z",
          missiles: [
            {
              reload_cycles: 2,
              hits: 30,
              missiles_per_cycle: 156,
              launchers: 6,
              dead: 0,
            },
          ],
        },
        {
          occurred_at: "2026-08-03T14:40:00.000Z",
          missiles: [
            {
              reload_cycles: 1,
              hits: 10,
              missiles_per_cycle: 156,
              launchers: 6,
              dead: 0,
            },
          ],
        },
        {
          occurred_at: "2026-08-03T15:01:00.000Z",
          missiles: [
            {
              reload_cycles: 9,
              hits: 1,
              missiles_per_cycle: 156,
              launchers: 6,
              dead: 0,
            },
          ],
        },
      ]),
    ).toEqual({
      hitPct: 40 / 78,
      missPct: 38 / 78,
    });
  });
});
