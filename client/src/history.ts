// R-UI-07 «Историческая справка»: the scenario briefing, windows of important
// scenario events and the unit reference note. Texts come from localisation
// keys in the data (briefing.json, events.json `window`, units.json `info`,
// sprites.json `model_info`); victory conditions, «Отличия от истории» and the
// «Игровой эффект» line are built from the scenario data, not written as text.

export type Locale = Record<string, string>;

/** `data/scenarios/<id>/briefing.json`. */
export interface BriefingFile {
  image: string;
  title: string;
  dates: string;
  essay: string;
  question: string;
  /** Side key → localisation key of the player's role. */
  roles: Record<string, string>;
  sources: string[];
}

/** `window` of an important event in `events.json`. */
export interface EventWindow {
  title: string;
  text: string;
  image?: string;
}

/** The parts of `scenario.json` the briefing reads. */
export interface ScenarioFacts {
  last_turn?: number;
  options?: Record<string, string>;
  sides: { id: number; key: string; purchase_limits?: Record<string, number | { count: number; alt?: boolean }> }[];
  victory?: { side: number; victory: number; decisive: number }[];
  objectives?: { hex: [number, number]; vp: number; nm_value: number }[];
  sudden_death?: { side: number; threshold: number | null }[];
  vp_per_enemy_step?: { side: number; vp: number; per_steps?: number }[];
  exits?: { side: number; vp_per_step: number }[];
}

/** A scenario event effect as the core reports it (`scenario_event_fired.effects`). */
export type Effect = { kind: string } & Record<string, unknown>;

export function fill(s: string, vars: Record<string, string | number>): string {
  return s.replace(/\{(\w+)(:\+)?\}/g, (m, k: string, plus?: string) => {
    if (!(k in vars)) return m;
    const v = vars[k]!;
    return plus && typeof v === "number" && v > 0 ? `+${v}` : String(v);
  });
}

const signed = (n: number) => (n > 0 ? `+${n}` : n < 0 ? `−${-n}` : "0");

/**
 * R-UI-07: the «Игровой эффект» line of an event window, from its effects.
 * Messages are the window's own text and are left out.
 */
export function effectLine(effects: Effect[], ru: Locale, sideName: (id: number) => string, unitName: (u: Record<string, unknown>) => string): string {
  const t = (k: string) => ru[k] ?? k;
  const parts: string[] = [];
  for (const e of effects) {
    const side = typeof e.side === "number" ? sideName(e.side) : "";
    switch (e.kind) {
      case "message":
        break;
      case "reinforcements":
        parts.push(fill(t("effect.reinforcements"), { list: ((e.units as Record<string, unknown>[]) ?? []).map(unitName).join(", ") }));
        break;
      case "mpp":
        parts.push(fill(t("effect.mpp"), { side, amount: Number(e.amount) }));
        break;
      case "nm":
        parts.push(fill(t("effect.nm"), { side, change: signed(Number(e.change)) }));
        break;
      case "shift_last_turn":
        parts.push(fill(t("effect.shift_last_turn"), { turns: signed(Number(e.turns)) }));
        break;
      case "set_last_turn":
        parts.push(fill(t("effect.set_last_turn"), { turn: Number(e.turn) }));
        break;
      case "units_morale":
        parts.push(fill(t("effect.units_morale"), { side, change: signed(Number(e.change)) }));
        break;
      case "units_entrenchment":
        parts.push(fill(t("effect.units_entrenchment"), { side, change: signed(Number(e.change)) }));
        break;
      case "units_readiness_penalty":
        parts.push(fill(t("effect.units_readiness_penalty"), { side, pct: Number(e.pct) }));
        break;
      case "open_exits":
        parts.push(fill(t("effect.open_exits"), { side }));
        break;
      case "destroy_bridge":
        parts.push(t("effect.destroy_bridge"));
        break;
      default:
        parts.push(e.kind);
    }
  }
  return parts.length ? parts.join("; ") : t("ui.no_effect");
}

/** R-UI-07 «Отличия от истории»: limit classes marked `alt`, start options not at their historical value. */
export function historyDifferences(sc: ScenarioFacts, ru: Locale, sideName: (id: number) => string): string[] {
  const t = (k: string) => ru[k] ?? k;
  const out: string[] = [];
  for (const s of sc.sides) {
    for (const [cls, lim] of Object.entries(s.purchase_limits ?? {})) {
      if (typeof lim === "object" && lim.alt) {
        out.push(`${sideName(s.id)}: ${fill(t("ui.alt_purchase"), { class: ru[`unit.${cls}`] ?? cls })}`);
      }
    }
  }
  for (const [opt, value] of Object.entries(sc.options ?? {})) {
    if (value !== "historical") {
      out.push(
        fill(t("ui.option_differs"), {
          option: ru[`option.${opt}`] ?? opt,
          value: ru[`option.${opt}.${value}`] ?? value,
          history: ru[`option.${opt}.historical`] ?? "historical",
        }),
      );
    }
  }
  return out;
}

/** R-UI-07: victory conditions from the scenario data (R-NM-02). */
export function victoryLines(sc: ScenarioFacts, ru: Locale, sideName: (id: number) => string, objectiveName: (hex: [number, number]) => string, defaultSuddenDeath: number): string[] {
  const t = (k: string) => ru[k] ?? k;
  const out: string[] = [];
  if (sc.last_turn !== undefined) out.push(fill(t("ui.victory_last_turn"), { turn: sc.last_turn }));
  for (const v of sc.victory ?? []) {
    // A threshold ≤ 0: the side wins while the opponent leads by no more than |threshold|.
    const key = v.victory > 0 ? "ui.victory_line" : "ui.victory_line_hold";
    out.push(fill(t(key), { side: sideName(v.side), victory: v.victory, lead: -v.victory, decisive: v.decisive }));
  }
  for (const s of sc.vp_per_enemy_step ?? []) {
    out.push(fill(t("ui.victory_steps"), { side: sideName(s.side), vp: s.vp, per: s.per_steps ?? 1 }));
  }
  for (const e of sc.exits ?? []) {
    out.push(fill(t("ui.victory_exits"), { side: sideName(e.side), vp: e.vp_per_step }));
  }
  for (const o of sc.objectives ?? []) {
    out.push(fill(t("ui.objective_line"), { name: objectiveName(o.hex), vp: o.vp, nm: o.nm_value }));
  }
  for (const s of sc.sides) {
    const entry = sc.sudden_death?.find((d) => d.side === s.id);
    if (entry && entry.threshold === null) continue;
    out.push(`${sideName(s.id)}: ${fill(t("ui.sudden_death_line"), { pct: entry?.threshold ?? defaultSuddenDeath })}`);
  }
  return out;
}

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** Localised text: paragraphs from blank lines; a missing key shows the key. */
export function paragraphs(ru: Locale, key: string): string {
  const text = ru[key] ?? key;
  return text
    .split(/\n\s*\n/)
    .map((p) => `<p>${esc(p.trim()).replace(/\n/g, "<br>")}</p>`)
    .join("");
}

export interface StoryWindow {
  title: string;
  /** Image URL; while there is none (or it fails) a dark plate with the title. */
  image?: string;
  /** The wide strip under the window («9 июня 1967: …»). */
  strip?: string;
  /** Body HTML under the picture. */
  body: string;
  button: string;
  /** A narrower window (the unit reference note). */
  compact?: boolean;
}

/** R-UI-07: the story window; resolves when the player closes it. */
export function showStory(w: StoryWindow): Promise<void> {
  let root = document.getElementById("story");
  if (!root) {
    root = document.createElement("div");
    root.id = "story";
    document.body.appendChild(root);
  }
  const plate = `<div class="story-plate"><span>${esc(w.title)}</span></div>`;
  root.innerHTML = `<div class="story-box${w.compact ? " compact" : ""}" role="dialog" aria-modal="true" aria-label="${esc(w.title)}">
      <div class="story-title">${esc(w.title)}</div>
      <div class="story-frame">${w.image ? `<img class="story-img" alt="" src="${esc(w.image)}">` : plate}</div>
      <div class="story-body">${w.body}</div>
      <div class="story-actions"><button class="story-ok">${esc(w.button)}</button></div>
    </div>
    ${w.strip ? `<div class="story-strip">${esc(w.strip)}</div>` : ""}`;
  root.style.display = "flex";
  const img = root.querySelector<HTMLImageElement>(".story-img");
  if (img) img.onerror = () => img.replaceWith(Object.assign(document.createElement("div"), { className: "story-plate", innerHTML: `<span>${esc(w.title)}</span>` }));
  const box = root;
  return new Promise((resolve) => {
    const close = () => {
      box.style.display = "none";
      box.innerHTML = "";
      document.removeEventListener("keydown", onKey, true);
      resolve();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" || e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        close();
      }
    };
    document.addEventListener("keydown", onKey, true);
    box.querySelector<HTMLButtonElement>(".story-ok")!.onclick = close;
    box.querySelector<HTMLButtonElement>(".story-ok")!.focus();
  });
}

/** R-UI-07: the scenario briefing as a story window. */
export function briefingWindow(b: BriefingFile, sc: ScenarioFacts, ru: Locale, opts: { sideName: (id: number) => string; objectiveName: (hex: [number, number]) => string; suddenDeath: number; role?: string }): StoryWindow {
  const t = (k: string) => ru[k] ?? k;
  const list = (items: string[]) => (items.length ? `<ul>${items.map((i) => `<li>${esc(i)}</li>`).join("")}</ul>` : "");
  const section = (title: string, html: string) => (html ? `<h3>${esc(title)}</h3>${html}` : "");
  const roles = sc.sides
    .filter((s) => b.roles[s.key] && (!opts.role || opts.role === s.key))
    .map((s) => `<p><b>${esc(opts.sideName(s.id))}.</b> ${esc(t(b.roles[s.key]!))}</p>`)
    .join("");
  const body = [
    paragraphs(ru, b.essay),
    section(t("ui.design_question"), paragraphs(ru, b.question)),
    section(t("ui.your_role"), roles),
    section(t("ui.victory_conditions"), list(victoryLines(sc, ru, opts.sideName, opts.objectiveName, opts.suddenDeath))),
    section(t("ui.history_differences"), list(historyDifferences(sc, ru, opts.sideName))),
    section(t("ui.sources"), b.sources.length ? `<ul dir="ltr">${b.sources.map((i) => `<li><bdi>${esc(i)}</bdi></li>`).join("")}</ul>` : ""),
  ].join("");
  return {
    title: t(b.title),
    image: `/assets/briefing/${b.image}.webp`,
    strip: `${t(b.dates)}: ${t(b.title)}`,
    body,
    button: t("ui.to_battle"),
  };
}
