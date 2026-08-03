import { formatAppVersionLabel } from "../lib/updateUx";
import type { ProgressStage } from "../lib/updateUx";

type UpdateModalsProps = {
  pendingVersion: string | null;
  promptOpen: boolean;
  progress: null | { version: string; stage: ProgressStage };
  progressError: string | null;
  onUpdateNow: () => void;
  onLater: () => void;
  onCloseProgressError: () => void;
  onRetryInstall: () => void;
};

export function UpdateModals({
  pendingVersion,
  promptOpen,
  progress,
  progressError,
  onUpdateNow,
  onLater,
  onCloseProgressError,
  onRetryInstall,
}: UpdateModalsProps) {
  if (progressError) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
        <div className="w-full max-w-xs rounded-lg border border-border bg-surface-raised p-4 shadow-xl">
          <h2 className="text-sm font-semibold text-fg">Update failed</h2>
          <p className="mt-1 text-xs text-muted">{progressError}</p>
          <div className="mt-4 flex justify-end gap-2">
            <button
              type="button"
              className="rounded-md border border-border px-3 py-1.5 text-xs text-muted hover:text-fg"
              onClick={onCloseProgressError}
            >
              Close
            </button>
            <button
              type="button"
              className="rounded-md border border-accent/40 px-3 py-1.5 text-xs text-accent hover:bg-accent/10"
              onClick={onRetryInstall}
            >
              Retry
            </button>
          </div>
        </div>
      </div>
    );
  }

  if (progress) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
        <div className="w-full max-w-xs rounded-lg border border-border bg-surface-raised p-4 shadow-xl">
          <h2 className="text-sm font-semibold text-fg">
            {progress.stage === "installing" ? "Installing…" : "Updating…"}
          </h2>
          <p className="mt-1 text-xs text-muted">
            {formatAppVersionLabel(progress.version)}
          </p>
          <div className="mt-4 h-0.5 w-full overflow-hidden rounded-full bg-border">
            <div className="animate-indeterminate h-full w-1/3 rounded-full bg-accent" />
          </div>
        </div>
      </div>
    );
  }

  if (promptOpen && pendingVersion) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
        <div className="w-full max-w-xs rounded-lg border border-border bg-surface-raised p-4 shadow-xl">
          <h2 className="text-sm font-semibold text-fg">Update available</h2>
          <p className="mt-1 text-xs text-muted">
            {formatAppVersionLabel(pendingVersion)} is ready.
          </p>
          <div className="mt-4 flex justify-end gap-2">
            <button
              type="button"
              className="rounded-md border border-border px-3 py-1.5 text-xs text-muted hover:text-fg"
              onClick={onLater}
            >
              Later
            </button>
            <button
              type="button"
              className="rounded-md border border-accent/40 px-3 py-1.5 text-xs text-accent hover:bg-accent/10"
              onClick={onUpdateNow}
            >
              Update now
            </button>
          </div>
        </div>
      </div>
    );
  }

  return null;
}
