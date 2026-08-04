/** Nested analytics wire shapes. Import only from join/rate modules and runDeskTypes. */

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

export type Diagnostic = {
  level: string;
  message: string;
};

export type EnrichmentSource = "fc" | "borrowed" | "heuristic" | "fleet";

export type MissileStat = {
  listener: string;
  reload_cycles: number;
  hits: number;
  missiles_per_cycle: number;
  /** Launchers at enrich time (= missiles per volley). */
  launchers: number;
  dead: number;
};

export type EnrichmentSite = {
  occurred_at: string;
  approach_seconds: number | null;
  combat_to_payout_seconds: number | null;
  is_break: boolean;
  source: EnrichmentSource;
  missiles: MissileStat[];
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
  diagnostics: Diagnostic[];
  sites: EnrichmentSite[];
  missiles: MissileStat[];
  totals: EnrichmentTotals;
};
