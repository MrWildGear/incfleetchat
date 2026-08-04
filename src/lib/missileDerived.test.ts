import { describe, expect, it } from "vitest";
import {
  deadCycles,
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

  it("deadCycles is floor(dead / missiles_per_cycle)", () => {
    expect(deadCycles({ dead: 212, missiles_per_cycle: 156 })).toBe(1);
    expect(deadCycles({ dead: 312, missiles_per_cycle: 156 })).toBe(2);
  });

  it("hitRate is dead/expended; missRate is hits/expended; null when expended is 0", () => {
    expect(hitRate({ dead: 212, reload_cycles: 2, missiles_per_cycle: 156 })).toBeCloseTo(
      212 / 312,
    );
    expect(missRate({ hits: 100, reload_cycles: 2, missiles_per_cycle: 156 })).toBeCloseTo(
      100 / 312,
    );
    expect(hitRate({ dead: 0, reload_cycles: 0, missiles_per_cycle: 156 })).toBeNull();
    expect(missRate({ hits: 0, reload_cycles: 0, missiles_per_cycle: 156 })).toBeNull();
  });

  it("formatPercent shows one decimal or em dash", () => {
    expect(formatPercent(null)).toBe("—");
    expect(formatPercent(0.1234)).toBe("12.3%");
    expect(formatPercent(1)).toBe("100.0%");
  });

  it("sumMissileStats sums bases and per-listener dead cycles", () => {
    const sum = sumMissileStats([
      {
        listener: "A",
        reload_cycles: 2,
        hits: 100,
        missiles_per_cycle: 156,
        dead: 212,
      },
      {
        listener: "B",
        reload_cycles: 1,
        hits: 50,
        missiles_per_cycle: 100,
        dead: 50,
      },
    ]);
    expect(sum.reload_cycles).toBe(3);
    expect(sum.hits).toBe(150);
    expect(sum.dead).toBe(262);
    expect(sum.expended).toBe(312 + 100);
    expect(sum.dead_cycles).toBe(1 + 0); // floor(212/156)+floor(50/100)
  });
});
