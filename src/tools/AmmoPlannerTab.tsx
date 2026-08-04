import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { computeAmmoLoad } from "../lib/ammo";
import { formatCount } from "../lib/formatAnalytics";
import {
  getToolsSettings,
  setToolsSettings,
} from "../lib/toolsSettings";

export type AmmoFit = {
  launchers: number;
  ammoPerLauncher: number;
};

type AmmoState = {
  ammoStock: number;
  launchers: number;
  ammoPerLauncher: number;
  shipCount: number;
  reloadPerSite: number;
};

const defaultAmmo: AmmoState = {
  ammoStock: 1_000_000,
  launchers: 6,
  ammoPerLauncher: 26,
  shipCount: 14,
  reloadPerSite: 2.2,
};

type AmmoPlannerTabProps = {
  onAmmoFitChange: (fit: AmmoFit) => void;
};

export function AmmoPlannerTab({ onAmmoFitChange }: AmmoPlannerTabProps) {
  const [ammo, setAmmo] = useState(defaultAmmo);
  const ammoResult = useMemo(() => computeAmmoLoad(ammo), [ammo]);

  useEffect(() => {
    void getToolsSettings((cmd, args) =>
      args === undefined ? invoke(cmd) : invoke(cmd, args),
    ).then((s) => {
      const fit = {
        launchers: s.ammo_launchers,
        ammoPerLauncher: s.ammo_per_launcher,
      };
      setAmmo((prev) => ({
        ...prev,
        launchers: fit.launchers,
        ammoPerLauncher: fit.ammoPerLauncher,
      }));
      onAmmoFitChange(fit);
    });
    // Load Tools settings once on mount; parent holds Enrichment-input fit.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentional mount-only
  }, []);

  async function persistAmmoFit(patch: {
    ammo_launchers?: number;
    ammo_per_launcher?: number;
  }) {
    const next = await setToolsSettings(
      (cmd, args) => (args === undefined ? invoke(cmd) : invoke(cmd, args)),
      patch,
    );
    onAmmoFitChange({
      launchers: next.ammo_launchers,
      ammoPerLauncher: next.ammo_per_launcher,
    });
  }

  async function copyLoad() {
    await navigator.clipboard.writeText(String(ammoResult.loadIntoShip));
  }

  return (
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
        onCommit={(n) => {
          setAmmo((prev) => ({ ...prev, launchers: n }));
          void persistAmmoFit({ ammo_launchers: n });
        }}
      />
      <AmmoField
        label="Ammo per launcher"
        value={ammo.ammoPerLauncher}
        onChange={(n) => setAmmo({ ...ammo, ammoPerLauncher: n })}
        onCommit={(n) => {
          setAmmo((prev) => ({ ...prev, ammoPerLauncher: n }));
          void persistAmmoFit({ ammo_per_launcher: n });
        }}
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
  onCommit,
  step = 1,
}: {
  label: string;
  value: number;
  onChange: (n: number) => void;
  onCommit?: (n: number) => void;
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
        onBlur={(e) => onCommit?.(Number(e.target.value) || 0)}
      />
    </label>
  );
}
