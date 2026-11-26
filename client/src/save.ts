// R-UI-05 autosave: the command log (the deterministic core replays it) plus
// client-only state the log cannot restore — R-UI-03 facing of the models.
// Older saves were the bare command array; they still load (default facing).

import type { Facing } from "./facing";

export interface SaveData {
  /** Game.commands() JSON array. */
  commands: unknown[];
  /** Unit id → facing; units missing here face their side's default. */
  facing: Map<number, Facing>;
}

export function encodeSave(commands: string, facing: Map<number, Facing>): string {
  return JSON.stringify({ commands: JSON.parse(commands) as unknown[], facing: Object.fromEntries(facing) });
}

/** Null if the text is not a save at all. */
export function decodeSave(text: string): SaveData | null {
  let v: unknown;
  try {
    v = JSON.parse(text);
  } catch {
    return null;
  }
  if (Array.isArray(v)) return { commands: v, facing: new Map() };
  if (!v || typeof v !== "object") return null;
  const o = v as { commands?: unknown; facing?: unknown };
  if (!Array.isArray(o.commands)) return null;
  const facing = new Map<number, Facing>();
  if (o.facing && typeof o.facing === "object") {
    for (const [k, f] of Object.entries(o.facing)) {
      const id = Number(k);
      if (Number.isInteger(id) && (f === "left" || f === "right")) facing.set(id, f);
    }
  }
  return { commands: o.commands, facing };
}
