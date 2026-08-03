import type { MissileStat } from "./analyticsTypes";

export function hasMissileActivity(
  m: Pick<MissileStat, "reload_cycles" | "hits" | "dead">,
): boolean {
  return m.reload_cycles !== 0 || m.hits !== 0 || m.dead !== 0;
}
