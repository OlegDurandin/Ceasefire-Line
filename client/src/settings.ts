// Client settings remembered per browser (gear button): unit pictures
// (R-UI-03) and how the AI's phase is shown (R-UI-05: speed of the AI turn).

export type AiSpeed = "x1" | "x2" | "x4" | "instant";
export const AI_SPEEDS: AiSpeed[] = ["x1", "x2", "x4", "instant"];

export interface Settings {
  units?: string;
  aiSpeed?: AiSpeed;
}

/** Pause before each AI command and how long its combat result stays, ms. */
export interface AiTiming {
  stepMs: number;
  resultMs: number;
  /** The camera follows the action (not at «instant»). */
  follow: boolean;
}

const BASE_STEP_MS = 600;
const BASE_RESULT_MS = 1100;

export function aiTiming(speed: AiSpeed): AiTiming {
  if (speed === "instant") return { stepMs: 0, resultMs: 0, follow: false };
  const k = speed === "x4" ? 4 : speed === "x2" ? 2 : 1;
  return { stepMs: Math.round(BASE_STEP_MS / k), resultMs: Math.round(BASE_RESULT_MS / k), follow: true };
}

export function parseSettings(text: string | null): Settings {
  try {
    const v = JSON.parse(text ?? "{}") as Record<string, unknown>;
    const out: Settings = {};
    if (typeof v.units === "string") out.units = v.units;
    if (AI_SPEEDS.includes(v.aiSpeed as AiSpeed)) out.aiSpeed = v.aiSpeed as AiSpeed;
    return out;
  } catch {
    return {};
  }
}

/** Reads, changes one field and writes back (other fields are kept). */
export function storeSetting(key: string, patch: Settings) {
  try {
    const now = parseSettings(localStorage.getItem(key));
    localStorage.setItem(key, JSON.stringify({ ...now, ...patch }));
  } catch {
    // no storage: the choice lasts for this session
  }
}
