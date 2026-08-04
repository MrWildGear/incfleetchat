import { useCallback, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  EditionFocus,
  EnrichPrelude,
  ReportScope,
  RunSettings,
  SpaceBand,
} from "../lib/runDeskTypes";
import { createRunDeskClient } from "../lib/runDesk";

const defaultSettings: RunSettings = {
  space: "low_null",
  fleet_size: 15,
  expected_isk: 15_000_000,
  lp_per_char: 2_000,
  isk_per_lp: 1400,
  break_threshold_minutes: 25,
  run_start: null,
};

export type UseRunDeskResult = {
  focus: EditionFocus | null;
  error: string | null;
  settings: RunSettings;
  enriching: boolean;
  scopeBusy: boolean;
  deleting: boolean;
  applyFocus: (f: EditionFocus) => void;
  open: () => Promise<void>;
  analyze: (prelude: EnrichPrelude) => Promise<void>;
  reenrich: (prelude: EnrichPrelude) => Promise<void>;
  pasteManifest: (text: string) => Promise<void>;
  importWallet: (text: string, replace: boolean) => Promise<EditionFocus | null>;
  setScope: (scope: ReportScope) => Promise<void>;
  setSessionSettings: (next: RunSettings) => Promise<void>;
  setSpaceAndFleet: (space: SpaceBand, fleetSize: number) => Promise<void>;
  setConstellation: (constellation: string) => Promise<void>;
  clearWalletTray: () => Promise<void>;
  deleteRun: (runId: string) => Promise<void>;
  deleteSpawn: (constellation: string) => Promise<void>;
  clearAll: () => Promise<void>;
  patchAppSettings: (patch: Record<string, unknown>) => Promise<void>;
  setError: (message: string | null) => void;
};

export function useRunDesk(): UseRunDeskResult {
  const client = useMemo(
    () =>
      createRunDeskClient((cmd, args) =>
        args === undefined ? invoke(cmd) : invoke(cmd, args),
      ),
    [],
  );

  const [focus, setFocus] = useState<EditionFocus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [settings, setSettings] = useState<RunSettings>(defaultSettings);
  const [enriching, setEnriching] = useState(false);
  const [scopeBusy, setScopeBusy] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const applyFocus = useCallback((f: EditionFocus) => {
    setFocus(f);
    setSettings(f.session_settings);
    setError(null);
  }, []);

  const open = useCallback(async () => {
    try {
      applyFocus(await client.cmds.open());
    } catch (e) {
      setError(String(e));
    }
  }, [applyFocus, client]);

  const analyze = useCallback(
    async (prelude: EnrichPrelude) => {
      try {
        setEnriching(true);
        applyFocus(await client.analyze(settings, prelude));
      } catch (e) {
        setError(String(e));
      } finally {
        setEnriching(false);
      }
    },
    [applyFocus, client, settings],
  );

  const reenrich = useCallback(
    async (prelude: EnrichPrelude) => {
      try {
        setEnriching(true);
        const runId =
          focus?.scope.kind === "run" ? focus.scope.run_id : undefined;
        applyFocus(await client.reenrich(runId, prelude));
      } catch (e) {
        setError(String(e));
      } finally {
        setEnriching(false);
      }
    },
    [applyFocus, client, focus?.scope],
  );

  const pasteManifest = useCallback(
    async (text: string) => {
      try {
        applyFocus(await client.cmds.pasteManifest(text));
      } catch (e) {
        setError(String(e));
      }
    },
    [applyFocus, client],
  );

  const importWallet = useCallback(
    async (text: string, replace: boolean) => {
      try {
        const f = await client.importWallet(text, replace);
        applyFocus(f);
        return f;
      } catch (e) {
        setError(String(e));
        return null;
      }
    },
    [applyFocus, client],
  );

  const setScope = useCallback(
    async (scope: ReportScope) => {
      try {
        setScopeBusy(true);
        applyFocus(await client.cmds.focus(scope));
      } catch (e) {
        setError(String(e));
      } finally {
        setScopeBusy(false);
      }
    },
    [applyFocus, client],
  );

  const setSessionSettings = useCallback(
    async (next: RunSettings) => {
      setSettings(next);
      try {
        applyFocus(await client.cmds.setSessionSettings(next));
      } catch (e) {
        setError(String(e));
      }
    },
    [applyFocus, client],
  );

  const setSpaceAndFleet = useCallback(
    async (space: SpaceBand, fleetSize: number) => {
      try {
        applyFocus(await client.setSpaceAndFleet(settings, space, fleetSize));
      } catch (e) {
        setError(String(e));
      }
    },
    [applyFocus, client, settings],
  );

  const setConstellation = useCallback(
    async (constellation: string) => {
      try {
        applyFocus(await client.cmds.setConstellation(constellation));
      } catch (e) {
        setError(String(e));
      }
    },
    [applyFocus, client],
  );

  const clearWalletTray = useCallback(async () => {
    try {
      applyFocus(await client.cmds.clearWalletTray());
    } catch (e) {
      setError(String(e));
      throw e;
    }
  }, [applyFocus, client]);

  const deleteRun = useCallback(
    async (runId: string) => {
      try {
        setDeleting(true);
        applyFocus(await client.cmds.deleteRun(runId));
      } catch (e) {
        setError(String(e));
      } finally {
        setDeleting(false);
      }
    },
    [applyFocus, client],
  );

  const deleteSpawn = useCallback(
    async (constellation: string) => {
      try {
        setDeleting(true);
        applyFocus(await client.cmds.deleteSpawn(constellation));
      } catch (e) {
        setError(String(e));
      } finally {
        setDeleting(false);
      }
    },
    [applyFocus, client],
  );

  const clearAll = useCallback(async () => {
    try {
      setDeleting(true);
      applyFocus(await client.cmds.clearAll());
    } catch (e) {
      setError(String(e));
    } finally {
      setDeleting(false);
    }
  }, [applyFocus, client]);

  const patchAppSettings = useCallback(
    async (patch: Record<string, unknown>) => {
      try {
        await client.cmds.patchAppSettings(patch);
      } catch (e) {
        setError(String(e));
        throw e;
      }
    },
    [client],
  );

  return {
    focus,
    error,
    settings,
    enriching,
    scopeBusy,
    deleting,
    applyFocus,
    open,
    analyze,
    reenrich,
    pasteManifest,
    importWallet,
    setScope,
    setSessionSettings,
    setSpaceAndFleet,
    setConstellation,
    clearWalletTray,
    deleteRun,
    deleteSpawn,
    clearAll,
    patchAppSettings,
    setError,
  };
}
