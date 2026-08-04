import { useMemo, useState } from "react";
import {
  formatCount,
  formatIskMoney,
  formatLp,
  formatPercent,
} from "../lib/formatAnalytics";
import type { EditionFocus, RunSettings } from "../lib/runDeskTypes";
import { joinAnalytics } from "../lib/joinedResults";
import { activeOnly, isActive, of, sum } from "../lib/missileRates";
import { cn } from "../lib/utils";

type SiteSortKey =
  | "time"
  | "duration"
  | "break"
  | "approach"
  | "combat"
  | "hitPct"
  | "missPct";

type SiteSort = { key: SiteSortKey; dir: "asc" | "desc" };

const defaultSiteSort: SiteSort = { key: "time", dir: "asc" };

function compareNullableNumber(
  a: number | null,
  b: number | null,
  sign: number,
): number {
  if (a == null && b == null) return 0;
  if (a == null) return 1;
  if (b == null) return -1;
  return (a - b) * sign;
}

function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds)) return "—";
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  return `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
}

export type JoinedResultsViewProps = {
  focus: EditionFocus | null;
  settings: RunSettings;
  enriching: boolean;
  onReenrich: () => void;
};

export function JoinedResultsView({
  focus,
  settings,
  enriching,
  onReenrich,
}: JoinedResultsViewProps) {
  const [showDrilldown, setShowDrilldown] = useState(false);
  const [siteDrill, setSiteDrill] = useState<null | {
    level: 1 | 2;
    occurredAt: string;
  }>(null);
  const [siteSort, setSiteSort] = useState<SiteSort>(defaultSiteSort);
  const [showListeners, setShowListeners] = useState(false);
  const [showEnrichLog, setShowEnrichLog] = useState(false);

  const session = focus?.report?.session;
  const canReenrich =
    focus?.scope.kind === "run" ||
    (focus?.scope.kind === "spawn" && (focus.spawn?.run_count ?? 0) > 0) ||
    (focus?.scope.kind === "overall" && (focus.catalog?.runs?.length ?? 0) > 0);
  const visibleMissiles = activeOnly(focus?.enrichment?.missiles ?? []);
  const fleetMissileSum = useMemo(
    () => sum(focus?.enrichment?.missiles ?? []),
    [focus?.enrichment?.missiles],
  );
  const joined = useMemo(
    () => joinAnalytics(focus?.report ?? null, focus?.enrichment ?? null),
    [focus?.report, focus?.enrichment],
  );
  const sortedSiteRows = useMemo(() => {
    const rows = joined.sites.map((row, index) => ({ ...row, index }));
    const { key, dir } = siteSort;
    const sign = dir === "asc" ? 1 : -1;
    return [...rows].sort((a, b) => {
      let cmp = 0;
      switch (key) {
        case "time":
          cmp =
            (Date.parse(a.site.occurred_at) - Date.parse(b.site.occurred_at)) *
            sign;
          break;
        case "duration":
          cmp = compareNullableNumber(
            a.site.duration_seconds ?? null,
            b.site.duration_seconds ?? null,
            sign,
          );
          break;
        case "break":
          cmp = (Number(a.site.is_break) - Number(b.site.is_break)) * sign;
          break;
        case "approach":
          cmp = compareNullableNumber(
            a.enrichment?.approach_seconds ?? null,
            b.enrichment?.approach_seconds ?? null,
            sign,
          );
          break;
        case "combat":
          cmp = compareNullableNumber(
            a.enrichment?.combat_to_payout_seconds ?? null,
            b.enrichment?.combat_to_payout_seconds ?? null,
            sign,
          );
          break;
        case "hitPct":
          cmp = compareNullableNumber(a.hitPct, b.hitPct, sign);
          break;
        case "missPct":
          cmp = compareNullableNumber(a.missPct, b.missPct, sign);
          break;
      }
      return cmp === 0 ? a.index - b.index : cmp;
    });
  }, [joined.sites, siteSort]);
  const siteDrillEnrich = siteDrill
    ? focus?.enrichment?.sites.find(
        (x) => x.occurred_at === siteDrill.occurredAt,
      )
    : undefined;
  const siteDrillSum = useMemo(
    () => sum(siteDrillEnrich?.missiles ?? []),
    [siteDrillEnrich?.missiles],
  );
  const siteDrillMissiles = activeOnly(siteDrillEnrich?.missiles ?? []);
  const topDiagnostics = useMemo(() => {
    const generic = focus?.diagnostics ?? [];
    const enrichmentOnly = focus?.enrichment?.diagnostics ?? [];
    return [...generic, ...enrichmentOnly];
  }, [focus?.diagnostics, focus?.enrichment?.diagnostics]);

  return (
    <>
<div className="flex shrink-0 items-center justify-between gap-2">
  <div className="flex items-center gap-2">
    <h2 className="text-xs font-semibold text-muted">Results</h2>
    {enriching ? (
      <span className="text-xs text-accent">Enriching…</span>
    ) : null}
  </div>
  <div className="flex flex-wrap justify-end gap-2">
    <button
      type="button"
      disabled={!canReenrich || enriching}
      onClick={() => void onReenrich()}
      className={cn(
        "rounded border px-2 py-1 text-xs",
        canReenrich && !enriching
          ? "border-accent/40 bg-accent/10 text-accent hover:bg-accent/20"
          : "cursor-not-allowed border-border text-muted/40",
      )}
      title="Recompute combat→payout and dead missiles from gamelogs"
    >
      Re-enrich
    </button>
    <button
      type="button"
      disabled={
        !focus?.enrichment && topDiagnostics.length === 0
      }
      onClick={() => setShowEnrichLog((v) => !v)}
      className={cn(
        "rounded border px-2 py-1 text-xs",
        focus?.enrichment || topDiagnostics.length > 0
          ? "border-accent/40 text-accent hover:bg-accent/10"
          : "cursor-not-allowed border-border text-muted/40",
      )}
    >
      {showEnrichLog ? "Hide enrich log" : "Enrich log"}
    </button>
    <button
      type="button"
      disabled={!focus?.report?.sites?.length}
      onClick={() =>
        setShowDrilldown((v) => {
          if (v) setSiteDrill(null);
          return !v;
        })
      }
      className={cn(
        "rounded border px-2 py-1 text-xs",
        focus?.report?.sites?.length
          ? "border-accent/40 text-accent hover:bg-accent/10"
          : "cursor-not-allowed border-border text-muted/40",
      )}
    >
      {showDrilldown ? "Hide site list" : "Per-site drill-down"}
    </button>
    <button
      type="button"
      disabled={!visibleMissiles.length}
      onClick={() => setShowListeners((v) => !v)}
      className={cn(
        "rounded border px-2 py-1 text-xs",
        visibleMissiles.length
          ? "border-accent/40 text-accent hover:bg-accent/10"
          : "cursor-not-allowed border-border text-muted/40",
      )}
    >
      {showListeners ? "Hide listeners" : "Listeners"}
    </button>
  </div>
</div>

<div className="grid max-h-[40vh] shrink-0 gap-3 lg:grid-cols-[1fr_280px]">
  <div className="overflow-auto rounded border border-border">
    <table className="w-full text-left text-xs">
      <thead className="sticky top-0 z-10 bg-surface-raised text-muted">
        <tr>
          <th className="px-2 py-1">Hour</th>
          <th className="px-2 py-1">Total ISK</th>
          <th className="px-2 py-1">Total LP</th>
          <th className="px-2 py-1">Sites</th>
          <th className="px-2 py-1">Avg site</th>
          <th className="px-2 py-1">Avg combat→payout</th>
          <th className="px-2 py-1">Hit %</th>
          <th className="px-2 py-1">Miss %</th>
        </tr>
      </thead>
      <tbody>
        {joined.hours.map(
          ({ bucket: h, avgCombatToPayout, hitPct, missPct }) => (
          <tr key={h.hour_start} className="border-t border-border">
            <td className="px-2 py-1">
              {new Date(h.hour_start).toLocaleString()}
            </td>
            <td className="px-2 py-1">{formatIskMoney(h.total_isk)}</td>
            <td className="px-2 py-1">{formatLp(h.total_lp)}</td>
            <td className="px-2 py-1">{formatCount(h.sites)}</td>
            <td className="px-2 py-1">
              {formatDuration(h.avg_site_seconds)}
            </td>
            <td className="px-2 py-1">
              {formatDuration(avgCombatToPayout)}
            </td>
            <td className="px-2 py-1">
              {formatPercent(hitPct)}
            </td>
            <td className="px-2 py-1">
              {formatPercent(missPct)}
            </td>
          </tr>
          ),
        )}
        {!joined.hours.length && (
          <tr>
            <td colSpan={8} className="px-2 py-6 text-center text-muted">
              No report yet — paste Manifest + wallet and Analyze
            </td>
          </tr>
        )}
      </tbody>
    </table>
  </div>

  <aside className="space-y-2 overflow-auto rounded border border-border bg-surface-raised p-3 text-xs">
    <h2 className="font-semibold">Summary</h2>
    {session ? (
      <>
        <Row
          label="Time spent"
          value={formatDuration(session.active_site_seconds)}
        />
        <Row
          label="Wallet elapsed"
          value={formatDuration(session.wallet_elapsed_seconds)}
        />
        <Row label="Sites ran" value={formatCount(session.sites_ran)} />
        <Row
          label="Avg site time"
          value={formatDuration(session.avg_site_seconds)}
        />
        <Row
          label="Avg combat→payout"
          value={formatDuration(joined.avgCombatToPayout)}
        />
        <Row
          label="Liquid ISK/hr"
          value={formatIskMoney(session.liquid_isk_per_hour)}
        />
        <Row
          label="LP value/hr"
          value={formatIskMoney(session.lp_value_per_hour)}
        />
        <Row label="Net/hr" value={formatIskMoney(session.net_per_hour)} />
        <Row label="Net LP" value={formatLp(session.net_lp)} />
        {session.lp_per_character_total != null && (
          <Row
            label="Per character LP"
            value={formatLp(session.lp_per_character_total)}
          />
        )}
        <Row label="ISK / LP" value={formatIskMoney(settings.isk_per_lp)} />
        <Row
          label="Liquid Value"
          value={formatIskMoney(session.fleet_liquid_isk)}
        />
        <Row label="LP value" value={formatIskMoney(session.lp_value)} />
        <Row label="Net value" value={formatIskMoney(session.net_value)} />
        <Row
          label="Character liquid"
          value={formatIskMoney(session.character_liquid_isk)}
        />
      </>
    ) : (
      <p className="text-muted">No aggregate for this scope.</p>
    )}
    {focus?.enrichment && (
      <>
        <div className="my-1 border-t border-border pt-1" />
        <h2 className="font-semibold">Gamelog enrichment</h2>
        <Row
          label="Resolved FC"
          value={focus.enrichment.resolved_fc ?? "—"}
        />
        <Row
          label="Approach"
          value={formatDuration(focus.enrichment.totals.approach_seconds)}
        />
        <Row
          label="Combat→payout"
          value={formatDuration(focus.enrichment.totals.combat_to_payout_seconds)}
        />
        <Row
          label="Avg combat→payout"
          value={formatDuration(focus.enrichment.totals.avg_combat_to_payout_seconds)}
        />
        <Row
          label="Expended"
          value={formatCount(fleetMissileSum.expended)}
        />
        <Row
          label="Hits"
          value={formatCount(fleetMissileSum.hits)}
        />
        <Row
          label="Dead missiles"
          value={formatCount(fleetMissileSum.dead_missiles)}
        />
        <Row
          label="Dead volleys/unused"
          value={formatCount(fleetMissileSum.dead_volleys)}
        />
        <Row
          label="Hit %"
          value={formatPercent(fleetMissileSum.hit_pct)}
        />
        <Row
          label="Miss %"
          value={formatPercent(fleetMissileSum.miss_pct)}
        />
        <p className="text-[10px] text-muted">
          Incomplete magazines can undercount dead missiles.
        </p>
      </>
    )}
  </aside>
</div>

{showEnrichLog &&
(focus?.enrichment || topDiagnostics.length > 0) ? (
  <div
    className="flex max-h-[50vh] min-h-48 shrink-0 flex-col overflow-hidden rounded border border-border"
  >
    <div className="sticky top-0 z-20 flex shrink-0 items-center justify-between border-b border-border bg-surface-raised px-3 py-2">
      <h3 className="text-xs font-semibold">
        Enrich log
        {focus?.enrichment
          ? ` · ${focus.enrichment.listeners.length} listeners`
          : ""}
        {topDiagnostics.length
          ? ` · ${topDiagnostics.length} messages`
          : ""}
      </h3>
      <button
        type="button"
        onClick={() => setShowEnrichLog(false)}
        className="rounded border border-border px-2 py-1 text-xs hover:border-accent/40 hover:text-accent"
      >
        Close
      </button>
    </div>
    <div className="min-h-0 flex-1 space-y-3 overflow-auto p-3 text-xs">
      {focus?.enrichment?.listeners?.length ? (
        <div>
          <p className="mb-1 font-semibold text-muted">
            Enriched listeners ({focus.enrichment.listeners.length})
          </p>
          <ul className="columns-2 gap-x-4 text-fg md:columns-3">
            {focus.enrichment.listeners.map((name) => (
              <li key={name} className="break-inside-avoid py-0.5">
                {name}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      {topDiagnostics.length > 0 ? (
        <div>
          <p className="mb-1 font-semibold text-muted">
            Diagnostics ({topDiagnostics.length})
          </p>
          <ul className="space-y-1 text-muted">
            {topDiagnostics.map((d, i) => (
              <li key={i} className="break-all">
                [{d.level}] {d.message}
              </li>
            ))}
          </ul>
        </div>
      ) : (
        <p className="text-muted">No enrichment diagnostics.</p>
      )}
    </div>
  </div>
) : null}

{showListeners && visibleMissiles.length ? (
  <div
    className="flex max-h-[50vh] min-h-48 shrink-0 flex-col overflow-hidden rounded border border-border"
  >
    <div className="sticky top-0 z-20 flex shrink-0 items-center justify-between border-b border-border bg-surface-raised px-3 py-2">
      <h3 className="text-xs font-semibold">
        Listeners ({visibleMissiles.length})
      </h3>
      <button
        type="button"
        onClick={() => setShowListeners(false)}
        className="rounded border border-border px-2 py-1 text-xs hover:border-accent/40 hover:text-accent"
      >
        Close
      </button>
    </div>
    <div className="min-h-0 flex-1 overflow-auto">
      <table className="w-full text-left text-xs">
        <thead className="sticky top-0 z-10 bg-surface text-muted">
          <tr>
            <th className="px-2 py-1">Listener</th>
            <th className="px-2 py-1">Reload cycles</th>
            <th className="px-2 py-1">Hits</th>
            <th className="px-2 py-1">Dead volleys/unused</th>
            <th className="px-2 py-1">Dead missiles</th>
            <th className="px-2 py-1">Hit %</th>
            <th className="px-2 py-1">Miss %</th>
          </tr>
        </thead>
        <tbody>
          {visibleMissiles.map((m) => {
            const r = of(m);
            return (
            <tr key={m.listener} className="border-t border-border">
              <td className="px-2 py-1">{m.listener}</td>
              <td className="px-2 py-1">{formatCount(m.reload_cycles)}</td>
              <td className="px-2 py-1">{formatCount(m.hits)}</td>
              <td className="px-2 py-1">{formatCount(r.dead_volleys)}</td>
              <td className="px-2 py-1">{formatCount(r.dead_missiles)}</td>
              <td className="px-2 py-1">{formatPercent(r.hit_pct)}</td>
              <td className="px-2 py-1">{formatPercent(r.miss_pct)}</td>
            </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  </div>
) : null}

{showDrilldown && focus?.report?.sites?.length ? (
  <div
    className="flex max-h-[50vh] min-h-48 shrink-0 flex-col overflow-hidden rounded border border-border"
  >
    <div className="sticky top-0 z-20 flex shrink-0 items-center justify-between border-b border-border bg-surface-raised px-3 py-2">
      <h3 className="text-xs font-semibold">
        {siteDrill == null
          ? `Per-site detail (${focus.report.sites.length})`
          : siteDrill.level === 1
            ? `Site totals · ${new Date(siteDrill.occurredAt).toLocaleString()}`
            : `Site listeners · ${new Date(siteDrill.occurredAt).toLocaleString()}`}
      </h3>
      <div className="flex items-center gap-2">
        {siteDrill?.level === 1 ? (
          <>
            <button
              type="button"
              onClick={() => setSiteDrill(null)}
              className="rounded border border-border px-2 py-1 text-xs hover:border-accent/40 hover:text-accent"
            >
              Back
            </button>
            <button
              type="button"
              onClick={() =>
                setSiteDrill({ ...siteDrill, level: 2 })
              }
              className="rounded border border-border px-2 py-1 text-xs hover:border-accent/40 hover:text-accent"
            >
              Listeners
            </button>
          </>
        ) : null}
        {siteDrill?.level === 2 ? (
          <button
            type="button"
            onClick={() =>
              setSiteDrill({ ...siteDrill, level: 1 })
            }
            className="rounded border border-border px-2 py-1 text-xs hover:border-accent/40 hover:text-accent"
          >
            Back
          </button>
        ) : null}
        <button
          type="button"
          onClick={() => {
            setShowDrilldown(false);
            setSiteDrill(null);
          }}
          className="rounded border border-border px-2 py-1 text-xs hover:border-accent/40 hover:text-accent"
        >
          Close
        </button>
      </div>
    </div>
    <div className="min-h-0 flex-1 overflow-auto">
      {siteDrill == null ? (
        <table className="w-full text-left text-xs">
          <thead className="sticky top-0 z-10 bg-surface text-muted">
            <tr>
              {(
                [
                  ["time", "Time"],
                  ["duration", "Duration"],
                  ["break", "Break?"],
                  ["approach", "Approach"],
                  ["combat", "Combat→payout"],
                  ["hitPct", "Hit %"],
                  ["missPct", "Miss %"],
                ] as const
              ).map(([key, label]) => {
                const active = siteSort.key === key;
                const marker = !active
                  ? ""
                  : siteSort.dir === "asc"
                    ? " ↑"
                    : " ↓";
                return (
                  <th key={key} className="px-2 py-1">
                    <button
                      type="button"
                      className="hover:text-accent"
                      onClick={() =>
                        setSiteSort((prev) =>
                          prev.key === key
                            ? {
                                key,
                                dir:
                                  prev.dir === "asc" ? "desc" : "asc",
                              }
                            : { key, dir: "asc" },
                        )
                      }
                    >
                      {label}
                      {marker}
                    </button>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {sortedSiteRows.map(
              ({ site: s, enrichment: e, hitPct, missPct, index }) => {
              const canDrill =
                focus.scope.kind === "run" &&
                !!e &&
                e.missiles.some(isActive);
              return (
                <tr
                  key={index}
                  className="border-t border-border"
                  onClick={
                    canDrill
                      ? () =>
                          setSiteDrill({
                            level: 1,
                            occurredAt: s.occurred_at,
                          })
                      : undefined
                  }
                  style={canDrill ? { cursor: "pointer" } : undefined}
                >
                  <td className="px-2 py-1">
                    {new Date(s.occurred_at).toLocaleString()}
                  </td>
                  <td className="px-2 py-1">
                    {formatDuration(s.duration_seconds)}
                  </td>
                  <td className="px-2 py-1">
                    {s.is_break ? "yes" : ""}
                  </td>
                  <td className="px-2 py-1">
                    {e ? formatDuration(e.approach_seconds) : "—"}
                  </td>
                  <td className="px-2 py-1">
                    {e
                      ? formatDuration(e.combat_to_payout_seconds)
                      : "—"}
                  </td>
                  <td className="px-2 py-1">
                    {formatPercent(hitPct)}
                  </td>
                  <td className="px-2 py-1">
                    {formatPercent(missPct)}
                  </td>
                </tr>
              );
            },
            )}
          </tbody>
        </table>
      ) : null}
      {siteDrill?.level === 1 ? (
        <table className="w-full text-left text-xs">
          <thead className="sticky top-0 z-10 bg-surface text-muted">
            <tr>
              <th className="px-2 py-1">Listener</th>
              <th className="px-2 py-1">Reload cycles</th>
              <th className="px-2 py-1">Hits</th>
              <th className="px-2 py-1">Dead volleys/unused</th>
              <th className="px-2 py-1">Dead missiles</th>
              <th className="px-2 py-1">Hit %</th>
              <th className="px-2 py-1">Miss %</th>
            </tr>
          </thead>
          <tbody>
            <tr className="border-t border-border font-medium">
              <td className="px-2 py-1">Total</td>
              <td className="px-2 py-1">
                {formatCount(siteDrillSum.reload_cycles)}
              </td>
              <td className="px-2 py-1">
                {formatCount(siteDrillSum.hits)}
              </td>
              <td className="px-2 py-1">
                {formatCount(siteDrillSum.dead_volleys)}
              </td>
              <td className="px-2 py-1">
                {formatCount(siteDrillSum.dead_missiles)}
              </td>
              <td className="px-2 py-1">
                {formatPercent(siteDrillSum.hit_pct)}
              </td>
              <td className="px-2 py-1">
                {formatPercent(siteDrillSum.miss_pct)}
              </td>
            </tr>
          </tbody>
        </table>
      ) : null}
      {siteDrill?.level === 2 ? (
        <table className="w-full text-left text-xs">
          <thead className="sticky top-0 z-10 bg-surface text-muted">
            <tr>
              <th className="px-2 py-1">Listener</th>
              <th className="px-2 py-1">Reload cycles</th>
              <th className="px-2 py-1">Hits</th>
              <th className="px-2 py-1">Dead volleys/unused</th>
              <th className="px-2 py-1">Dead missiles</th>
              <th className="px-2 py-1">Hit %</th>
              <th className="px-2 py-1">Miss %</th>
            </tr>
          </thead>
          <tbody>
            {siteDrillMissiles.map((m) => {
              const r = of(m);
              return (
              <tr
                key={m.listener}
                className="border-t border-border"
              >
                <td className="px-2 py-1">{m.listener}</td>
                <td className="px-2 py-1">
                  {formatCount(m.reload_cycles)}
                </td>
                <td className="px-2 py-1">{formatCount(m.hits)}</td>
                <td className="px-2 py-1">
                  {formatCount(r.dead_volleys)}
                </td>
                <td className="px-2 py-1">
                  {formatCount(r.dead_missiles)}
                </td>
                <td className="px-2 py-1">
                  {formatPercent(r.hit_pct)}
                </td>
                <td className="px-2 py-1">
                  {formatPercent(r.miss_pct)}
                </td>
              </tr>
              );
            })}
          </tbody>
          <tfoot>
            <tr className="sticky bottom-0 border-t border-border bg-surface-raised font-medium">
              <td className="px-2 py-1">Total</td>
              <td className="px-2 py-1">
                {formatCount(siteDrillSum.reload_cycles)}
              </td>
              <td className="px-2 py-1">
                {formatCount(siteDrillSum.hits)}
              </td>
              <td className="px-2 py-1">
                {formatCount(siteDrillSum.dead_volleys)}
              </td>
              <td className="px-2 py-1">
                {formatCount(siteDrillSum.dead_missiles)}
              </td>
              <td className="px-2 py-1">
                {formatPercent(siteDrillSum.hit_pct)}
              </td>
              <td className="px-2 py-1">
                {formatPercent(siteDrillSum.miss_pct)}
              </td>
            </tr>
          </tfoot>
        </table>
      ) : null}
    </div>
  </div>
) : null}
    </>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-2">
      <span className="text-muted">{label}</span>
      <span className="font-medium tabular-nums">{value}</span>
    </div>
  );
}
