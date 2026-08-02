import { describe, expect, it } from "vitest";
import { computeAmmoLoad } from "./ammo";

describe("computeAmmoLoad", () => {
  it("matches sheet example: 1e6 stock / 14 ships", () => {
    const r = computeAmmoLoad({
      ammoStock: 1_000_000,
      launchers: 6,
      ammoPerLauncher: 26,
      shipCount: 14,
      reloadPerSite: 2.2,
    });
    expect(r.missilesPerCycle).toBe(156);
    expect(r.loadIntoShip).toBe(71_429);
    expect(Math.round(r.sites)).toBe(208);
  });
});
