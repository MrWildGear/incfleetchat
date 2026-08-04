import { describe, expect, it, vi } from "vitest";
import {
  createUpdateSession,
  type UpdateHandle,
  type UpdaterPorts,
} from "./updateSession";

function fakeUpdate(
  version: string,
  download: UpdateHandle["downloadAndInstall"] = async (onEvent) => {
    onEvent({ event: "Started" });
    onEvent({ event: "Progress" });
    onEvent({ event: "Finished" });
  },
): UpdateHandle {
  return { version, downloadAndInstall: download };
}

function ports(overrides: Partial<UpdaterPorts> = {}): UpdaterPorts {
  return {
    getVersion: async () => "0.1.2",
    check: async () => null,
    relaunch: async () => undefined,
    ...overrides,
  };
}

describe("createUpdateSession", () => {
  it("loadVersion records app version", async () => {
    const session = createUpdateSession(ports());
    await session.loadVersion();
    expect(session.getSnapshot().appVersion).toBe("0.1.2");
  });

  it("loadVersion nulls version when getVersion fails", async () => {
    const session = createUpdateSession(
      ports({ getVersion: async () => {
        throw new Error("no version");
      }}),
    );
    await session.loadVersion();
    expect(session.getSnapshot().appVersion).toBeNull();
  });

  it("runLaunchCheck opens prompt when an update is available", async () => {
    const session = createUpdateSession(
      ports({ check: async () => fakeUpdate("0.2.0") }),
    );
    await session.runLaunchCheck();
    const snap = session.getSnapshot();
    expect(snap.promptOpen).toBe(true);
    expect(snap.pendingVersion).toBe("0.2.0");
  });

  it("runLaunchCheck swallows check errors silently", async () => {
    const session = createUpdateSession(
      ports({
        check: async () => {
          throw new Error("network");
        },
      }),
    );
    await session.runLaunchCheck();
    expect(session.getSnapshot()).toMatchObject({
      promptOpen: false,
      pendingVersion: null,
      manualResult: null,
    });
  });

  it("onLater defers the update", async () => {
    const session = createUpdateSession(
      ports({ check: async () => fakeUpdate("0.2.0") }),
    );
    await session.runLaunchCheck();
    session.onLater();
    expect(session.getSnapshot()).toMatchObject({
      promptOpen: false,
      deferredUpdate: true,
      pendingVersion: "0.2.0",
    });
  });

  it("checkManual up-to-date clears deferred state", async () => {
    const session = createUpdateSession(
      ports({
        check: vi
          .fn()
          .mockResolvedValueOnce(fakeUpdate("0.2.0"))
          .mockResolvedValueOnce(null),
      }),
    );
    await session.runLaunchCheck();
    session.onLater();
    expect(session.getSnapshot().deferredUpdate).toBe(true);

    await session.checkManual();
    expect(session.getSnapshot()).toMatchObject({
      deferredUpdate: false,
      pendingVersion: null,
      promptOpen: false,
      manualResult: { kind: "upToDate" },
      checkingManual: false,
    });
  });

  it("checkManual available clears deferred and opens prompt", async () => {
    const session = createUpdateSession(
      ports({
        check: vi
          .fn()
          .mockResolvedValueOnce(fakeUpdate("0.2.0"))
          .mockResolvedValueOnce(fakeUpdate("0.2.1")),
      }),
    );
    await session.runLaunchCheck();
    session.onLater();

    await session.checkManual();
    expect(session.getSnapshot()).toMatchObject({
      deferredUpdate: false,
      pendingVersion: "0.2.1",
      promptOpen: true,
      manualResult: { kind: "available", version: "0.2.1" },
    });
  });

  it("onUpdateNow tracks progress then relaunches", async () => {
    const relaunch = vi.fn(async () => undefined);
    const stages: string[] = [];
    const session = createUpdateSession(
      ports({
        check: async () =>
          fakeUpdate("0.2.0", async (onEvent) => {
            onEvent({ event: "Started" });
            stages.push(session.getSnapshot().progress?.stage ?? "");
            onEvent({ event: "Finished" });
            stages.push(session.getSnapshot().progress?.stage ?? "");
          }),
        relaunch,
      }),
    );
    await session.runLaunchCheck();
    await session.onUpdateNow();
    expect(stages).toEqual(["downloading", "installing"]);
    expect(relaunch).toHaveBeenCalledOnce();
    expect(session.getSnapshot().deferredUpdate).toBe(false);
  });

  it("install failure records progressError and clears progress", async () => {
    const session = createUpdateSession(
      ports({
        check: async () =>
          fakeUpdate("0.2.0", async () => {
            throw new Error("disk full");
          }),
      }),
    );
    await session.runLaunchCheck();
    await session.onUpdateNow();
    expect(session.getSnapshot()).toMatchObject({
      progress: null,
      progressError: "disk full",
      promptOpen: false,
    });
  });
});
