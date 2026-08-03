import { describe, expect, it } from "vitest";
import {
  formatAppVersionLabel,
  manualCheckOutcome,
  progressStageFromUpdaterEvent,
} from "./updateUx";

describe("formatAppVersionLabel", () => {
  it("prefixes v when missing", () => {
    expect(formatAppVersionLabel("0.1.0")).toBe("v0.1.0");
  });
  it("does not double-prefix", () => {
    expect(formatAppVersionLabel("v0.1.0")).toBe("v0.1.0");
  });
});

describe("manualCheckOutcome", () => {
  it("maps null update to upToDate", () => {
    expect(manualCheckOutcome(null)).toEqual({ kind: "upToDate" });
  });
  it("maps update object to available", () => {
    expect(manualCheckOutcome({ version: "0.2.0" })).toEqual({
      kind: "available",
      version: "0.2.0",
    });
  });
  it("maps thrown error to error message", () => {
    expect(manualCheckOutcome(null, new Error("network down"))).toEqual({
      kind: "error",
      message: "network down",
    });
  });
});

describe("progressStageFromUpdaterEvent", () => {
  it("treats Started and Progress as downloading", () => {
    expect(progressStageFromUpdaterEvent("Started")).toBe("downloading");
    expect(progressStageFromUpdaterEvent("Progress")).toBe("downloading");
  });
  it("treats Finished as installing", () => {
    expect(progressStageFromUpdaterEvent("Finished")).toBe("installing");
  });
});
