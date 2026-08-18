import { invoke } from "@tauri-apps/api/core";
import type { SessionTrackingSiteKind } from "./lib/types";

const ACTIONS: { label: string; siteKind: SessionTrackingSiteKind | null }[] = [
  { label: "OTA (Hacking)", siteKind: "ota_hacking" },
  { label: "NCO", siteKind: "nco" },
  { label: "NMC (Mining)", siteKind: "nmc_mining" },
  { label: "Break", siteKind: null },
];

export function TrackingPipApp() {
  async function onAction(siteKind: SessionTrackingSiteKind | null) {
    try {
      if (siteKind) {
        await invoke("record_session_tracking_event", {
          input: {
            event_kind: "fleet_warp",
            site_kind: siteKind,
          },
        });
      } else {
        await invoke("record_session_tracking_event", {
          input: { event_kind: "break_start" },
        });
      }
    } finally {
      await invoke("hide_tracking_pip").catch(() => {});
    }
  }

  return (
    <div className="flex h-full flex-col bg-surface p-2 text-fg">
      <p className="mb-2 text-xs text-muted">Fleet warp — pick site or break</p>
      <div className="grid gap-1">
        {ACTIONS.map((action) => (
          <button
            key={action.label}
            type="button"
            onClick={() => void onAction(action.siteKind)}
            className={
              action.siteKind
                ? "rounded border border-border px-2 py-1.5 text-left text-xs hover:border-accent/40"
                : "rounded border border-border px-2 py-1.5 text-left text-xs hover:border-overdue/40"
            }
          >
            {action.label}
          </button>
        ))}
      </div>
    </div>
  );
}
