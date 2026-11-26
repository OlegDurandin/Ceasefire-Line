// R-UI-04 «Отчёты»: core events → lines for the players (locale strings).
// Routine events (refresh, moves, HQ links, phase end) are left out.

import { axialToOffset, type Axial } from "./hex";

type Locale = Record<string, string>;

export interface UnitRef {
  class: string;
  side: number;
}

export interface LogContext {
  ru: Locale;
  /** Units by id, including ones destroyed since (names stay known). */
  units: Map<number, UnitRef>;
  sideName(id: number | null): string;
}

type Ev = { type: string } & Record<string, unknown>;

export function fill(template: string, args: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (m, k: string) => (k in args ? String(args[k]) : m));
}

const hexLabel = (h: unknown) => {
  const o = axialToOffset(h as Axial);
  return `[${o.col}, ${o.row}]`;
};

/** One line for the log, or null for events not shown. */
export function formatEvent(e: Ev, ctx: LogContext): string | null {
  const t = (k: string) => ctx.ru[k] ?? k;
  const unit = (id: unknown) => {
    const u = ctx.units.get(id as number);
    return u ? `${t(`unit.${u.class}`)} (${ctx.sideName(u.side)})` : `#${String(id)}`;
  };
  const side = (id: unknown) => ctx.sideName(id === null || id === undefined ? null : (id as number));
  const line = (key: string, args: Record<string, string | number>) => fill(t(key), args);
  const n = (v: unknown) => v as number;
  switch (e.type) {
    case "phase_started":
      return line("log.phase_started", { turn: n(e.turn), side: side(e.side), time: e.night ? t("ui.night") : t("ui.day") });
    case "income_received":
      return line("log.income", { side: side(e.side), amount: n(e.amount), total: n(e.total) });
    case "combat_resolved":
      return line("log.combat", { attacker: unit(e.attacker), defender: unit(e.defender), al: n(e.attacker_losses), dl: n(e.defender_losses) });
    case "bombarded":
      return line("log.bombarded", { unit: unit(e.unit), target: unit(e.target), losses: n(e.losses) });
    case "retreated":
      return line("log.retreated", { unit: unit(e.unit), hex: hexLabel(e.to) });
    case "retreat_blocked":
      return line("log.retreat_blocked", { unit: unit(e.unit), losses: n(e.extra_losses) });
    case "unit_destroyed":
      return line("log.destroyed", { unit: unit(e.unit) });
    case "advanced":
      return line("log.advanced", { unit: unit(e.unit), hex: hexLabel(e.to) });
    case "attrition":
      return line("log.attrition", { unit: unit(e.unit), steps: n(e.steps) });
    case "source_besieged":
      return line("log.source_besieged", { hex: hexLabel(e.hex), value: n(e.value) });
    case "deserted":
      return line("log.deserted", { unit: unit(e.unit), steps: n(e.steps) });
    case "reinforced":
      return line(e.elite ? "log.reinforced_elite" : "log.reinforced", { unit: unit(e.unit), steps: n(e.steps), cost: n(e.cost) });
    case "purchased":
      return line("log.purchased", { side: side(e.side), class: t(`unit.${String(e.class)}`), cost: n(e.cost), turn: n(e.arrives_turn) });
    case "unit_arrived":
      return line("log.arrived", { class: t(`unit.${String(e.class)}`), hex: hexLabel(e.hex) });
    case "arrival_delayed":
      return line("log.arrival_delayed", { side: side(e.side), class: t(`unit.${String(e.class)}`) });
    case "hq_destroyed":
      return line("log.hq_destroyed", { unit: unit(e.hq) });
    case "national_morale": {
      const c = n(e.change);
      return line("log.nm", { side: side(e.side), change: c > 0 ? `+${c}` : String(c), value: n(e.value) });
    }
    case "objective_changed":
      return line("log.objective", { hex: hexLabel(e.hex), from: side(e.from), to: side(e.to) });
    case "transferred": {
      const path = e.path as unknown[];
      return line("log.transferred", { unit: unit(e.unit), hex: hexLabel(path[path.length - 1]), cost: n(e.cost) });
    }
    case "pinned":
      return e.hq === null ? line("log.unpinned", { unit: unit(e.unit) }) : line("log.pinned", { unit: unit(e.unit), hq: unit(e.hq) });
    case "bridge_started":
      return line("log.bridge_started", { unit: unit(e.unit), a: hexLabel(e.a), b: hexLabel(e.b) });
    case "bridge_built":
      return e.completed ? line("log.bridge_built", { a: hexLabel(e.a), b: hexLabel(e.b) }) : null;
    case "bridge_strike":
      return line(e.destroyed ? "log.bridge_hit" : "log.bridge_missed", {
        unit: unit(e.unit),
        a: hexLabel(e.a),
        b: hexLabel(e.b),
        chance: Math.round(n(e.chance) * 100),
      });
    case "mpp_changed": {
      const a = n(e.amount);
      return line("log.mpp", { side: side(e.side), amount: a > 0 ? `+${a}` : String(a), total: n(e.total) });
    }
    case "ceasefire_moved":
      return line("log.ceasefire", { turn: n(e.last_turn) });
    case "message":
      return t(String(e.key));
    case "tech_boost_bought":
      return line("log.boost_bought", { side: side(e.side), boost: t(`boost.${String(e.id)}`), turn: n(e.ready_turn) });
    case "tech_advanced":
      return line("log.tech", { side: side(e.side), branch: t(`tech.${String(e.branch)}`), level: n(e.level) });
    case "setup_warning":
      return line("log.setup_warning", { side: side(e.side), warning: JSON.stringify(e.warning) });
    case "game_over":
      return t("log.game_over");
    default:
      return null;
  }
}
