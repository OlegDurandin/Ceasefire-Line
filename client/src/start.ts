// Title backdrop and the two SC-style screens over it: scenario selection
// (new game) and options. The markup comes from ui/scenario-screen and
// ui/options-screen; this file loads the data and wires the buttons.

import { victoryLines, type BriefingFile, type Locale, type ScenarioFacts } from "./history";
import { modeKey, modes, scenarioScreenHtml, type Mode, type ScenarioCard } from "./ui/scenario-screen";
import { optionsHtml, stepSpeed, stepStyle, toggle, type Options } from "./ui/options-screen";
import type { UnitStyle } from "./sprites";

async function optionalText(url: string): Promise<string | null> {
  try {
    const r = await fetch(url);
    return r.ok ? await r.text() : null;
  } catch {
    return null;
  }
}

/** Cards of the scenarios in data/scenarios/index.json (a missing file → just `current`). */
export async function loadCards(current: string, ru: Locale, suddenDeath: number): Promise<ScenarioCard[]> {
  const index = await optionalText("/data/scenarios/index.json");
  const ids = index ? ((JSON.parse(index) as { scenarios?: string[] }).scenarios ?? [current]) : [current];
  if (!ids.includes(current)) ids.unshift(current);
  const cards = await Promise.all(
    ids.map(async (id): Promise<ScenarioCard | null> => {
      const [sc, br] = await Promise.all([optionalText(`/data/scenarios/${id}/scenario.json`), optionalText(`/data/scenarios/${id}/briefing.json`)]);
      if (!sc) return null;
      const s = JSON.parse(sc) as ScenarioFacts & { start_date?: string; hex_km?: number };
      const b = br ? (JSON.parse(br) as BriefingFile) : null;
      const keys = [...s.sides].sort((a, c) => a.id - c.id).map((x) => x.key);
      const side = (n: number) => ru[`side.${keys[n] ?? ""}`] ?? String(n);
      const place = (h: [number, number]) => ru[`place.${id}.${h[0]}_${h[1]}`] ?? `[${h[0]}, ${h[1]}]`;
      return {
        id,
        name: ru[`scenario.${id}.name`] ?? id,
        startDate: s.start_date ?? null,
        turns: s.last_turn ?? null,
        hexKm: s.hex_km ?? null,
        sideKeys: keys,
        image: b?.image ?? null,
        dates: b ? (ru[b.dates] ?? null) : null,
        question: b ? (ru[b.question] ?? null) : null,
        victory: victoryLines(s, ru, side, place, suddenDeath),
      };
    }),
  );
  return cards.filter((c): c is ScenarioCard => c !== null);
}

const imageUrl = (id: string) => `/assets/briefing/${id}.webp`;
/** assets/manifest.json id of the title picture; until it exists — the scenario's illustration. */
const MENU_BACKGROUND = "menu_background";

/** A full-screen layer over the game (or over nothing, at the start). */
function layer(className: string): HTMLElement {
  const el = document.createElement("div");
  el.className = className;
  document.body.appendChild(el);
  return el;
}

/**
 * The scenario selection screen on the title backdrop. Resolves with the
 * scenario and mode, or null on «Отмена» (only when `cancel`).
 */
export async function chooseScenario(cards: ScenarioCard[], current: string, ru: Locale, cancel: boolean): Promise<{ id: string; mode: Mode } | null> {
  // The title picture, when the asset manifest has it (no request for a missing file).
  const manifest = await optionalText("/assets/manifest.json");
  const menuArt = manifest ? (JSON.parse(manifest) as { id: string; file: string }[]).find((a) => a.id === MENU_BACKGROUND) : undefined;
  const title = layer("title-screen");
  let selected = current;
  let view: "about" | "victory" = "about";
  let mode: Mode = { kind: "hotseat" };
  const card = () => cards.find((c) => c.id === selected) ?? cards[0]!;
  const render = () => {
    const c = card();
    // Backdrop: the menu picture when it exists, else the scenario's illustration.
    const bg = menuArt ? `/assets/${menuArt.file}` : c.image ? imageUrl(c.image) : null;
    title.style.setProperty("--ss-bg", bg ? `url('${bg}')` : "none");
    title.innerHTML = `<div class="title-logo"><div class="title-name">${ru["ui.title"] ?? "Ceasefire Line"}</div><div class="title-sub">${ru["ui.title_sub"] ?? ""}</div></div>${scenarioScreenHtml(cards, c.id, mode, view, ru, imageUrl, cancel)}`;
  };
  render();
  return await new Promise((resolve) => {
    const done = (v: { id: string; mode: Mode } | null) => {
      window.removeEventListener("keydown", onKey);
      title.remove();
      resolve(v);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && cancel) done(null);
      if (e.key === "Enter") done({ id: card().id, mode });
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        const i = cards.findIndex((c) => c.id === card().id);
        const next = cards[Math.min(cards.length - 1, Math.max(0, i + (e.key === "ArrowDown" ? 1 : -1)))]!;
        if (next.id !== selected) pick(next.id);
        e.preventDefault();
      }
    };
    const pick = (id: string) => {
      selected = id;
      view = "about";
      // The mode carries over when the new scenario has that side.
      if (!modes(card()).some((m) => modeKey(m) === modeKey(mode))) mode = { kind: "hotseat" };
      render();
    };
    window.addEventListener("keydown", onKey);
    title.addEventListener("click", (ev) => {
      const el = (ev.target as HTMLElement).closest<HTMLElement>("button");
      if (!el) return;
      if (el.dataset.scenario) pick(el.dataset.scenario);
      else if (el.dataset.mode) {
        mode = modes(card()).find((m) => modeKey(m) === el.dataset.mode) ?? mode;
        render();
      } else if (el.id === "ss-victory") {
        view = view === "victory" ? "about" : "victory";
        render();
      } else if (el.id === "ss-play") done({ id: card().id, mode });
      else if (el.id === "ss-cancel") done(null);
    });
  });
}

/** The options screen over the game; resolves with the new options, or null on «Отмена». */
export function editOptions(start: Options, ru: Locale, stylePreview: (s: UnitStyle) => string): Promise<Options | null> {
  const dim = layer("screen-dim");
  let o = start;
  const render = () => (dim.innerHTML = optionsHtml(o, ru, stylePreview));
  render();
  return new Promise((resolve) => {
    const done = (v: Options | null) => {
      window.removeEventListener("keydown", onKey, true);
      dim.remove();
      resolve(v);
    };
    // Captured so the game's keys (Enter = end of phase) do not fire under the screen.
    const onKey = (e: KeyboardEvent) => {
      e.stopPropagation();
      if (e.key === "Escape") done(null);
      else if (e.key === "Enter") done(o);
    };
    window.addEventListener("keydown", onKey, true);
    dim.addEventListener("click", (ev) => {
      const el = (ev.target as HTMLElement).closest<HTMLElement>("button");
      if (!el || el.hasAttribute("disabled")) return;
      if (el.dataset.switch) o = toggle(o, el.dataset.switch);
      else if (el.dataset.step) {
        const [what, dir] = el.dataset.step.split(":") as [string, string];
        o = what === "style" ? stepStyle(o, Number(dir) as -1 | 1) : stepSpeed(o, Number(dir) as -1 | 1);
      } else if (el.id === "op-ok") return done(o);
      else if (el.id === "op-cancel") return done(null);
      render();
    });
  });
}
