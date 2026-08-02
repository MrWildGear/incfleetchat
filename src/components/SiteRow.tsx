import { cn, formatCountdown, formatPosted } from "../lib/utils";
import type { SitePhase, SiteRow } from "../lib/types";

type Props = {
  site: SiteRow;
  nowMs: number;
  onRan: () => void;
  onClear: () => void;
};

/** Derive phase client-side so Clear unlocks without waiting for a log rewrite. */
export function derivePhase(site: SiteRow, nowMs: number): {
  phase: SitePhase;
  clearable: boolean;
} {
  const expired = nowMs >= Date.parse(site.expires_at);
  if (expired && site.ran) return { phase: "ready", clearable: true };
  if (expired) return { phase: "overdue", clearable: false };
  return { phase: "active", clearable: false };
}

export function SiteRowView({ site, nowMs, onRan, onClear }: Props) {
  const { label, overdue } = formatCountdown(site.expires_at, nowMs);
  const { phase, clearable } = derivePhase(site, nowMs);

  return (
    <article
      className={cn(
        "border-b border-border px-3 py-3",
        phase === "overdue" && "bg-overdue/5",
        phase === "ready" && "bg-ready/5",
      )}
    >
      <header className="flex items-baseline justify-between gap-2 text-sm">
        <span className="font-medium text-fg truncate">{site.speaker}</span>
        <time className="text-xs text-muted shrink-0">
          {formatPosted(site.posted_at)}
        </time>
      </header>
      <p className="mt-1 text-2xl font-semibold tracking-wide text-fg">{site.tag}</p>
      <div className="mt-2 ml-3 border-l-2 border-border pl-3 flex items-center justify-between gap-2">
        <p
          className={cn(
            "text-sm tabular-nums",
            phase === "ready"
              ? "text-ready"
              : overdue || phase === "overdue"
                ? "text-overdue"
                : "text-accent",
          )}
        >
          {phase === "ready"
            ? `Ready to clear · ${label}`
            : overdue || phase === "overdue"
              ? label
              : `expires in ${label}`}
        </p>
        <div className="flex items-center gap-1.5 shrink-0">
          <button
            type="button"
            onClick={onRan}
            title="Mark ran"
            className={cn(
              "rounded-md border px-2 py-1 text-xs transition",
              site.ran
                ? "border-ready/40 bg-ready/20 text-ready"
                : "border-border bg-surface-raised text-muted hover:text-fg",
            )}
          >
            Ran
          </button>
          <button
            type="button"
            disabled={!clearable}
            onClick={onClear}
            className={cn(
              "rounded-md border px-2 py-1 text-xs transition",
              clearable
                ? "border-ready/40 text-ready hover:bg-ready/15"
                : "border-border text-muted/40 cursor-not-allowed",
            )}
          >
            Clear
          </button>
        </div>
      </div>
    </article>
  );
}
