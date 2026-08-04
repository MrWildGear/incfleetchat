import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SpaceBand } from "../lib/runDeskTypes";
import {
  getToolsSettings,
  setToolsSettings,
} from "../lib/toolsSettings";
import { cn } from "../lib/utils";
import { AmmoPlannerTab, type AmmoFit } from "./AmmoPlannerTab";
import { JoinedResultsView } from "./JoinedResultsView";
import { useRunDesk } from "./useRunDesk";

type Tab = "analytics" | "ammo";

export function ToolsApp() {
  const [tab, setTab] = useState<Tab>("analytics");
  const [manifestPaste, setManifestPaste] = useState("");
  const [walletPaste, setWalletPaste] = useState("");
  const [walletBuffer, setWalletBuffer] = useState("");
  const [gamelogsPath, setGamelogsPath] = useState("");
  const [fcCharacter, setFcCharacter] = useState("");
  const [ammoFit, setAmmoFit] = useState<AmmoFit>({
    launchers: 6,
    ammoPerLauncher: 26,
  });
  const walletRef = useRef<HTMLTextAreaElement>(null);

  const {
    focus,
    error,
    settings,
    enriching,
    scopeBusy,
    deleting,
    open,
    analyze,
    reenrich,
    pasteManifest,
    importWallet,
    setScope,
    setSessionSettings,
    setSpaceAndFleet,
    setConstellation,
    clearWalletTray,
    deleteRun,
    deleteSpawn,
    clearAll,
  } = useRunDesk();

  const enrichmentInputs = () => ({
    gamelogsDir: gamelogsPath,
    fcCharacter,
    ammoLaunchers: ammoFit.launchers,
    ammoPerLauncher: ammoFit.ammoPerLauncher,
  });

  useEffect(() => {
    void open();
    void getToolsSettings((cmd, args) =>
      args === undefined ? invoke(cmd) : invoke(cmd, args),
    ).then((s) => {
      setGamelogsPath(s.gamelogs_dir ?? "");
      setFcCharacter(s.fc_character ?? "");
      setAmmoFit({
        launchers: s.ammo_launchers,
        ammoPerLauncher: s.ammo_per_launcher,
      });
    });
  }, [open]);

  async function persistToolsSettings(patch: {
    gamelogs_dir?: string | null;
    fc_character?: string | null;
  }) {
    await setToolsSettings(
      (cmd, args) => (args === undefined ? invoke(cmd) : invoke(cmd, args)),
      patch,
    );
  }

  async function onImportWallet(replace: boolean) {
    if (replace) {
      setWalletBuffer(walletPaste);
    } else {
      setWalletBuffer((b) => (b ? `${b}\n${walletPaste}` : walletPaste));
    }
    const f = await importWallet(walletPaste, replace);
    if (!f) return;
    setWalletPaste("");
    requestAnimationFrame(() => {
      const el = walletRef.current;
      if (el) el.scrollTop = el.scrollHeight;
    });
  }

  async function onSetScope(scope: Parameters<typeof setScope>[0]) {
    await setScope(scope);
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
      await deleteRun(scope.run_id);
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
      await deleteSpawn(scope.constellation);
      return;
    }

    const n = catalog.runs.length;
    const m = catalog.spawns.length;
    if (n === 0 && m === 0) return;
    const ok = window.confirm(
      `Delete ALL analytics data (${n} runs, ${m} spawns)? This cannot be undone.`,
    );
    if (!ok) return;
    await clearAll();
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

  const resultsKey =
    focus?.scope.kind === "spawn"
      ? `spawn:${focus.scope.constellation}`
      : focus?.scope.kind === "run"
        ? `run:${focus.scope.run_id}`
        : "overall";

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
                  if (v === "overall") void onSetScope({ kind: "overall" });
                  else if (v.startsWith("spawn:"))
                    void onSetScope({
                      kind: "spawn",
                      constellation: v.slice(6),
                    });
                  else if (v.startsWith("run:"))
                    void onSetScope({ kind: "run", run_id: v.slice(4) });
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
                  void setSpaceAndFleet(
                    e.target.value as SpaceBand,
                    settings.fleet_size,
                  )
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
                  void setSpaceAndFleet(
                    settings.space,
                    Number(e.target.value) || 1,
                  )
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
                  void setSessionSettings({
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
                  void setSessionSettings({
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
                  void setSessionSettings({
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
                  void setSessionSettings({
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
                  void setSessionSettings({
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
                  void setConstellation(e.target.value);
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
                onBlur={() =>
                  void persistToolsSettings({
                    gamelogs_dir: gamelogsPath.trim() || null,
                  })
                }
                placeholder="Documents\EVE\logs\Gamelogs"
              />
            </label>
            <label className="text-xs text-muted">
              FC character
              <input
                className="ml-2 w-32 rounded border border-border bg-surface-raised px-2 py-1 text-fg"
                value={fcCharacter}
                onChange={(e) => setFcCharacter(e.target.value)}
                onBlur={() =>
                  void persistToolsSettings({
                    fc_character: fcCharacter.trim() || null,
                  })
                }
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
                onClick={() => void pasteManifest(manifestPaste)}
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
                  onClick={() => void onImportWallet(true)}
                  className="rounded border border-border px-2 py-1 text-xs hover:border-accent/40"
                >
                  Import
                </button>
                <button
                  type="button"
                  onClick={() => void onImportWallet(false)}
                  className="rounded border border-border px-2 py-1 text-xs hover:border-accent/40"
                >
                  Import more
                </button>
                <button
                  type="button"
                  onClick={() => {
                    void clearWalletTray()
                      .then(() => {
                        setWalletBuffer("");
                        setWalletPaste("");
                      })
                      .catch(() => {
                        /* error already on useRunDesk.error */
                      });
                  }}
                  className="rounded border border-border px-2 py-1 text-xs hover:border-overdue/40"
                >
                  Clear
                </button>
                <button
                  type="button"
                  onClick={() => void analyze(enrichmentInputs())}
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
          <JoinedResultsView
            key={resultsKey}
            focus={focus}
            settings={settings}
            enriching={enriching}
            onReenrich={() => void reenrich(enrichmentInputs())}
          />
        </div>
      ) : (
        <AmmoPlannerTab onAmmoFitChange={setAmmoFit} />
      )}
    </div>
  );
}
