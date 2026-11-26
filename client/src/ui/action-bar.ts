// R-UI-04 action bar above the bottom window, as on the approved mock
// (component «Панель действий»): ◀ ▶ (N), hexagonal buttons of the selected
// unit's actions, «i» — unit / hex information. Whether an action is
// available and why not come from the core's dry run (`Game.check`); this
// module only lays the buttons out.

import { attrText } from "../explain";
import { icon } from "./icons";

type Locale = Record<string, string>;

export type ActionId = "reinforce" | "elite" | "transfer" | "pin" | "bombard" | "bridge" | "heli" | "undo";
export const ACTIONS: ActionId[] = ["reinforce", "elite", "transfer", "pin", "bombard", "bridge", "heli", "undo"];

export interface ActionState {
  enabled: boolean;
  /** The tooltip: what the action does, or why it is not available. */
  tip: string;
  /** A mode is on (transfer waiting for a hex). */
  on?: boolean;
}

export function actionBarHtml(states: Record<ActionId, ActionState>, info: "unit" | "hex", ru: Locale): string {
  const t = (k: string) => ru[k] ?? k;
  const round = (id: string, name: string, label: string, cls = "") =>
    `<button type="button" id="${id}" class="ab-round${cls}" aria-label="${attrText(label)}" title="${attrText(label)}">${icon(name, 18, 2.4)}</button>`;
  const hexes = ACTIONS.map((a) => {
    const s = states[a];
    return `<button type="button" class="ab-hex${s.enabled ? "" : " off"}${s.on ? " on" : ""}" data-action="${a}" aria-label="${attrText(t(`ui.act.${a}`))}" title="${attrText(s.tip)}" aria-disabled="${!s.enabled}"><span>${icon(a, 20, 2.4)}</span></button>`;
  }).join("");
  return `<div class="ab" role="toolbar" aria-label="${attrText(t("ui.act.bar"))}">
    ${round("prev-unit", "prev", t("ui.prev_unit"))}${round("next-unit", "next", t("ui.next_unit"))}
    <div class="ab-sep" aria-hidden="true"></div>${hexes}<div class="ab-sep" aria-hidden="true"></div>
    ${round("info-mode", "info", t("ui.act.info"), info === "hex" ? " info on" : " info")}
  </div>`;
}
