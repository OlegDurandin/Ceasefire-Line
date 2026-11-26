// R-UI-04 «Итоги фазы противника» window (key L): the summary of the
// opponent's last phase from the viewer's side, hexes as buttons that take
// the camera there.

import type { Cell, PhaseSummary } from "../phase-summary";
import { fill } from "../log";

type Locale = Record<string, string>;

export interface SummaryNames {
  side: (id: number | null) => string;
  unit: (id: number) => string;
}

/** Hexes shown as buttons, at most this many per list. */
const MAX_CELLS = 12;

const cellButton = (c: Cell) => `<button type="button" class="sum-cell" data-cell="${c[0]},${c[1]}">[${c[0]}, ${c[1]}]</button>`;

export function phaseSummaryHtml(s: PhaseSummary, viewer: number, names: SummaryNames, ru: Locale): string {
  const t = (k: string) => ru[k] ?? k;
  const rows: string[] = [];
  const losses = [...s.losses].sort((a, b) => a[0] - b[0]);
  if (losses.length) rows.push(`<b>${t("ui.summary.losses")}</b>: ${losses.map(([side, n]) => `${names.side(side)} −${n}`).join(" · ")}`);
  if (s.destroyed.length) rows.push(`<b>${t("ui.summary.destroyed")}</b>: ${s.destroyed.map((d) => names.unit(d.id)).join(", ")}`);
  const cells = (list: Cell[]) => {
    const uniq = [...new Map(list.map((c) => [c.join(","), c] as const)).values()];
    const more = uniq.length > MAX_CELLS ? ` … +${uniq.length - MAX_CELLS}` : "";
    return uniq.slice(0, MAX_CELLS).map(cellButton).join(" ") + more;
  };
  const lost = s.captured.filter((c) => c.from === viewer).map((c) => c.cell);
  const taken = s.captured.filter((c) => c.by === viewer).map((c) => c.cell);
  if (lost.length) rows.push(`<b>${fill(t("ui.summary.lost_hexes"), { n: new Set(lost.map((c) => c.join(","))).size })}</b>: ${cells(lost)}`);
  if (taken.length) rows.push(`<b>${fill(t("ui.summary.taken_hexes"), { n: new Set(taken.map((c) => c.join(","))).size })}</b>: ${cells(taken)}`);
  if (s.objectives.length) {
    rows.push(
      `<b>${t("ui.summary.objectives")}</b>: ${s.objectives
        .map((o) => `${cellButton(o.cell)} ${names.side(o.from)} → ${names.side(o.to)}`)
        .join("; ")}`,
    );
  }
  const nm = [...s.nm].sort((a, b) => a[0] - b[0]);
  if (nm.length) {
    rows.push(
      `<b>${t("ui.summary.nm")}</b>: ${nm.map(([side, v]) => `${names.side(side)} ${v.change > 0 ? "+" : ""}${v.change} → ${v.value}%`).join(" · ")}`,
    );
  }
  const body = rows.length ? `<ul class="summary">${rows.map((r) => `<li>${r}</li>`).join("")}</ul>` : `<p class="muted">${t("ui.summary.quiet")}</p>`;
  return `<h2>${t("ui.summary.title")}</h2><p class="muted">${fill(t("ui.summary.phase"), { side: names.side(s.side), turn: s.turn })}</p>${body}`;
}
