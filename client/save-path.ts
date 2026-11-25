// Dev-only editor saves: which files may be written, and where.

import { resolve, sep } from "node:path";

/** Files the editor may overwrite inside data/scenarios/<id>/. */
export const SAVABLE = ["map.json", "scenario.json"] as const;
const SCENARIO_ID = /^[a-z0-9_]{1,64}$/;

/** Absolute target path, or null if the request is not allowed. */
export function savePath(repo: string, scenario: string, file: string): string | null {
  if (!SCENARIO_ID.test(scenario) || !(SAVABLE as readonly string[]).includes(file)) return null;
  const base = resolve(repo, "data", "scenarios", scenario);
  const target = resolve(base, file);
  return target.startsWith(base + sep) ? target : null;
}
