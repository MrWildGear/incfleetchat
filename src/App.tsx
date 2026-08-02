import { useEffect, useMemo } from "react";
import { Pin, Settings, Wrench } from "lucide-react";
import { useAppStore } from "./store";
import { SiteRowView, derivePhase } from "./components/SiteRow";
import { SettingsPanel } from "./components/SettingsPanel";
import { cn } from "./lib/utils";
import type { BoardStatus, SiteRow } from "./lib/types";
import { invoke } from "@tauri-apps/api/core";
import type { Board } from "./lib/types";

function statusLabel(status: BoardStatus): string {
  switch (status.kind) {
    case "watching":
      return `${status.character} · ${status.log_name}`;
    case "no_character":
      return "Choose a character in Settings";
    case "waiting_for_log":
      return `Waiting for fleet log · ${status.character}`;
    case "error":
      return status.message;
  }
}

function App() {
  const board = useAppStore((s) => s.board);
  const settings = useAppStore((s) => s.settings);
  const nowMs = useAppStore((s) => s.nowMs);
  const hydrate = useAppStore((s) => s.hydrate);
  const tick = useAppStore((s) => s.tick);
  const markRan = useAppStore((s) => s.markRan);
  const clearSite = useAppStore((s) => s.clearSite);
  const clearReady = useAppStore((s) => s.clearReady);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);
  const setAlwaysOnTop = useAppStore((s) => s.setAlwaysOnTop);

  useEffect(() => {
    void hydrate();
  }, [hydrate]);

  useEffect(() => {
    const id = window.setInterval(() => tick(), 1000);
    return () => window.clearInterval(id);
  }, [tick]);

  // Keep server phases in sync occasionally so Clear ready count stays honest
  useEffect(() => {
    const id = window.setInterval(() => {
      void invoke<Board>("refresh_board")
        .then((board) => useAppStore.getState().setBoard(board))
        .catch(() => {});
    }, 5000);
    return () => window.clearInterval(id);
  }, []);

  const readyCount = useMemo(
    () => board.sites.filter((s) => derivePhase(s, nowMs).clearable).length,
    [board.sites, nowMs],
  );

  return (
    <div className="flex h-full flex-col bg-surface text-fg">
      <header className="flex items-center gap-2 border-b border-border px-3 py-2">
        <div className="min-w-0 flex-1">
          <h1 className="text-sm font-semibold tracking-tight">IncFleetChat</h1>
          <p className="truncate text-xs text-muted">{statusLabel(board.status)}</p>
        </div>
        <button
          type="button"
          title="Pin on top"
          onClick={() => void setAlwaysOnTop(!(settings?.always_on_top ?? false))}
          className={cn(
            "rounded-md border p-1.5",
            settings?.always_on_top
              ? "border-accent/40 text-accent"
              : "border-border text-muted hover:text-fg",
          )}
        >
          <Pin className="h-4 w-4" />
        </button>
        <button
          type="button"
          title="Tools"
          onClick={() => void invoke("open_tools_window")}
          className="rounded-md border border-border p-1.5 text-muted hover:text-fg"
        >
          <Wrench className="h-4 w-4" />
        </button>
        <button
          type="button"
          title="Settings"
          onClick={() => setSettingsOpen(true)}
          className="rounded-md border border-border p-1.5 text-muted hover:text-fg"
        >
          <Settings className="h-4 w-4" />
        </button>
        <button
          type="button"
          disabled={readyCount === 0}
          onClick={() => void clearReady()}
          className={cn(
            "rounded-md border px-2 py-1 text-xs",
            readyCount > 0
              ? "border-ready/40 text-ready hover:bg-ready/10"
              : "border-border text-muted/40 cursor-not-allowed",
          )}
        >
          Clear ready{readyCount > 0 ? ` (${readyCount})` : ""}
        </button>
      </header>

      <main className="flex-1 overflow-y-auto">
        {board.sites.length === 0 ? (
          <p className="px-4 py-8 text-center text-sm text-muted">
            No site tags yet. Exact single-character posts (0–9, a–z) will appear here.
          </p>
        ) : (
          board.sites.map((site: SiteRow) => (
            <SiteRowView
              key={site.id}
              site={site}
              nowMs={nowMs}
              onRan={() => void markRan(site.id)}
              onClear={() => void clearSite(site.id)}
            />
          ))
        )}
      </main>

      <SettingsPanel />
    </div>
  );
}

export default App;
