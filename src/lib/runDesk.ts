import type {
  EditionFocus,
  EnrichmentInputs,
  PayoutTicket,
  ReportScope,
  RunSettings,
  SpaceBand,
} from "./runDeskTypes";

/** Tauri-compatible invoke used as the RunDesk adapter. */
export type DeskInvoke = <T>(
  cmd: string,
  args?: Record<string, unknown>,
) => Promise<T>;

/** One-shot RunDesk commands — thin EditionFocus invokes. */
export type RunDeskCmds = {
  open(): Promise<EditionFocus>;
  paste(tray: "manifest" | "wallet", text: string): Promise<EditionFocus>;
  pasteManifest(text: string): Promise<EditionFocus>;
  focus(scope: ReportScope): Promise<EditionFocus>;
  setSessionSettings(settings: RunSettings): Promise<EditionFocus>;
  setConstellation(constellation: string): Promise<EditionFocus>;
  clearWalletTray(): Promise<EditionFocus>;
  deleteRun(runId: string): Promise<EditionFocus>;
  deleteSpawn(constellation: string): Promise<EditionFocus>;
  clearAll(): Promise<EditionFocus>;
  patchAppSettings(patch: Record<string, unknown>): Promise<void>;
};

/** Multi-step orchestration that composes cmds / side effects. */
export type RunDeskOps = {
  importWallet(text: string, replace: boolean): Promise<EditionFocus>;
  analyze(settings: RunSettings, inputs: EnrichmentInputs): Promise<EditionFocus>;
  setSpaceAndFleet(
    current: RunSettings,
    space: SpaceBand,
    fleetSize: number,
  ): Promise<EditionFocus>;
  reenrich(
    runId: string | undefined,
    inputs: EnrichmentInputs,
  ): Promise<EditionFocus>;
};

export type RunDeskClient = RunDeskOps & {
  cmds: RunDeskCmds;
};

export function createRunDeskClient(invoke: DeskInvoke): RunDeskClient {
  const cmds: RunDeskCmds = {
    open: () => invoke<EditionFocus>("run_desk_open"),

    paste: (tray, text) =>
      invoke<EditionFocus>("run_desk_paste", { tray, text }),

    pasteManifest: (text) =>
      invoke<EditionFocus>("run_desk_paste", { tray: "manifest", text }),

    focus: (scope) => invoke<EditionFocus>("run_desk_focus", { scope }),

    setSessionSettings: (settings) =>
      invoke<EditionFocus>("run_desk_amend", {
        op: { op: "set_session_settings", settings },
      }),

    setConstellation: (constellation) =>
      invoke<EditionFocus>("run_desk_amend", {
        op: { op: "set_constellation", constellation },
      }),

    clearWalletTray: () =>
      invoke<EditionFocus>("run_desk_amend", {
        op: { op: "clear_wallet_tray" },
      }),

    deleteRun: (runId) =>
      invoke<EditionFocus>("run_desk_delete_run", { runId }),

    deleteSpawn: (constellation) =>
      invoke<EditionFocus>("run_desk_delete_spawn", { constellation }),

    clearAll: () => invoke<EditionFocus>("run_desk_clear_all"),

    async patchAppSettings(patch) {
      await invoke("set_settings", { patch });
    },
  };

  return {
    cmds,

    async importWallet(text, replace) {
      if (replace) {
        await cmds.clearWalletTray();
      }
      return cmds.paste("wallet", text);
    },

    async analyze(settings, inputs) {
      await cmds.setSessionSettings(settings);
      return invoke<EditionFocus>("run_desk_analyze", { inputs });
    },

    async setSpaceAndFleet(current, space, fleetSize) {
      const ticket = await invoke<PayoutTicket>("lookup_vanguard_payout", {
        space,
        fleetSize,
      });
      return cmds.setSessionSettings({
        ...current,
        space,
        fleet_size: fleetSize,
        expected_isk: ticket.isk,
        lp_per_char: ticket.lp_per_char,
      });
    },

    async reenrich(runId, inputs) {
      return invoke<EditionFocus>("run_desk_reenrich", { runId, inputs });
    },
  };
}
