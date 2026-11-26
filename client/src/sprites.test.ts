import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { miniIndex, pickSprite, spriteSide, styledSprite, terrainTextureUrl, type SpritesFile } from "./sprites";

const sprites = JSON.parse(readFileSync(new URL("../../data/sprites.json", import.meta.url), "utf8")) as SpritesFile;

describe("sprites.json binding", () => {
  it("egy_syr covers Egypt and Syria", () => {
    expect(spriteSide(sprites, "egypt")).toBe("egy_syr");
    expect(spriteSide(sprites, "syria")).toBe("egy_syr");
    expect(spriteSide(sprites, "israel")).toBe("isr");
  });

  it("tank tier follows armour level", () => {
    expect(pickSprite(sprites, "tank", "israel", {})?.id).toBe("isr_centurion");
    expect(pickSprite(sprites, "tank", "israel", { armor: 2 })?.id).toBe("isr_m60");
    expect(pickSprite(sprites, "infantry", "syria", { antitank: 2 })?.id).toBe("egy_atgm_team");
  });

  it("unknown nation or class falls back to APP-6 (null)", () => {
    expect(pickSprite(sprites, "tank", null, {})).toBeNull();
    expect(pickSprite(sprites, "battleship", "israel", {})).toBeNull();
  });

  it("every terrain type has a texture", () => {
    for (const t of ["plain", "dunes", "hills", "mountains", "marsh", "city", "fortified"])
      expect(terrainTextureUrl(sprites, t)).toMatch(/^\/assets\/terrain\/terrain_\w+\.png$/);
  });
});

describe("unit styles (R-UI-03)", () => {
  const minis = miniIndex([{ id: "isr_centurion", side: "isr", anchor: [0.5, 0.88] }]);
  const tank = pickSprite(sprites, "tank", "israel", {})!;

  it("uses the miniature when the pack has it", () => {
    const s = styledSprite(tank, "miniature", sprites, minis);
    expect(s.style).toBe("miniature");
    expect(s.url).toBe("/assets/units_minis/isr/isr_centurion.png");
    expect(s.anchor).toEqual([0.5, 0.88]);
  });

  it("falls back to the medallion for a missing miniature or on request", () => {
    const hawk = { side: "isr", id: "isr_hawk", url: "/assets/units/isr/isr_hawk.png" };
    expect(styledSprite(hawk, "miniature", sprites, minis).style).toBe("medallion");
    expect(styledSprite(tank, "medallion", sprites, minis).url).toBe("/assets/units/isr/isr_centurion.png");
  });

  it("sprites.json defaults to miniatures", () => {
    expect(sprites.default_style).toBe("miniature");
  });
});
