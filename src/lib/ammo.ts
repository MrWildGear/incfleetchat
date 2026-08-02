export type AmmoInput = {
  ammoStock: number;
  launchers: number;
  ammoPerLauncher: number;
  shipCount: number;
  reloadPerSite: number;
};

export type AmmoResult = {
  missilesPerCycle: number;
  loadIntoShip: number;
  sites: number;
};

/** Plan missile load per ship from stock and fit. */
export function computeAmmoLoad(input: AmmoInput): AmmoResult {
  const launchers = Math.max(0, input.launchers);
  const ammoPerLauncher = Math.max(0, input.ammoPerLauncher);
  const shipCount = Math.max(1, input.shipCount);
  const stock = Math.max(0, input.ammoStock);
  const reloadPerSite = Math.max(0, input.reloadPerSite);

  const missilesPerCycle = launchers * ammoPerLauncher;
  const loadIntoShip = Math.ceil(stock / shipCount);
  const denom = missilesPerCycle * reloadPerSite;
  const sites = denom > 0 ? loadIntoShip / denom : 0;

  return { missilesPerCycle, loadIntoShip, sites };
}
