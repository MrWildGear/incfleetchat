import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { computeAmmoLoad } from "../lib/ammo";
import { formatCount, formatIskMoney, formatLp } from "../lib/formatAnalytics";
import type {
  EditionFocus,
  EnrichmentSite,
  PayoutTicket,
  ReportScope,
  RunSettings,
  SpaceBand,
} from "../lib/analyticsTypes";
import { hasMissileActivity } from "../lib/missileActivity";
import { cn } from "../lib/utils";

type ToolsSettings = {
  gamelogs_dir: string | null;
  fc_character: string | null;
};

type Tab = "analytics" | "ammo";

function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds)) return "—";
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  return `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
}

const defaultAmmo = {
  ammoStock: 1_000_000,
  launchers: 6,
  ammoPerLauncher: 26,
  shipCount: 14,
  reloadPerSite: 2.2,
};

export function ToolsApp() {
  const [tab, setTab] = useState<Tab>("analytics");
  const [focus, setFocus] = useState<EditionFocus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [manifestPaste, setManifestPaste] = useState("");
  const [walletPaste, setWalletPaste] = useState("");
  const [walletBuffer, setWalletBuffer] = useState("");
  const [showDrilldown, setShowDrilldown] = useState(false);
  const [showListeners, setShowListeners] = useState(false);
  const [showEnrichLog, setShowEnrichLog] = useState(false);
  const [gamelogsPath, setGamelogsPath] = useState("");
  const [fcCharacter, setFcCharacter] = useState("");
  const [enriching, setEnriching] = useState(false);
  const [scopeBusy, setScopeBusy] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const walletRef = useRef<HTMLTextAreaElement>(null);

  const [settings, setSettings] = useState<RunSettings>({
    space: "low_null",
    fleet_size: 15,
    expected_isk: 15_000_000,
    lp_per_char: 2_000,
    isk_per_lp: 1400,
    break_threshold_minutes: 25,
    run_start: null,
  });

  const [ammo, setAmmo] = useState(() => {
    try {
      const raw = localStorage.getItem("incfleetchat.ammo");
      return raw ? { ...defaultAmmo, ...JSON.parse(raw) } : defaultAmmo;
    } catch {
      return defaultAmmo;
    }
  });

  const ammoResult = useMemo(() => computeAmmoLoad(ammo), [ammo]);

  useEffect(() => {
    localStorage.setItem("incfleetchat.ammo", JSON.stringify(ammo));
  }, [ammo]);

  const applyFocus = useCallback((f: EditionFocus) => {
    setFocus(f);
    setSettings(f.session_settings);
    setError(null);
  }, []);

  useEffect(() => {
    void invoke<EditionFocus>("run_desk_open")
      .then(applyFocus)
      .catch((e: unknown) => setError(String(e)));
    void invoke<ToolsSettings>("get_settings").then((s) => {
      setGamelogsPath(s.gamelogs_dir ?? "");
      setFcCharacter(s.fc_character ?? "");
    });
  }, [applyFocus]);

  async function persistToolsSettings() {
    try {
      await invoke("set_settings", {
        patch: {
          gamelogs_dir: gamelogsPath.trim() || null,
          fc_character: fcCharacter.trim() || null,
        },
      });
    } catch (e) {
      setError(String(e));
      throw e;
    }
  }

  async function syncAmmoSettings() {
    await invoke("set_settings", {
      patch: {
        ammo_launchers: ammo.launchers,
        ammo_per_launcher: ammo.ammoPerLauncher,
      },
    });
  }

  async function reenrich() {
    try {
      setEnriching(true);
      await persistToolsSettings();
      await syncAmmoSettings();
      const runId = focus?.scope.kind === "run" ? focus.scope.run_id : undefined;
      const f = await invoke<EditionFocus>("run_desk_reenrich", { runId });
      applyFocus(f);
    } catch (e) {
      setError(String(e));
    } finally {
      setEnriching(false);
    }
  }

  async function syncSettings(next: RunSettings) {
    setSettings(next);
    const f = await invoke<EditionFocus>("run_desk_amend", {
      op: { op: "set_session_settings", settings: next },
    });
    applyFocus(f);
  }

  async function onSpaceOrFleet(space: SpaceBand, fleet_size: number) {
    const ticket = await invoke<PayoutTicket>("lookup_vanguard_payout", {
      space,
      fleetSize: fleet_size,
    });
    await syncSettings({
      ...settings,
      space,
      fleet_size,
      expected_isk: ticket.isk,
      lp_per_char: ticket.lp_per_char,
    });
  }

  async function pasteManifest() {
    try {
      const f = await invoke<EditionFocus>("run_desk_paste", {
        tray: "manifest",
        text: manifestPaste,
      });
      applyFocus(f);
    } catch (e) {
      setError(String(e));
    }
  }

  async function importWallet(replace: boolean) {
    try {
      if (replace) {
        await invoke("run_desk_amend", { op: { op: "clear_wallet_tray" } });
        setWalletBuffer(walletPaste);
      } else {
        setWalletBuffer((b) => (b ? `${b}\n${walletPaste}` : walletPaste));
      }
      const f = await invoke<EditionFocus>("run_desk_paste", {
        tray: "wallet",
        text: walletPaste,
      });
      applyFocus(f);
      setWalletPaste("");
      requestAnimationFrame(() => {
        const el = walletRef.current;
        if (el) el.scrollTop = el.scrollHeight;
      });
    } catch (e) {
      setError(String(e));
    }
  }

  async function analyze() {
    try {
      setEnriching(true);
      await syncSettings(settings);
      await syncAmmoSettings();
      const f = await invoke<EditionFocus>("run_desk_analyze");
      applyFocus(f);
    } catch (e) {
      setError(String(e));
    } finally {
      setEnriching(false);
    }
  }

  async function setScope(scope: ReportScope) {
    try {
      setScopeBusy(true);
      const f = await invoke<EditionFocus>("run_desk_focus", { scope });
      applyFocus(f);
    } catch (e) {
      setError(String(e));
    } finally {
      setScopeBusy(false);
    }
  }

  async function onDeleteScope() {
    if (!focus || scopeBusy || deleting) return;
    const catalog = focus.catalog;
    const scope = focus.scope;

    if (scope.kind === "run") {
      const ok = window.confirm(
        `Delete run ${scope.run_id}? This cannot be undone.`,
      );
      if (!ok) return;
      try {
        setDeleting(true);
        const f = await invoke<EditionFocus>("run_desk_delete_run", {
          runId: scope.run_id,
        });
        applyFocus(f);
      } catch (e) {
        setError(String(e));
      } finally {
        setDeleting(false);
      }
      return;
    }

    if (scope.kind === "spawn") {
      const n =
        catalog.spawns.find((s) => s.constellation === scope.constellation)
          ?.run_count ??
        catalog.runs.filter((r) => r.constellation === scope.constellation)
          .length;
      const ok = window.confirm(
        `Delete spawn ${scope.constellation} and ${n} runs? This cannot be undone.`,
      );
      if (!ok) return;
      try {
        setDeleting(true);
        const f = await invoke<EditionFocus>("run_desk_delete_spawn", {
          constellation: scope.constellation,
        });
        applyFocus(f);
      } catch (e) {
        setError(String(e));
      } finally {
        setDeleting(false);
      }
      return;
    }

    const n = catalog.runs.length;
    const m = catalog.spawns.length;
    if (n === 0 && m === 0) return;
    const ok = window.confirm(
      `Delete ALL analytics data (${n} runs, ${m} spawns)? This cannot be undone.`,
    );
    if (!ok) return;
    try {
      setDeleting(true);
      const f = await invoke<EditionFocus>("run_desk_clear_all");
      applyFocus(f);
    } catch (e) {
      setError(String(e));
    } finally {
      setDeleting(false);
    }
  }

  const deleteDisabled =
    !focus ||
    scopeBusy ||
    deleting ||
    (focus.scope.kind === "overall" &&
      focus.catalog.runs.length === 0 &&
      focus.catalog.spawns.length === 0);

  const deleteLabel =
    focus?.scope.kind === "run"
      ? "Delete run…"
      : focus?.scope.kind === "spawn"
        ? "Delete spawn…"
        : "Clear all analytics…";

  async function copyLoad() {
    await navigator.clipboard.writeText(String(ammoResult.loadIntoShip));
  }

  const session = focus?.report?.session;
  const canReenrich =
    focus?.scope.kind === "run" ||
    (focus?.scope.kind === "spawn" && (focus.spawn?.run_count ?? 0) > 0) ||
    (focus?.scope.kind === "overall" && (focus.catalog?.runs?.length ?? 0) > 0);
  const visibleMissiles =
    focus?.enrichment?.missiles?.filter(hasMissileActivity) ?? [];
  const enrichmentByTime = useMemo(() => {
    const map = new Map<string, EnrichmentSite>();
    for (const s of focus?.enrichment?.sites ?? []) map.set(s.occurred_at, s);
    return map;
  }, [focus?.enrichment]);
  /** Enrichment + focus diagnostics for the Enrich log drill-down (not Summary). */
  const topDiagnostics = useMemo(() => {
    const generic = focus?.diagnostics ?? [];
    const enrichmentOnly = focus?.enrichment?.diagnostics ?? [];
    return [...generic, ...enrichmentOnly];
  }, [focus?.diagnostics, focus?.enrichment?.diagnostics]);

  return (
    <div className="flex h-full flex-col bg-surface text-fg">
      <header className="flex items-center gap-2 border-b border-border px-3 py-2">
        <h1 className="text-sm font-semibold tracking-tight">Tools</h1>
        <div className="ml-4 flex gap-1">
          {(["analytics", "ammo"] as const).map((t) => (
            <button
              key={t}
              type="button"
              onClick={() => setTab(t)}
              className={cn(
                "rounded-md px-3 py-1 text-xs capitalize",
                tab === t
                  ? "bg-accent/20 text-accent"
                  : "text-muted hover:text-fg",
              )}
            >
              {t}
            </button>
          ))}
        </div>
      </header>

      {error && (
        <p className="border-b border-overdue/40 bg-overdue/10 px-3 py-2 text-xs text-overdue">
          {error}
        </p>
      )}

      {tab === "analytics" ? (
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-3">
          <div className="flex flex-wrap items-end gap-2">
            <label className="text-xs text-muted">
              Scope
              <select
                className="ml-2 rounded border border-border bg-surface-raised px-2 py-1 text-fg"
                disabled={scopeBusy}
                value={
                  focus?.scope.kind === "spawn"
                    ? `spawn:${focus.scope.constellation}`
                    : focus?.scope.kind === "run"
                      ? `run:${focus.scope.run_id}`
                      : "overall"
                }
                onChange={(e) => {
                  const v = e.target.value;
                  if (v === "overall") void setScope({ kind: "overall" });
                  else if (v.startsWith("spawn:"))
                    void setScope({
                      kind: "spawn",
                      constellation: v.slice(6),
                    });
                  else if (v.startsWith("run:"))
                    void setScope({ kind: "run", run_id: v.slice(4) });
                }}
              >
                <option value="overall">Overall</option>
                {(focus?.catalog.spawns ?? []).map((s) => (
                  <option key={s.constellation} value={`spawn:${s.constellation}`}>
                    Spawn {s.constellation} ({s.run_count})
                  </option>
                ))}
                {(focus?.catalog.runs ?? []).map((r) => (
                  <option key={r.run_id} value={`run:${r.run_id}`}>
                    Run {r.run_id}
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              disabled={deleteDisabled}
              className="rounded border border-border bg-surface-raised px-2 py-1 text-xs text-fg disabled:opacity-40"
              onClick={() => void onDeleteScope()}
            >
              {deleteLabel}
            </button>
            <label className="text-xs text-muted">
              Space
              <select
                className="ml-2 rounded border border-border bg-surface-raised px-2 py-1 text-fg"
                value={settings.space}
                onChange={(e) =>
                  void onSpaceOrFleet(e.target.value as SpaceBand, settings.fleet_size)
                }
              >
                <option value="low_null">Low / Null</option>
                <option value="highsec">Highsec</option>
              </select>
            </label>
            <label className="text-xs text-muted">
              Fleet size
              <input
                type="number"
                min={1}
                className="ml-2 w-16 rounded border border-border bg-surface-raised px-2 py-1 text-fg"
                value={settings.fleet_size}
                onChange={(e) =>
                  void onSpaceOrFleet(settings.space, Number(e.target.value) || 1)
                }
              />
            </label>
            <label className="text-xs text-muted">
              Expected ISK
              <input
                type="number"
                className="ml-2 w-28 rounded border border-border bg-surface-raised px-2 py-1 text-fg"
                value={settings.expected_isk}
                onChange={(e) =>
                  void syncSettings({
                    ...settings,
                    expected_isk: Number(e.target.value) || 0,
                  })
                }
              />
            </label>
            <label className="text-xs text-muted">
              LP / char
              <input
                type="number"
                className="ml-2 w-20 rounded border border-border bg-surface-raised px-2 py-1 text-fg"
                value={settings.lp_per_char}
                onChange={(e) =>
                  void syncSettings({
                    ...settings,
                    lp_per_char: Number(e.target.value) || 0,
                  })
                }
              />
            </label>
            <label className="text-xs text-muted">
              ISK / LP
              <input
                type="number"
                className="ml-2 w-20 rounded border border-border bg-surface-raised px-2 py-1 text-fg"
                value={settings.isk_per_lp}
                onChange={(e) =>
                  void syncSettings({
                    ...settings,
                    isk_per_lp: Number(e.target.value) || 0,
                  })
                }
              />
            </label>
            <label className="text-xs text-muted">
              Break (min)
              <input
                type="number"
                className="ml-2 w-16 rounded border border-border bg-surface-raised px-2 py-1 text-fg"
                value={settings.break_threshold_minutes}
                onChange={(e) =>
                  void syncSettings({
                    ...settings,
                    break_threshold_minutes: Number(e.target.value) || 25,
                  })
                }
              />
            </label>
            <label className="text-xs text-muted">
              Run start (UTC)
              <input
                type="datetime-local"
                className="ml-2 rounded border border-border bg-surface-raised px-2 py-1 text-fg"
                value={
                  settings.run_start
                    ? settings.run_start.slice(0, 16)
                    : ""
                }
                onChange={(e) => {
                  const v = e.target.value;
                  void syncSettings({
                    ...settings,
                    run_start: v ? `${v}:00Z` : null,
                  });
                }}
              />
            </label>
            <label className="text-xs text-muted">
              Constellation
              <input
                className="ml-2 w-24 rounded border border-border bg-surface-raised px-2 py-1 text-fg"
                value={focus?.staging_spawn?.constellation ?? ""}
                onChange={(e) => {
                  void invoke<EditionFocus>("run_desk_amend", {
                    op: {
                      op: "set_constellation",
                      constellation: e.target.value,
                    },
                  }).then(applyFocus);
                }}
                placeholder="4MY-AB"
              />
            </label>
            <label className="text-xs text-muted">
              Gamelogs path
              <input
                className="ml-2 w-56 rounded border border-border bg-surface-raised px-2 py-1 text-fg"
                value={gamelogsPath}
                onChange={(e) => setGamelogsPath(e.target.value)}
                onBlur={() => void persistToolsSettings()}
                placeholder="Documents\EVE\logs\Gamelogs"
              />
            </label>
            <label className="text-xs text-muted">
              FC character
              <input
                className="ml-2 w-32 rounded border border-border bg-surface-raised px-2 py-1 text-fg"
                value={fcCharacter}
                onChange={(e) => setFcCharacter(e.target.value)}
                onBlur={() => void persistToolsSettings()}
                placeholder="FC Pilot"
              />
            </label>
          </div>

          <div className="grid gap-3 md:grid-cols-2">
            <div>
              <p className="mb-1 text-xs text-muted">INC Manifest paste</p>
              <textarea
                className="h-28 w-full rounded border border-border bg-surface-raised p-2 font-mono text-xs"
                value={manifestPaste}
                onChange={(e) => setManifestPaste(e.target.value)}
                placeholder="Paste Kundalini Manifest notice…"
              />
              <button
                type="button"
                onClick={() => void pasteManifest()}
                className="mt-1 rounded border border-border px-2 py-1 text-xs hover:border-accent/40"
              >
                Apply Manifest
              </button>
              {focus?.staging_spawn?.constellation && (
                <p className="mt-1 text-xs text-accent">
                  Spawn {focus.staging_spawn.constellation}
                  {focus.staging_spawn.region
                    ? ` · ${focus.staging_spawn.region}`
                    : ""}
                </p>
              )}
            </div>
            <div>
              <p className="mb-1 text-xs text-muted">
                Wallet journal (pending sites: {focus?.trays.pending_sites ?? 0})
              </p>
              <textarea
                ref={walletRef}
                className="h-28 w-full rounded border border-border bg-surface-raised p-2 font-mono text-xs"
                value={walletPaste}
                onChange={(e) => setWalletPaste(e.target.value)}
                placeholder="Paste Corporate Reward Payout lines…"
              />
              <div className="mt-1 flex gap-2">
                <button
                  type="button"
                  onClick={() => void importWallet(true)}
                  className="rounded border border-border px-2 py-1 text-xs hover:border-accent/40"
                >
                  Import
                </button>
                <button
                  type="button"
                  onClick={() => void importWallet(false)}
                  className="rounded border border-border px-2 py-1 text-xs hover:border-accent/40"
                >
                  Import more
                </button>
                <button
                  type="button"
                  onClick={() => {
                    void invoke<EditionFocus>("run_desk_amend", {
                      op: { op: "clear_wallet_tray" },
                    }).then((f) => {
                      applyFocus(f);
                      setWalletBuffer("");
                      setWalletPaste("");
                    });
                  }}
                  className="rounded border border-border px-2 py-1 text-xs hover:border-overdue/40"
                >
                  Clear
                </button>
                <button
                  type="button"
                  onClick={() => void analyze()}
                  className="rounded border border-accent/40 bg-accent/10 px-2 py-1 text-xs text-accent"
                >
                  Analyze
                </button>
              </div>
              {walletBuffer && (
                <p className="mt-1 text-[10px] text-muted">
                  Buffered {walletBuffer.split("\n").filter(Boolean).length} lines
                  across {focus?.trays.wallet_batches ?? 0} batch(es)
                </p>
              )}
            </div>
          </div>

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
                onClick={() => void reenrich()}
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
                onClick={() => setShowDrilldown((v) => !v)}
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
                  </tr>
                </thead>
                <tbody>
                  {(focus?.report?.hourly ?? []).map((h) => (
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
                    </tr>
                  ))}
                  {!focus?.report?.hourly?.length && (
                    <tr>
                      <td colSpan={5} className="px-2 py-6 text-center text-muted">
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
                    label="Dead missiles"
                    value={formatCount(focus.enrichment.totals.fleet_dead)}
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
                      <th className="px-2 py-1">Missiles/cycle</th>
                      <th className="px-2 py-1">Dead</th>
                    </tr>
                  </thead>
                  <tbody>
                    {visibleMissiles.map((m) => (
                      <tr key={m.listener} className="border-t border-border">
                        <td className="px-2 py-1">{m.listener}</td>
                        <td className="px-2 py-1">{formatCount(m.reload_cycles)}</td>
                        <td className="px-2 py-1">{formatCount(m.hits)}</td>
                        <td className="px-2 py-1">
                          {formatCount(m.missiles_per_cycle)}
                        </td>
                        <td className="px-2 py-1">{formatCount(m.dead)}</td>
                      </tr>
                    ))}
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
                  Per-site detail ({focus.report.sites.length})
                </h3>
                <button
                  type="button"
                  onClick={() => setShowDrilldown(false)}
                  className="rounded border border-border px-2 py-1 text-xs hover:border-accent/40 hover:text-accent"
                >
                  Close
                </button>
              </div>
              <div className="min-h-0 flex-1 overflow-auto">
                <table className="w-full text-left text-xs">
                  <thead className="sticky top-0 z-10 bg-surface text-muted">
                    <tr>
                      <th className="px-2 py-1">Time</th>
                      <th className="px-2 py-1">Duration</th>
                      <th className="px-2 py-1">Break?</th>
                      <th className="px-2 py-1">ISK</th>
                      <th className="px-2 py-1">LP</th>
                      <th className="px-2 py-1">Approach</th>
                      <th className="px-2 py-1">Combat→payout</th>
                      <th className="px-2 py-1">Source</th>
                    </tr>
                  </thead>
                  <tbody>
                    {focus.report.sites.map((s, i) => {
                      const e = enrichmentByTime.get(s.occurred_at);
                      return (
                        <tr key={i} className="border-t border-border">
                          <td className="px-2 py-1">
                            {new Date(s.occurred_at).toLocaleString()}
                          </td>
                          <td className="px-2 py-1">
                            {formatDuration(s.duration_seconds)}
                          </td>
                          <td className="px-2 py-1">{s.is_break ? "yes" : ""}</td>
                          <td className="px-2 py-1">{formatIskMoney(s.amount_isk)}</td>
                          <td className="px-2 py-1">{formatLp(s.fleet_lp)}</td>
                          <td className="px-2 py-1">
                            {e ? formatDuration(e.approach_seconds) : "—"}
                          </td>
                          <td className="px-2 py-1">
                            {e ? formatDuration(e.combat_to_payout_seconds) : "—"}
                          </td>
                          <td className="px-2 py-1">{e ? e.source : "—"}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </div>
          ) : null}
        </div>
      ) : (
        <div className="mx-auto w-full max-w-md space-y-3 p-4">
          <AmmoField
            label="Ammo stock"
            value={ammo.ammoStock}
            onChange={(n) => setAmmo({ ...ammo, ammoStock: n })}
          />
          <AmmoField
            label="Launchers"
            value={ammo.launchers}
            onChange={(n) => setAmmo({ ...ammo, launchers: n })}
          />
          <AmmoField
            label="Ammo per launcher"
            value={ammo.ammoPerLauncher}
            onChange={(n) => setAmmo({ ...ammo, ammoPerLauncher: n })}
          />
          <Row
            label="Missiles per cycle"
            value={formatCount(ammoResult.missilesPerCycle)}
          />
          <AmmoField
            label="Ship count"
            value={ammo.shipCount}
            onChange={(n) => setAmmo({ ...ammo, shipCount: n })}
          />
          <div className="flex items-center justify-between rounded border border-border bg-surface-raised px-3 py-2">
            <div>
              <p className="text-xs text-muted">Load into ship</p>
              <p className="text-lg font-semibold">
                {formatCount(ammoResult.loadIntoShip)}
              </p>
            </div>
            <button
              type="button"
              onClick={() => void copyLoad()}
              className="rounded border border-accent/40 bg-accent/10 px-3 py-1.5 text-xs text-accent"
            >
              Copy
            </button>
          </div>
          <AmmoField
            label="Reload per site"
            value={ammo.reloadPerSite}
            step={0.1}
            onChange={(n) => setAmmo({ ...ammo, reloadPerSite: n })}
          />
          <Row label="Sites capacity" value={ammoResult.sites.toFixed(1)} />
        </div>
      )}
    </div>
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

function AmmoField({
  label,
  value,
  onChange,
  step = 1,
}: {
  label: string;
  value: number;
  onChange: (n: number) => void;
  step?: number;
}) {
  return (
    <label className="flex items-center justify-between gap-3 text-xs">
      <span className="text-muted">{label}</span>
      <input
        type="number"
        step={step}
        className="w-36 rounded border border-border bg-surface-raised px-2 py-1 text-right text-fg"
        value={value}
        onChange={(e) => onChange(Number(e.target.value) || 0)}
      />
    </label>
  );
}
