import { useEffect } from "react";
import { useAppStore } from "../store";
import { cn } from "../lib/utils";
import { formatAppVersionLabel, type ManualCheckOutcome } from "../lib/updateUx";

type SettingsPanelProps = {
  appVersion: string | null;
  checkManual: () => Promise<void>;
  checkingManual: boolean;
  manualResult: ManualCheckOutcome | null;
};

export function SettingsPanel({
  appVersion,
  checkManual,
  checkingManual,
  manualResult,
}: SettingsPanelProps) {
  const open = useAppStore((s) => s.settingsOpen);
  const settings = useAppStore((s) => s.settings);
  const characters = useAppStore((s) => s.characters);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);
  const loadCharacters = useAppStore((s) => s.loadCharacters);
  const saveSettings = useAppStore((s) => s.saveSettings);

  useEffect(() => {
    if (open) void loadCharacters();
  }, [open, loadCharacters]);

  if (!open || !settings) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-end sm:items-center justify-center bg-black/60 p-4">
      <div className="w-full max-w-md rounded-lg border border-border bg-surface-raised p-4 shadow-xl">
        <h2 className="text-lg font-semibold text-fg">Settings</h2>
        <p className="mt-1 text-sm text-muted">
          Pick the Listener character whose fleet log to follow.
        </p>

        <label className="mt-4 block text-xs uppercase tracking-wide text-muted">
          Character
        </label>
        <select
          className="mt-1 w-full rounded-md border border-border bg-surface px-3 py-2 text-sm text-fg"
          value={settings.character ?? ""}
          onChange={(e) => {
            const v = e.target.value || null;
            void saveSettings({ character: v });
          }}
        >
          <option value="">Select character…</option>
          {characters.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>

        <label className="mt-4 block text-xs uppercase tracking-wide text-muted">
          Chatlogs folder
        </label>
        <input
          className="mt-1 w-full rounded-md border border-border bg-surface px-3 py-2 text-sm text-fg"
          defaultValue={settings.chatlogs_dir ?? ""}
          placeholder="Default: Documents/EVE/logs/Chatlogs"
          onBlur={(e) => {
            const v = e.target.value.trim();
            void saveSettings({ chatlogs_dir: v || null });
          }}
        />

        <label className="mt-4 flex items-center gap-2 text-sm text-fg">
          <input
            type="checkbox"
            checked={settings.always_on_top}
            onChange={(e) => void saveSettings({ always_on_top: e.target.checked })}
          />
          Always on top
        </label>

        <label className="mt-3 flex items-center gap-2 text-sm text-fg">
          <input
            type="checkbox"
            checked={settings.tracking_pip_enabled}
            onChange={(e) =>
              void saveSettings({ tracking_pip_enabled: e.target.checked })
            }
          />
          Fleet warp popup
        </label>
        <p className="mt-1 text-xs text-muted">
          If dismissed, it reopens on the next new warp.
        </p>

        <div className="mt-6 border-t border-border pt-4">
          <p className="text-xs uppercase tracking-wide text-muted">About</p>
          <p className="mt-1 text-sm text-fg">
            IncFleetChat {appVersion ? formatAppVersionLabel(appVersion) : "…"}
          </p>
          <button
            type="button"
            disabled={checkingManual}
            className="mt-2 w-full rounded-md border border-border px-3 py-1.5 text-xs text-muted hover:text-fg disabled:opacity-50"
            onClick={() => void checkManual()}
          >
            {checkingManual ? "Checking…" : "Check for updates"}
          </button>
          {manualResult?.kind === "upToDate" && (
            <p className="mt-1 text-[10px] text-muted">Up to date</p>
          )}
          {manualResult?.kind === "available" && (
            <p className="mt-1 text-[10px] text-muted">
              {formatAppVersionLabel(manualResult.version)} available
            </p>
          )}
          {manualResult?.kind === "error" && (
            <p className="mt-1 text-[10px] text-overdue">{manualResult.message}</p>
          )}
        </div>

        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            className={cn(
              "rounded-md border border-border px-3 py-1.5 text-sm text-muted hover:text-fg",
            )}
            onClick={() => setSettingsOpen(false)}
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
