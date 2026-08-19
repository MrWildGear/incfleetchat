export type SitePhase = "active" | "overdue" | "ready";

export type SiteRow = {
  id: string;
  tag: string;
  speaker: string;
  posted_at: string;
  expires_at: string;
  ran: boolean;
  phase: SitePhase;
  clearable: boolean;

  // Metrics added to support the Drill Down View (Task 2)
  duration_minutes: number | null;
  approach_type: "automatic" | "manual" | null; // Assuming an enum/string for 'Approach'
  combat_payout: number | null; // For Combat->Payout
  hit_percent: number | null;   // For Hit %
  miss_percent: number | null;  // For Miss %
};

export type BoardStatus =
  | { kind: "watching"; character: string; log_name: string }
  | { kind: "no_character" }
  | { kind: "waiting_for_log"; character: string }
  | { kind: "error"; message: string };

export type Board = {
  status: BoardStatus;
  sites: SiteRow[];
  ready_count: number;
  updated_at: string;
};

export type OverlaySettings = {
  character: string | null;
  chatlogs_dir: string | null;
  always_on_top: boolean;
  tracking_pip_enabled: boolean;
};

export type SessionTrackingSiteKind = "ota_hacking" | "nco" | "nmc_mining";

export type SessionTrackingEventKind = "fleet_warp" | "break_start";

export type SessionTrackingEvent = {
  id: number;
  fleet_log_id: string;
  event_kind: SessionTrackingEventKind;
  site_kind: SessionTrackingSiteKind | null;
  overlay_site_id: string | null;
  occurred_at: string;
};

export type RecordSessionTrackingInput = {
  event_kind: SessionTrackingEventKind;
  site_kind?: SessionTrackingSiteKind | null;
  overlay_site_id?: string | null;
};

export const emptyBoard: Board = {
  status: { kind: "no_character" },
  sites: [],
  ready_count: 0,
  updated_at: new Date().toISOString(),
};
