import { describe, expect, it } from "vitest";
import {
  deadMissiles,
  deadVolleys,
  expended,
  formatPercent,
  hitRate,
  missRate,
  sumMissileStats,
} from "./missileDerived";

describe("missileDerived", () => {
  it("expended is reload_cycles × ammo_per_launcher (missiles_per_cycle / launchers)", () => {
    // 156 / 6 = 26 ammo per launcher
    expect(
      expended({ reload_cycles: 2, missiles_per_cycle: 156, launchers: 6 }),
    ).toBe(52);
  });

  it("deadVolleys is expended minus hits", () => {
    // 2×26 = 52 − 35 hits = 17
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
    ).toBe(0); // saturates at 0 when hits >= expended
  });

  it("deadMissiles is dead_volleys × launchers", () => {
    expect(
      deadMissiles({
        reload_cycles: 2,
        missiles_per_cycle: 156,
        launchers: 6,
        hits: 35,
      }),
    ).toBe(17 * 6);
    expect(
      deadMissiles({
        reload_cycles: 2,
        missiles_per_cycle: 156,
        launchers: 6,
        hits: 100,
      }),
    ).toBe(0);
  });

  it("hitRate is hits/expended; missRate is dead_volleys/expended; null when expended is 0", () => {
    expect(
      hitRate({
        hits: 35,
        reload_cycles: 2,
        missiles_per_cycle: 156,
        launchers: 6,
      }),
    ).toBeCloseTo(35 / 52);
    expect(
      missRate({
        hits: 35,
        reload_cycles: 2,
        missiles_per_cycle: 156,
        launchers: 6,
      }),
    ).toBeCloseTo(17 / 52);
    expect(
      hitRate({
        hits: 0,
        reload_cycles: 0,
        missiles_per_cycle: 156,
        launchers: 6,
      }),
    ).toBeNull();
    expect(
      missRate({
        hits: 0,
        reload_cycles: 0,
        missiles_per_cycle: 156,
        launchers: 6,
      }),
    ).toBeNull();
  });

  it("formatPercent shows one decimal or em dash", () => {
    expect(formatPercent(null)).toBe("—");
    expect(formatPercent(0.1234)).toBe("12.3%");
    expect(formatPercent(1)).toBe("100.0%");
  });

  it("sumMissileStats sums bases, derived dead missiles, and dead volleys", () => {
    const sum = sumMissileStats([
      {
        listener: "A",
        reload_cycles: 2,
        hits: 35,
        missiles_per_cycle: 156,
        launchers: 6,
        dead: 0, // stored value ignored for sum.dead
      },
      {
        listener: "B",
        reload_cycles: 1,
        hits: 10,
        missiles_per_cycle: 100,
        launchers: 5,
        dead: 0,
      },
    ]);
    expect(sum.reload_cycles).toBe(3);
    expect(sum.hits).toBe(45);
    // A: 17×6=102; B: 10×5=50
    expect(sum.dead).toBe(102 + 50);
    // A: 2×26=52; B: 1×20=20
    expect(sum.expended).toBe(52 + 20);
    // A: 52−35=17; B: 20−10=10
    expect(sum.dead_volleys).toBe(17 + 10);
  });
});
