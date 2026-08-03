import { useCallback, useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  manualCheckOutcome,
  progressStageFromUpdaterEvent,
  type ManualCheckOutcome,
  type ProgressStage,
} from "../lib/updateUx";

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

export function useAppUpdater(): AppUpdaterState {
  const [appVersion, setAppVersion] = useState<string | null>(null);
  const [pendingVersion, setPendingVersion] = useState<string | null>(null);
  const [promptOpen, setPromptOpen] = useState(false);
  const [progress, setProgress] = useState<null | {
    version: string;
    stage: ProgressStage;
  }>(null);
  const [progressError, setProgressError] = useState<string | null>(null);
  const [deferredUpdate, setDeferredUpdate] = useState(false);
  const [manualResult, setManualResult] = useState<ManualCheckOutcome | null>(
    null,
  );
  const [checkingManual, setCheckingManual] = useState(false);

  const updateRef = useRef<Update | null>(null);

  useEffect(() => {
    void getVersion()
      .then(setAppVersion)
      .catch(() => setAppVersion(null));
  }, []);

  const runLaunchCheck = useCallback(async () => {
    try {
      const update = await check();
      if (update) {
        updateRef.current = update;
        setPendingVersion(update.version);
        setPromptOpen(true);
      }
    } catch {
      // Silent: launch checks should never interrupt the user.
    }
  }, []);

  const checkManual = useCallback(async () => {
    setCheckingManual(true);
    try {
      const update = await check();
      updateRef.current = update;
      setManualResult(manualCheckOutcome(update));
      if (update) {
        setPendingVersion(update.version);
        setPromptOpen(true);
        setDeferredUpdate(false);
      }
    } catch (err) {
      setManualResult(manualCheckOutcome(null, err));
    } finally {
      setCheckingManual(false);
    }
  }, []);

  const installUpdate = useCallback(async () => {
    const update = updateRef.current;
    if (!update) return;
    setProgressError(null);
    setProgress({ version: update.version, stage: "downloading" });
    try {
      await update.downloadAndInstall((event) => {
        setProgress({
          version: update.version,
          stage: progressStageFromUpdaterEvent(event.event),
        });
      });
      setDeferredUpdate(false);
      await relaunch();
    } catch (err) {
      setProgress(null);
      setProgressError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  const onUpdateNow = useCallback(async () => {
    setPromptOpen(false);
    await installUpdate();
  }, [installUpdate]);

  const onLater = useCallback(() => {
    setPromptOpen(false);
    setDeferredUpdate(true);
  }, []);

  const onCloseProgressError = useCallback(() => {
    setProgressError(null);
  }, []);

  const onRetryInstall = useCallback(async () => {
    setProgressError(null);
    await installUpdate();
  }, [installUpdate]);

  return {
    appVersion,
    pendingVersion,
    promptOpen,
    progress,
    progressError,
    deferredUpdate,
    manualResult,
    checkingManual,
    onUpdateNow,
    onLater,
    onCloseProgressError,
    onRetryInstall,
    checkManual,
    runLaunchCheck,
  };
}
