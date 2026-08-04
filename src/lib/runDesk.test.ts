import { describe, expect, it } from "vitest";
import { createRunDeskClient, type DeskInvoke } from "./runDesk";
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

const inputs = {
  gamelogsDir: " C:\\logs ",
  fcCharacter: " FC ",
  ammoLaunchers: 6,
  ammoPerLauncher: 26,
};

describe("createRunDeskClient.cmds.open", () => {
  it("returns EditionFocus from run_desk_open", async () => {
    const snap = focus({ sealed_run_id: "r1" });
    const { invoke, calls } = recordingInvoke({
      run_desk_open: () => snap,
    });
    const client = createRunDeskClient(invoke);
    await expect(client.cmds.open()).resolves.toEqual(snap);
    expect(calls).toEqual([{ cmd: "run_desk_open", args: undefined }]);
  });
});

describe("createRunDeskClient.analyze", () => {
  it("amends session settings then analyzes with Enrichment inputs", async () => {
    const after = focus({ sealed_run_id: "analyzed" });
    const { invoke, calls } = recordingInvoke({
      run_desk_amend: () => focus(),
      run_desk_analyze: () => after,
    });
    const client = createRunDeskClient(invoke);
    await expect(client.analyze(defaultSettings, inputs)).resolves.toEqual(
      after,
    );
    expect(calls.map((c) => c.cmd)).toEqual([
      "run_desk_amend",
      "run_desk_analyze",
    ]);
    expect(calls[0]?.args).toEqual({
      op: { op: "set_session_settings", settings: defaultSettings },
    });
    expect(calls[1]?.args).toEqual({ inputs });
  });
});

describe("createRunDeskClient.reenrich", () => {
  it("passes Enrichment inputs and runId on the wire", async () => {
    const after = focus({ sealed_run_id: "r9" });
    const { invoke, calls } = recordingInvoke({
      run_desk_reenrich: () => after,
    });
    const client = createRunDeskClient(invoke);
    await expect(client.reenrich("r9", inputs)).resolves.toEqual(after);
    expect(calls).toEqual([
      { cmd: "run_desk_reenrich", args: { runId: "r9", inputs } },
    ]);
  });
});

describe("createRunDeskClient.importWallet", () => {
  it("clears tray before paste when replace is true", async () => {
    const after = focus();
    const { invoke, calls } = recordingInvoke({
      run_desk_amend: () => focus(),
      run_desk_paste: () => after,
    });
    const client = createRunDeskClient(invoke);
    await expect(client.importWallet("WALLET", true)).resolves.toEqual(after);
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
    const client = createRunDeskClient(invoke);
    await expect(client.importWallet("MORE", false)).resolves.toEqual(after);
    expect(calls).toEqual([
      {
        cmd: "run_desk_paste",
        args: { tray: "wallet", text: "MORE" },
      },
    ]);
  });
});

describe("createRunDeskClient.setSpaceAndFleet", () => {
  it("looks up payout then amends space, fleet, isk, and lp", async () => {
    const after = focus();
    const { invoke, calls } = recordingInvoke({
      lookup_vanguard_payout: () => ({ isk: 10_395_000, lp_per_char: 1_400 }),
      run_desk_amend: () => after,
    });
    const client = createRunDeskClient(invoke);
    await expect(
      client.setSpaceAndFleet(defaultSettings, "highsec", 10),
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
