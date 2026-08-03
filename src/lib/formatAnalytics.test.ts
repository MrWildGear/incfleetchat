import { describe, expect, it } from "vitest";
import { formatCount, formatIskMoney, formatLp } from "./formatAnalytics";

describe("formatAnalytics", () => {
  it("formats ISK with dollar and commas", () => {
    expect(formatIskMoney(1234)).toBe("$1,234");
  });

  it("formats counts with commas only", () => {
    expect(formatCount(1234)).toBe("1,234");
  });

  it("formats LP without dollar", () => {
    expect(formatLp(1234)).toBe("1,234");
  });

  it("rounds fractional ISK", () => {
    expect(formatIskMoney(1234.6)).toBe("$1,235");
  });

  it("formats negative values", () => {
    expect(formatIskMoney(-1234)).toBe("-$1,234");
  });

  it("formats zero", () => {
    expect(formatCount(0)).toBe("0");
  });
});
