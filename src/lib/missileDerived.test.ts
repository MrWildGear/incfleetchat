import { describe, expect, it } from "vitest";
import {
  deadVolleys,
  expended,
  formatPercent,
  hitRate,
  missRate,
  sumMissileStats,
} from "./missileDerived";

describe("missileDerived", () => {
  it("expended is reload_cycles × missiles_per_cycle", () => {
    expect(expended({ reload_cycles: 2, missiles_per_cycle: 156 })).toBe(312);
  });

  it("deadVolleys is expended_volleys minus hits", () => {
    // 2×156/6 = 52 volleys − 35 hits = 17
    expect(
      deadVolleys({
        reload_cycles: 2,
        missiles_per_cycle: 156,
        launchers: 6,
        hits: 35,
      }),
    ).toBe(17);
    expect(
      deadVolleys({
        reload_cycles: 2,
        missiles_per_cycle: 156,
        launchers: 6,
        hits: 100,
      }),
    ).toBe(0); // saturates at 0 when hits >= expended volleys
  });

  it("hitRate is dead/expended; missRate is hits/expended; null when expended is 0", () => {
    expect(
      hitRate({ dead: 212, reload_cycles: 2, missiles_per_cycle: 156 }),
    ).toBeCloseTo(212 / 312);
    expect(
      missRate({ hits: 100, reload_cycles: 2, missiles_per_cycle: 156 }),
    ).toBeCloseTo(100 / 312);
    expect(
      hitRate({ dead: 0, reload_cycles: 0, missiles_per_cycle: 156 }),
    ).toBeNull();
    expect(
      missRate({ hits: 0, reload_cycles: 0, missiles_per_cycle: 156 }),
    ).toBeNull();
  });

  it("formatPercent shows one decimal or em dash", () => {
    expect(formatPercent(null)).toBe("—");
    expect(formatPercent(0.1234)).toBe("12.3%");
    expect(formatPercent(1)).toBe("100.0%");
  });

  it("sumMissileStats sums bases and per-listener dead volleys", () => {
    const sum = sumMissileStats([
      {
        listener: "A",
        reload_cycles: 2,
        hits: 35,
        missiles_per_cycle: 156,
        launchers: 6,
        dead: 277,
      },
      {
        listener: "B",
        reload_cycles: 1,
        hits: 10,
        missiles_per_cycle: 100,
        launchers: 5,
        dead: 90,
      },
    ]);
    expect(sum.reload_cycles).toBe(3);
    expect(sum.hits).toBe(45);
    expect(sum.dead).toBe(367);
    expect(sum.expended).toBe(312 + 100);
    // A: floor(312/6)-35 = 17; B: floor(100/5)-10 = 10
    expect(sum.dead_volleys).toBe(17 + 10);
  });
});
