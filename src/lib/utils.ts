import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatCountdown(expiresAt: string, nowMs: number): {
  label: string;
  overdue: boolean;
} {
  const exp = Date.parse(expiresAt);
  const diff = exp - nowMs;
  if (diff >= 0) {
    const totalSec = Math.floor(diff / 1000);
    const m = Math.floor(totalSec / 60);
    const s = totalSec % 60;
    return {
      label: `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`,
      overdue: false,
    };
  }
  const past = Math.floor(-diff / 1000);
  const m = Math.floor(past / 60);
  const s = past % 60;
  return {
    label: `+${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")} past`,
    overdue: true,
  };
}

export function formatPosted(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}
