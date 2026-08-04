import type {
  AmendOp,
  EditionFocus,
  EnrichPrelude,
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

export type RunDesk = {
  open(): Promise<EditionFocus>;
  paste(tray: "manifest" | "wallet", text: string): Promise<EditionFocus>;
  pasteManifest(text: string): Promise<EditionFocus>;
  importWallet(text: string, replace: boolean): Promise<EditionFocus>;
  analyze(settings: RunSettings, prelude: EnrichPrelude): Promise<EditionFocus>;
  focus(scope: ReportScope): Promise<EditionFocus>;
  amend(op: AmendOp): Promise<EditionFocus>;
  setSessionSettings(settings: RunSettings): Promise<EditionFocus>;
  setConstellation(constellation: string): Promise<EditionFocus>;
  clearWalletTray(): Promise<EditionFocus>;
  setSpaceAndFleet(
    current: RunSettings,
    space: SpaceBand,
    fleetSize: number,
  ): Promise<EditionFocus>;
  reenrich(
    runId: string | undefined,
    prelude: EnrichPrelude,
  ): Promise<EditionFocus>;
  deleteRun(runId: string): Promise<EditionFocus>;
  deleteSpawn(constellation: string): Promise<EditionFocus>;
  clearAll(): Promise<EditionFocus>;
  patchAppSettings(patch: Record<string, unknown>): Promise<void>;
};

async function persistPrelude(
  invoke: DeskInvoke,
  prelude: EnrichPrelude,
): Promise<void> {
  await invoke("set_settings", {
    patch: {
      gamelogs_dir: prelude.gamelogsDir.trim() || null,
      fc_character: prelude.fcCharacter.trim() || null,
    },
  });
  await invoke("set_settings", {
    patch: {
      ammo_launchers: prelude.ammoLaunchers,
      ammo_per_launcher: prelude.ammoPerLauncher,
    },
  });
}

export function createRunDesk(invoke: DeskInvoke): RunDesk {
  return {
    open: () => invoke<EditionFocus>("run_desk_open"),

    paste: (tray, text) =>
      invoke<EditionFocus>("run_desk_paste", { tray, text }),

    pasteManifest: (text) =>
      invoke<EditionFocus>("run_desk_paste", { tray: "manifest", text }),

    async importWallet(text, replace) {
      if (replace) {
        await invoke<EditionFocus>("run_desk_amend", {
          op: { op: "clear_wallet_tray" },
        });
      }
      return invoke<EditionFocus>("run_desk_paste", { tray: "wallet", text });
    },

    async analyze(settings, prelude) {
      await persistPrelude(invoke, prelude);
      await invoke<EditionFocus>("run_desk_amend", {
        op: { op: "set_session_settings", settings },
      });
      return invoke<EditionFocus>("run_desk_analyze");
    },

    focus: (scope) => invoke<EditionFocus>("run_desk_focus", { scope }),

    amend: (op) => invoke<EditionFocus>("run_desk_amend", { op }),

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

    async setSpaceAndFleet(current, space, fleetSize) {
      const ticket = await invoke<PayoutTicket>("lookup_vanguard_payout", {
        space,
        fleetSize,
      });
      return invoke<EditionFocus>("run_desk_amend", {
        op: {
          op: "set_session_settings",
          settings: {
            ...current,
            space,
            fleet_size: fleetSize,
            expected_isk: ticket.isk,
            lp_per_char: ticket.lp_per_char,
          },
        },
      });
    },

    async reenrich(runId, prelude) {
      await persistPrelude(invoke, prelude);
      return invoke<EditionFocus>("run_desk_reenrich", { runId });
    },

    deleteRun: (runId) =>
      invoke<EditionFocus>("run_desk_delete_run", { runId }),

    deleteSpawn: (constellation) =>
      invoke<EditionFocus>("run_desk_delete_spawn", { constellation }),

    clearAll: () => invoke<EditionFocus>("run_desk_clear_all"),

    async patchAppSettings(patch) {
      await invoke("set_settings", { patch });
    },
  };
}
