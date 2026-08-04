import { describe, expect, it } from "vitest";
import {
  activeOnly,
  forHour,
  isActive,
  of,
  sum,
} from "./missileRates";

describe("missileRates.of", () => {
  it("derives expended, dead volleys, dead missiles, Hit %, Miss %", () => {
    // 156/6 = 26 ammo/launcher; 2×26 = 52 expended; 52−35 = 17 dead volleys
    const r = of({
      reload_cycles: 2,
      missiles_per_cycle: 156,
      launchers: 6,
      hits: 35,
    });
    expect(r.expended).toBe(52);
    expect(r.hits).toBe(35);
    expect(r.dead_volleys).toBe(17);
    expect(r.dead_missiles).toBe(17 * 6);
    expect(r.hit_pct).toBeCloseTo(35 / 52);
    expect(r.miss_pct).toBeCloseTo(17 / 52);
  });

  it("null Hit % / Miss % when expended is 0; dead volleys saturate at 0", () => {
    expect(
      of({
        reload_cycles: 0,
        missiles_per_cycle: 156,
        launchers: 6,
        hits: 0,
      }).hit_pct,
    ).toBeNull();
    expect(
      of({
        reload_cycles: 2,
        missiles_per_cycle: 156,
        launchers: 6,
        hits: 100,
      }).dead_volleys,
    ).toBe(0);
  });
});

describe("missileRates.sum", () => {
  it("sums row-derived counts then derives Hit % / Miss %", () => {
    const r = sum([
      {
        reload_cycles: 2,
        hits: 35,
        missiles_per_cycle: 156,
        launchers: 6,
      },
      {
        reload_cycles: 1,
        hits: 10,
        missiles_per_cycle: 100,
        launchers: 5,
      },
    ]);
    // A: 52 expended, 17 volleys, 102 dead; B: 20, 10, 50
    expect(r.hits).toBe(45);
    expect(r.expended).toBe(72);
    expect(r.dead_volleys).toBe(27);
    expect(r.dead_missiles).toBe(152);
    expect(r.hit_pct).toBeCloseTo(45 / 72);
    expect(r.miss_pct).toBeCloseTo(27 / 72);
  });

  it("empty input is zeros with null rates", () => {
    const r = sum([]);
    expect(r.expended).toBe(0);
    expect(r.hit_pct).toBeNull();
    expect(r.miss_pct).toBeNull();
  });
});

describe("missileRates.isActive / activeOnly", () => {
  it("ignores stored dead; uses recomputed dead missiles", () => {
    expect(
      isActive({
        reload_cycles: 0,
        hits: 0,
        missiles_per_cycle: 156,
        launchers: 6,
      }),
    ).toBe(false);
    expect(
      isActive({
        reload_cycles: 1,
        hits: 0,
        missiles_per_cycle: 156,
        launchers: 6,
      }),
    ).toBe(true);
    // Stored dead alone would have been "active" before; recompute says inactive
    expect(
      isActive({
        reload_cycles: 0,
        hits: 0,
        missiles_per_cycle: 0,
        launchers: 0,
      }),
    ).toBe(false);
  });

  it("activeOnly keeps full rows that are active", () => {
    const rows = [
      {
        listener: "A",
        reload_cycles: 0,
        hits: 0,
        missiles_per_cycle: 156,
        launchers: 6,
        dead: 99,
      },
      {
        listener: "B",
        reload_cycles: 1,
        hits: 0,
        missiles_per_cycle: 156,
        launchers: 6,
        dead: 0,
      },
    ];
    expect(activeOnly(rows).map((r) => r.listener)).toEqual(["B"]);
  });
});

describe("missileRates.forHour", () => {
  it("aggregates missiles for sites in the UTC hour", () => {
    const hour = "2026-08-03T14:00:00.000Z";
    const r = forHour(hour, [
      {
        occurred_at: "2026-08-03T14:30:00.000Z",
        missiles: [
          {
            reload_cycles: 2,
            hits: 30,
            missiles_per_cycle: 156,
            launchers: 6,
          },
        ],
      },
      {
        occurred_at: "2026-08-03T15:01:00.000Z",
        missiles: [
          {
            reload_cycles: 9,
            hits: 9,
            missiles_per_cycle: 156,
            launchers: 6,
          },
        ],
      },
      {
        occurred_at: "2026-08-03T14:10:00.000Z",
        missiles: [
          {
            reload_cycles: 1,
            hits: 10,
            missiles_per_cycle: 156,
            launchers: 6,
          },
        ],
      },
    ]);
    // Sites in 14:00: expended 52+26=78, hits 40, dead volleys 22+16=38
    expect(r.expended).toBe(78);
    expect(r.hits).toBe(40);
    expect(r.hit_pct).toBeCloseTo(40 / 78);
    expect(r.miss_pct).toBeCloseTo(38 / 78);
  });
});
