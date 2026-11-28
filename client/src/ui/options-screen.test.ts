import { describe, expect, it } from "vitest";

import { parseSettings } from "../settings";
import { columns, optionsFrom, optionsHtml, stepSpeed, stepStyle, toggle } from "./options-screen";

const preview = (s: string) => `<img alt="${s}">`;

describe("options screen", () => {
  it("switches are on unless stored off; stored values survive parsing", () => {
    const o = optionsFrom(parseSettings('{"summary":false,"trail":true,"bogus":1}'), "miniature", "x2");
    expect(o.switches).toEqual({ windows: true, summary: false, warnUnused: true, trail: true });
  });

  it("«Показывать ходы и бои» is the AI speed: off → instant, on → ×1", () => {
    let o = optionsFrom({}, "miniature", "x4");
    expect(columns(o)[2]!.rows[0]!.on).toBe(true);
    o = toggle(o, "showAi");
    expect(o.aiSpeed).toBe("instant");
    expect(toggle(o, "showAi").aiSpeed).toBe("x1");
    expect(toggle(o, "summary").switches.summary).toBe(false);
  });

  it("pickers step through values and stop at the ends", () => {
    const o = optionsFrom({}, "miniature", "x1");
    expect(stepStyle(o, 1).style).toBe("medallion");
    expect(stepStyle(stepStyle(o, 1), 1).style).toBe("medallion");
    expect(stepSpeed(o, -1).aiSpeed).toBe("x1");
    expect(stepSpeed(o, 1).aiSpeed).toBe("x2");
  });

  it("fog of war is shown but not available; the ends of a picker are disabled", () => {
    const html = optionsHtml(optionsFrom({}, "miniature", "x1"), {}, preview);
    expect(html).toMatch(/data-switch="fog" role="switch" aria-checked="false" disabled/);
    expect(html).toContain('data-step="style:-1" aria-label="ui.opt.prev" disabled');
    expect(html).not.toContain('data-step="style:1" aria-label="ui.opt.next" disabled');
    expect(html).toContain('<img alt="miniature">');
  });
});
