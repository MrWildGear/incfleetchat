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

export type SiteDetail = {
  occurred_at: string;
  amount_isk: number;
  fleet_isk: number;
  fleet_lp: number;
  gap_seconds: number | null;
  duration_seconds: number | null;
  is_break: boolean;
  counts_toward_avg: boolean;
};

export type HourlyBucket = {
  hour_start: string;
  total_isk: number;
  total_lp: number;
  sites: number;
  avg_site_seconds: number | null;
};

export type SessionSummary = {
  sites_ran: number;
  active_site_seconds: number;
  wallet_elapsed_seconds: number;
  avg_site_seconds: number | null;
  character_liquid_isk: number;
  fleet_liquid_isk: number;
  net_lp: number;
  lp_per_character_total: number | null;
  lp_value: number;
  net_value: number;
  liquid_isk_per_hour: number;
  lp_value_per_hour: number;
  net_per_hour: number;
};

export type AnalyticsReport = {
  session: SessionSummary;
  hourly: HourlyBucket[];
  sites: SiteDetail[];
};

export type SpawnSummary = {
  constellation: string;
  region: string | null;
  staging_system: string | null;
  hq_system: string | null;
  run_count: number;
};

export type RunSummary = {
  run_id: string;
  constellation: string;
  saved_at: string;
  site_count: number;
  /** Fleet liquid ISK for the run (DB column liquid_isk). */
  liquid_isk: number;
};

export type SpawnDraft = {
  constellation: string | null;
  region: string | null;
  security_status: string | null;
  sov_holder: string | null;
  staging_system: string | null;
  hq_system: string | null;
  assault_systems: string[];
  vanguard_systems: string[];
  announced_at: string | null;
  title: string | null;
};

export type EnrichmentSource = "fc" | "borrowed" | "heuristic" | "fleet";

export type EnrichmentSite = {
  occurred_at: string;
  approach_seconds: number | null;
  combat_to_payout_seconds: number | null;
  is_break: boolean;
  source: EnrichmentSource;
};

export type MissileStat = {
  listener: string;
  reload_cycles: number;
  hits: number;
  missiles_per_cycle: number;
  dead: number;
};

export type EnrichmentTotals = {
  approach_seconds: number | null;
  combat_to_payout_seconds: number | null;
  avg_combat_to_payout_seconds: number | null;
  fleet_dead: number;
};

export type EnrichmentSnapshot = {
  resolved_fc: string | null;
  /** Listener headers used (all listeners with logs in range). */
  listeners: string[];
  diagnostics: { level: string; message: string }[];
  sites: EnrichmentSite[];
  missiles: MissileStat[];
  totals: EnrichmentTotals;
};

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
  diagnostics: { level: string; message: string }[];
  session_settings: RunSettings;
  staging_spawn: SpawnDraft | null;
  /** Most recently sealed run in this session; re-enrichable at any scope. */
  sealed_run_id: string | null;
  enrichment: EnrichmentSnapshot | null;
};

export type PayoutTicket = { isk: number; lp_per_char: number };
