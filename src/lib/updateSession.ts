import {
  manualCheckOutcome,
  progressStageFromUpdaterEvent,
  type ManualCheckOutcome,
  type ProgressStage,
} from "./updateUx";

/** Minimal update handle — satisfied by Tauri Update in production. */
export type UpdateHandle = {
  version: string;
  downloadAndInstall: (
    onEvent: (event: { event: "Started" | "Progress" | "Finished" }) => void,
  ) => Promise<void>;
};

export type UpdaterPorts = {
  getVersion: () => Promise<string>;
  check: () => Promise<UpdateHandle | null>;
  relaunch: () => Promise<void>;
};

export type UpdateSessionSnapshot = {
  appVersion: string | null;
  pendingVersion: string | null;
  promptOpen: boolean;
  progress: null | { version: string; stage: ProgressStage };
  progressError: string | null;
  deferredUpdate: boolean;
  manualResult: ManualCheckOutcome | null;
  checkingManual: boolean;
};

export type UpdateSession = {
  getSnapshot: () => UpdateSessionSnapshot;
  subscribe: (listener: () => void) => () => void;
  loadVersion: () => Promise<void>;
  runLaunchCheck: () => Promise<void>;
  checkManual: () => Promise<void>;
  onUpdateNow: () => Promise<void>;
  onLater: () => void;
  onCloseProgressError: () => void;
  onRetryInstall: () => Promise<void>;
};

export function createUpdateSession(ports: UpdaterPorts): UpdateSession {
  let appVersion: string | null = null;
  let pendingVersion: string | null = null;
  let promptOpen = false;
  let progress: null | { version: string; stage: ProgressStage } = null;
  let progressError: string | null = null;
  let deferredUpdate = false;
  let manualResult: ManualCheckOutcome | null = null;
  let checkingManual = false;
  let heldUpdate: UpdateHandle | null = null;

  const listeners = new Set<() => void>();

  function emit() {
    for (const listener of listeners) listener();
  }

  function getSnapshot(): UpdateSessionSnapshot {
    return {
      appVersion,
      pendingVersion,
      promptOpen,
      progress,
      progressError,
      deferredUpdate,
      manualResult,
      checkingManual,
    };
  }

  async function installUpdate() {
    const update = heldUpdate;
    if (!update) return;
    progressError = null;
    progress = { version: update.version, stage: "downloading" };
    emit();
    try {
      await update.downloadAndInstall((event) => {
        progress = {
          version: update.version,
          stage: progressStageFromUpdaterEvent(event.event),
        };
        emit();
      });
      deferredUpdate = false;
      emit();
      await ports.relaunch();
    } catch (err) {
      progress = null;
      progressError = err instanceof Error ? err.message : String(err);
      emit();
    }
  }

  return {
    getSnapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },

    async loadVersion() {
      try {
        appVersion = await ports.getVersion();
      } catch {
        appVersion = null;
      }
      emit();
    },

    async runLaunchCheck() {
      try {
        const update = await ports.check();
        if (update) {
          heldUpdate = update;
          pendingVersion = update.version;
          promptOpen = true;
          emit();
        }
      } catch {
        // Silent: launch checks should never interrupt the user.
      }
    },

    async checkManual() {
      checkingManual = true;
      emit();
      try {
        const update = await ports.check();
        heldUpdate = update;
        manualResult = manualCheckOutcome(update);
        if (update) {
          pendingVersion = update.version;
          promptOpen = true;
          deferredUpdate = false;
        } else {
          pendingVersion = null;
          deferredUpdate = false;
        }
      } catch (err) {
        manualResult = manualCheckOutcome(null, err);
      } finally {
        checkingManual = false;
        emit();
      }
    },

    async onUpdateNow() {
      promptOpen = false;
      emit();
      await installUpdate();
    },

    onLater() {
      promptOpen = false;
      deferredUpdate = true;
      emit();
    },

    onCloseProgressError() {
      progressError = null;
      emit();
    },

    async onRetryInstall() {
      progressError = null;
      emit();
      await installUpdate();
    },
  };
}
