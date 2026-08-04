/** Public Run desk contract. Nested report/enrichment shapes stay in analyticsInternals. */

import type {
  AnalyticsReport,
  Diagnostic,
  EnrichmentSnapshot,
  RunSummary,
  SpawnDraft,
  SpawnSummary,
} from "./analyticsInternals";

export type SpaceBand = "highsec" | "low_null";

export type RunSettings = {
  space: SpaceBand;
  fleet_size: number;
  expected_isk: number;
  lp_per_char: number;
  isk_per_lp: number;
  break_threshold_minutes: number;
  run_start: string | null;
};

export type ReportScope =
  | { kind: "overall" }
  | { kind: "spawn"; constellation: string }
  | { kind: "run"; run_id: string };

export type EnrichPrelude = {
  gamelogsDir: string;
  fcCharacter: string;
  ammoLaunchers: number;
  ammoPerLauncher: number;
};

export type AmendOp =
  | { op: "clear_wallet_tray" }
  | { op: "set_session_settings"; settings: RunSettings }
  | { op: "reopen_trays" }
  | { op: "open_run"; run_id: string }
  | { op: "set_constellation"; constellation: string }
  | { op: "reenrich_run"; run_id: string | null };

export type PayoutTicket = { isk: number; lp_per_char: number };

export type EditionFocus = {
  trays: {
    manifest: string;
    wallet_batches: number;
    pending_sites: number;
  };
  spawn: SpawnSummary | null;
  catalog: { spawns: SpawnSummary[]; runs: RunSummary[] };
  scope: ReportScope;
  report: AnalyticsReport | null;
  diagnostics: Diagnostic[];
  session_settings: RunSettings;
  staging_spawn: SpawnDraft | null;
  /** Most recently sealed run in this session; re-enrichable at any scope. */
  sealed_run_id: string | null;
  enrichment: EnrichmentSnapshot | null;
};
