import { useCallback, useEffect, useMemo, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  createUpdateSession,
  type UpdateSessionSnapshot,
  type UpdaterPorts,
} from "../lib/updateSession";
import type { ManualCheckOutcome, ProgressStage } from "../lib/updateUx";

const defaultPorts: UpdaterPorts = {
  getVersion,
  check: () => check() as ReturnType<UpdaterPorts["check"]>,
  relaunch,
};

export interface AppUpdaterState {
  appVersion: string | null;
  pendingVersion: string | null;
  promptOpen: boolean;
  progress: null | { version: string; stage: ProgressStage };
  progressError: string | null;
  deferredUpdate: boolean;
  manualResult: ManualCheckOutcome | null;
  checkingManual: boolean;
  onUpdateNow: () => Promise<void>;
  onLater: () => void;
  onCloseProgressError: () => void;
  onRetryInstall: () => Promise<void>;
  checkManual: () => Promise<void>;
  runLaunchCheck: () => Promise<void>;
}

function snapToState(
  snap: UpdateSessionSnapshot,
  actions: Pick<
    AppUpdaterState,
    | "onUpdateNow"
    | "onLater"
    | "onCloseProgressError"
    | "onRetryInstall"
    | "checkManual"
    | "runLaunchCheck"
  >,
): AppUpdaterState {
  return { ...snap, ...actions };
}

export function useAppUpdater(ports: UpdaterPorts = defaultPorts): AppUpdaterState {
  const session = useMemo(() => createUpdateSession(ports), [ports]);
  const [snap, setSnap] = useState(() => session.getSnapshot());

  useEffect(() => {
    return session.subscribe(() => setSnap(session.getSnapshot()));
  }, [session]);

  useEffect(() => {
    void session.loadVersion();
  }, [session]);

  const onUpdateNow = useCallback(() => session.onUpdateNow(), [session]);
  const onLater = useCallback(() => session.onLater(), [session]);
  const onCloseProgressError = useCallback(
    () => session.onCloseProgressError(),
    [session],
  );
  const onRetryInstall = useCallback(() => session.onRetryInstall(), [session]);
  const checkManual = useCallback(() => session.checkManual(), [session]);
  const runLaunchCheck = useCallback(() => session.runLaunchCheck(), [session]);

  return snapToState(snap, {
    onUpdateNow,
    onLater,
    onCloseProgressError,
    onRetryInstall,
    checkManual,
    runLaunchCheck,
  });
}
