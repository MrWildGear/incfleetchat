import { describe, expect, it } from "vitest";
import { createRunDesk, type DeskInvoke } from "./runDesk";
import type { EditionFocus, RunSettings } from "./runDeskTypes";

const defaultSettings: RunSettings = {
  space: "low_null",
  fleet_size: 15,
  expected_isk: 15_000_000,
  lp_per_char: 2_000,
  isk_per_lp: 1400,
  break_threshold_minutes: 25,
  run_start: null,
};

function focus(partial: Partial<EditionFocus> = {}): EditionFocus {
  return {
    trays: { manifest: "empty", wallet_batches: 0, pending_sites: 0 },
    spawn: null,
    catalog: { spawns: [], runs: [] },
    scope: { kind: "overall" },
    report: null,
    diagnostics: [],
    session_settings: defaultSettings,
    staging_spawn: null,
    sealed_run_id: null,
    enrichment: null,
    ...partial,
  };
}

function recordingInvoke(handlers: Record<string, (args?: unknown) => unknown>) {
  const calls: { cmd: string; args?: unknown }[] = [];
  const invoke: DeskInvoke = async (cmd, args) => {
    calls.push({ cmd, args });
    const h = handlers[cmd];
    if (!h) throw new Error(`unexpected invoke: ${cmd}`);
    return h(args) as never;
  };
  return { invoke, calls };
}

const prelude = {
  gamelogsDir: " C:\\logs ",
  fcCharacter: " FC ",
  ammoLaunchers: 6,
  ammoPerLauncher: 26,
};

describe("createRunDesk.open", () => {
  it("returns EditionFocus from run_desk_open", async () => {
    const snap = focus({ sealed_run_id: "r1" });
    const { invoke, calls } = recordingInvoke({
      run_desk_open: () => snap,
    });
    const desk = createRunDesk(invoke);
    await expect(desk.open()).resolves.toEqual(snap);
    expect(calls).toEqual([{ cmd: "run_desk_open", args: undefined }]);
  });
});

describe("createRunDesk.analyze", () => {
  it("persists gamelogs/FC and ammo, amends session settings, then analyzes", async () => {
    const after = focus({ sealed_run_id: "analyzed" });
    const { invoke, calls } = recordingInvoke({
      set_settings: () => undefined,
      run_desk_amend: () => focus(),
      run_desk_analyze: () => after,
    });
    const desk = createRunDesk(invoke);
    await expect(desk.analyze(defaultSettings, prelude)).resolves.toEqual(
      after,
    );
    expect(calls.map((c) => c.cmd)).toEqual([
      "set_settings",
      "set_settings",
      "run_desk_amend",
      "run_desk_analyze",
    ]);
    expect(calls[0]?.args).toEqual({
      patch: { gamelogs_dir: "C:\\logs", fc_character: "FC" },
    });
    expect(calls[1]?.args).toEqual({
      patch: { ammo_launchers: 6, ammo_per_launcher: 26 },
    });
    expect(calls[2]?.args).toEqual({
      op: { op: "set_session_settings", settings: defaultSettings },
    });
  });
});

describe("createRunDesk.reenrich", () => {
  it("persists prelude then reenriches with runId", async () => {
    const after = focus({ sealed_run_id: "r9" });
    const { invoke, calls } = recordingInvoke({
      set_settings: () => undefined,
      run_desk_reenrich: () => after,
    });
    const desk = createRunDesk(invoke);
    await expect(desk.reenrich("r9", prelude)).resolves.toEqual(after);
    expect(calls.map((c) => c.cmd)).toEqual([
      "set_settings",
      "set_settings",
      "run_desk_reenrich",
    ]);
    expect(calls[2]?.args).toEqual({ runId: "r9" });
  });
});

describe("createRunDesk.importWallet", () => {
  it("clears tray before paste when replace is true", async () => {
    const after = focus();
    const { invoke, calls } = recordingInvoke({
      run_desk_amend: () => focus(),
      run_desk_paste: () => after,
    });
    const desk = createRunDesk(invoke);
    await expect(desk.importWallet("WALLET", true)).resolves.toEqual(after);
    expect(calls).toEqual([
      { cmd: "run_desk_amend", args: { op: { op: "clear_wallet_tray" } } },
      {
        cmd: "run_desk_paste",
        args: { tray: "wallet", text: "WALLET" },
      },
    ]);
  });

  it("pastes only when replace is false", async () => {
    const after = focus();
    const { invoke, calls } = recordingInvoke({
      run_desk_paste: () => after,
    });
    const desk = createRunDesk(invoke);
    await expect(desk.importWallet("MORE", false)).resolves.toEqual(after);
    expect(calls).toEqual([
      {
        cmd: "run_desk_paste",
        args: { tray: "wallet", text: "MORE" },
      },
    ]);
  });
});

describe("createRunDesk.setSpaceAndFleet", () => {
  it("looks up payout then amends space, fleet, isk, and lp", async () => {
    const after = focus();
    const { invoke, calls } = recordingInvoke({
      lookup_vanguard_payout: () => ({ isk: 10_395_000, lp_per_char: 1_400 }),
      run_desk_amend: () => after,
    });
    const desk = createRunDesk(invoke);
    await expect(
      desk.setSpaceAndFleet(defaultSettings, "highsec", 10),
    ).resolves.toEqual(after);
    expect(calls[0]).toEqual({
      cmd: "lookup_vanguard_payout",
      args: { space: "highsec", fleetSize: 10 },
    });
    expect(calls[1]?.args).toEqual({
      op: {
        op: "set_session_settings",
        settings: {
          ...defaultSettings,
          space: "highsec",
          fleet_size: 10,
          expected_isk: 10_395_000,
          lp_per_char: 1_400,
        },
      },
    });
  });
});
