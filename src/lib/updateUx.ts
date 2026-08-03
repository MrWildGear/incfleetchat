export function formatAppVersionLabel(version: string): string {
  return version.startsWith("v") ? version : `v${version}`;
}

export type ManualCheckOutcome =
  | { kind: "upToDate" }
  | { kind: "available"; version: string }
  | { kind: "error"; message: string };

export function manualCheckOutcome(
  update: { version: string } | null,
  err?: unknown,
): ManualCheckOutcome {
  if (err !== undefined) {
    const message =
      err instanceof Error
        ? err.message
        : err != null
          ? String(err)
          : "Update check failed";
    return { kind: "error", message };
  }
  if (update === null) {
    return { kind: "upToDate" };
  }
  return { kind: "available", version: update.version };
}

export type ProgressStage = "downloading" | "installing";

export function progressStageFromUpdaterEvent(
  event: "Started" | "Progress" | "Finished",
): ProgressStage {
  return event === "Finished" ? "installing" : "downloading";
}
