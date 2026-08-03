import { describe, expect, it } from "vitest";
import { hasMissileActivity } from "./missileActivity";

describe("hasMissileActivity", () => {
  it("hides all-zero rows", () => {
    expect(
      hasMissileActivity({ reload_cycles: 0, hits: 0, dead: 0 }),
    ).toBe(false);
  });
  it("keeps any non-zero reload/hits/dead", () => {
    expect(
      hasMissileActivity({ reload_cycles: 1, hits: 0, dead: 0 }),
    ).toBe(true);
    expect(
      hasMissileActivity({ reload_cycles: 0, hits: 1, dead: 0 }),
    ).toBe(true);
    expect(
      hasMissileActivity({ reload_cycles: 0, hits: 0, dead: 1 }),
    ).toBe(true);
  });
});
