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
};

export const emptyBoard: Board = {
  status: { kind: "no_character" },
  sites: [],
  ready_count: 0,
  updated_at: new Date().toISOString(),
};
