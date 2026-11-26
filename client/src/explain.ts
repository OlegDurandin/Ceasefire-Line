// R-UI-04: «у каждого числа тултип с причиной» — the formulas behind the
// numbers, built from the core's own inputs (Game.explain, unit readiness
// basis) and the coefficients in data/rules.json. Nothing is recomputed
// differently from the core: the results shown are the core's.

type Locale = Record<string, string>;

export interface Rules {
  combat: {
    p_base: number;
    p_slope: number;
    p_min: number;
    p_max: number;
    exp_attack_bonus: number;
    exp_defense_bonus: number;
    entrench_defense_bonus: number;
  };
  readiness: { base: number; per_supply: number; exp_bonus: number; max: number; fatigue_per_attack_pct: number; fatigue_per_attacked_pct: number };
  supply: { low_supply_max: number; entrench_min_supply: number };
  movement: { night_ap_penalty: number; low_supply_ap_penalty: number };
}

/** Game.explain(): FireInput of the core (R-COMBAT-02). */
export interface FireInput {
  strength: number;
  readiness_pct: number;
  shot_fraction: number;
  attack: number;
  shooter_exp: number;
  attack_mult: number;
  defense: number;
  target_exp: number;
  terrain_mult: number;
  entrenchment: number;
  defense_mult: number;
  p_shift: number;
}

/** Game.explain(): what the rule variants add to an attack (SC alignment). */
export interface AttackModifiers {
  /** R-COMBAT-07: readiness points of a prepared attack (0: none). */
  prepared_pct: number;
  /** R-COMBAT-05: × A against a target without supply (1: none). */
  unsupplied_target_mult: number;
  /** R-COMBAT-02: entrenchment levels the attack strips. */
  entrench_strip: number;
}

export interface FireParams {
  n: number;
  a: number;
  d: number;
  p: number;
}

/** Decimal comma, as in the GDD. */
export function num(x: number, digits = 2): string {
  let s = x.toFixed(digits);
  if (s.includes(".")) s = s.replace(/0+$/, "").replace(/\.$/, "");
  return s.replace(".", ",");
}

/** Turn → calendar date: turn 1 is the day of `startDate`, each night is the second half (R-TURN-02). */
export function turnDate(startDate: string | undefined, startTurn: number, turn: number, ru: Locale): string | null {
  const m = startDate && /^(\d{4})-(\d{2})-(\d{2})$/.exec(startDate);
  if (!m) return null;
  const days = Math.floor((turn - 1) / 2) - Math.floor((startTurn - 1) / 2);
  const d = new Date(Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3]) + days));
  return `${d.getUTCDate()} ${ru[`month.${d.getUTCMonth() + 1}`] ?? d.getUTCMonth() + 1} ${d.getUTCFullYear()}`;
}

/**
 * The four lines of one fire: n, A, D, p with every factor, ending in the
 * core's own values (`params`).
 */
export function fireLines(i: FireInput, params: FireParams, rules: Rules, ru: Locale, mods?: AttackModifiers): string[] {
  const t = (k: string) => ru[k] ?? k;
  const c = rules.combat;
  const frac = i.shot_fraction !== 1 ? ` × ${num(i.shot_fraction)}` : "";
  const prepared = mods?.prepared_pct ? ` (${t("ui.incl")} +${mods.prepared_pct} ${t("ui.pp")} ${t("ui.prepared_attack").toLowerCase()})` : "";
  const n = `n = ${t("ui.strength")} ${i.strength} × ${t("ui.readiness")} ${i.readiness_pct}%${prepared}${frac} = ${params.n}`;
  const aParts = [`${t("ui.attack_value")} ${num(i.attack, 1)}`];
  if (i.shooter_exp) aParts.push(`(1 + ${num(c.exp_attack_bonus)} × ${t("ui.exp")} ${i.shooter_exp})`);
  const unsupplied = mods?.unsupplied_target_mult ?? 1;
  const night = i.attack_mult / unsupplied;
  if (Math.abs(night - 1) > 1e-9) aParts.push(`${num(night)} ${t("ui.night_mult")}`);
  if (unsupplied !== 1) aParts.push(`${num(unsupplied)} ${t("ui.unsupplied_target").toLowerCase()}`);
  const a = `A = ${aParts.join(" × ")} = ${num(params.a)}`;
  const dParts = [`${t("ui.defense_value")} ${num(i.defense, 1)}`];
  if (i.terrain_mult !== 1) dParts.push(`${num(i.terrain_mult)} ${t("ui.terrain_mult")}`);
  if (i.entrenchment) dParts.push(`(1 + ${num(c.entrench_defense_bonus)} × ${t("ui.entrench")} ${i.entrenchment})`);
  if (i.target_exp) dParts.push(`(1 + ${num(c.exp_defense_bonus)} × ${t("ui.exp")} ${i.target_exp})`);
  if (i.defense_mult !== 1) dParts.push(`${num(i.defense_mult)} ${t("ui.canal_mult")}`);
  const d = `D = ${dParts.join(" × ")} = ${num(params.d)}`;
  const p =
    i.attack <= 0
      ? `p = 0 (${t("ui.no_fire")})`
      : `p = ${num(c.p_base)} + ${num(c.p_slope)} × (A − D)${i.p_shift ? ` − ${num(i.p_shift)}` : ""}, ${t("ui.clamped")} ${num(c.p_min)}…${num(c.p_max)} = ${num(params.p, 3)}`;
  return [n, a, d, p];
}

export interface ReadinessUnit {
  readiness_pct: number;
  base_readiness_pct: number;
  readiness_basis: { supply: number; morale_pct: number; exp_stars: number; hq_pct: number };
  attacks_made: number;
  attacks_received: number;
  crossing_penalty_pct: number;
  event_penalty_pct: number;
}

/** R-UNIT-01, R-HQ-02: how the unit's readiness came about. */
export function readinessTip(u: ReadinessUnit, rules: Rules, ru: Locale): string {
  const t = (k: string) => ru[k] ?? k;
  const r = rules.readiness;
  const b = u.readiness_basis;
  const lines = [
    `(${num(r.base)} + ${num(r.per_supply)} × ${t("ui.supply")} ${b.supply}) × ${t("ui.morale")} ${b.morale_pct}% × (1 + ${num(r.exp_bonus)} × ${t("ui.exp")} ${b.exp_stars}), ${t("ui.at_most")} ${num(r.max * 100, 0)}%`,
  ];
  if (b.hq_pct) lines.push(`${t("ui.hq")}: ${b.hq_pct > 0 ? "+" : ""}${b.hq_pct} ${t("ui.pp")}`);
  lines.push(`${t("ui.phase_start")}: ${u.base_readiness_pct}%`);
  const fatigue = u.attacks_made * r.fatigue_per_attack_pct + u.attacks_received * r.fatigue_per_attacked_pct;
  if (fatigue) lines.push(`${t("ui.fatigue")}: −${fatigue} ${t("ui.pp")}`);
  if (u.crossing_penalty_pct) lines.push(`${t("ui.crossing")}: −${u.crossing_penalty_pct} ${t("ui.pp")}`);
  if (u.event_penalty_pct) lines.push(`${t("ui.event_penalty")}: −${u.event_penalty_pct} ${t("ui.pp")}`);
  lines.push(`${t("ui.now")}: ${u.readiness_pct}%`);
  return lines.join("\n");
}

/** R-SUP-03, R-UNIT-02: what the supply value does. */
export function supplyTip(supply: number, rules: Rules, ru: Locale): string {
  const t = (k: string) => ru[k] ?? k;
  const s = rules.supply;
  const lines = [`${t("ui.supply")} ${supply} ${t("ui.of_10")}`];
  lines.push(supply <= s.low_supply_max ? `≤ ${s.low_supply_max}: AP −${rules.movement.low_supply_ap_penalty}` : `> ${s.low_supply_max}: ${t("ui.no_ap_penalty")}`);
  lines.push(supply >= s.entrench_min_supply ? `≥ ${s.entrench_min_supply}: ${t("ui.trenches_grow")}` : `< ${s.entrench_min_supply}: ${t("ui.trenches_stop")}`);
  return lines.join("\n");
}

/** Attribute-safe text for `title="…"`. */
export const attrText = (s: string) => s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
