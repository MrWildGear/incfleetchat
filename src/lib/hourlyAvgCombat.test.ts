import { describe, expect, it } from "vitest";
import {
  avgCombatToPayoutForHour,
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
