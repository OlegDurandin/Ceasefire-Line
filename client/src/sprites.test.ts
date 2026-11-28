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

  it("fictional nations (GDD section 14): Merid and Tarvia western, Salvera Soviet", () => {
    expect(spriteSide(sprites, "merid")).toBe("isr");
    expect(spriteSide(sprites, "tarvia")).toBe("isr");
    expect(spriteSide(sprites, "salvera")).toBe("egy_syr");
    expect(pickSprite(sprites, "tank", "merid", {}, 1975)?.id).toBe("isr_centurion");
    expect(pickSprite(sprites, "tank", "salvera", { armor: 2 }, 1975)?.id).toBe("egy_t62");
    expect(pickSprite(sprites, "fighter", "tarvia", {}, 1975)?.id).toBe("isr_mirage3");
  });

  it("Tarvia fields 1967 tanks: Super Sherman by default, AMX-13 and T-34-85 as models", () => {
    expect(pickSprite(sprites, "tank", "tarvia", {}, 1975)).toEqual({
      side: "isr",
      id: "isr_super_sherman",
      url: "/assets/units_minis/isr/isr_super_sherman.png",
    });
    expect(pickSprite(sprites, "tank", "tarvia", {}, 1975, "isr_amx13")?.url).toBe("/assets/units_minis/isr/isr_amx13.png");
    // A Soviet model from the other side's folder.
    expect(pickSprite(sprites, "tank", "tarvia", {}, 1975, "egy_t34_85")).toEqual({
      side: "egy_syr",
      id: "egy_t34_85",
      url: "/assets/units_minis/egy_syr/egy_t34_85.png",
    });
    // Israel 1967 does not get Tarvia's tier, nor Tarvia Israel's.
    expect(pickSprite(sprites, "tank", "israel", {}, 1967)?.id).toBe("isr_centurion");
    expect(pickSprite(sprites, "tank", "merid", {}, 1975, "egy_t34_85")?.id).toBe("isr_centurion");
  });

  it("unknown nation or class falls back to APP-6 (null)", () => {
    expect(pickSprite(sprites, "tank", null, {})).toBeNull();
    expect(pickSprite(sprites, "battleship", "israel", {})).toBeNull();
  });

  it("every terrain type has a texture", () => {
    for (const t of ["plain", "dunes", "hills", "mountains", "marsh", "city", "fortified", "forest"])
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

describe("R-UI-03: scenario year and models", () => {
  it.each([[1967, "isr_vautour", "isr_m50_sp"], [1968, "isr_a4", "isr_m50_sp"], [1969, "isr_a4", "isr_m50_sp"], [1970, "isr_a4", "isr_m109"]] as const)(
    "selects strike and artillery models in %i", (year, strike, artillery) => {
      for (const level of [1, 2, 3]) {
        expect(pickSprite(sprites, "strike", "israel", { aviation: level }, year)?.id).toBe(strike);
        expect(pickSprite(sprites, "artillery", "israel", {}, year)?.id).toBe(artillery);
      }
    },
  );
  it("has no A-4 or M109 for Israeli units in either 1967 scenario", () => {
    for (const id of ["golan67", "sinai67"]) {
      const scenario = JSON.parse(readFileSync(new URL(`../../data/scenarios/${id}/scenario.json`, import.meta.url), "utf8"));
      for (const unit of scenario.units) {
        if (unit.nation !== "israel") continue;
        const tech = scenario.sides.find((side: { id: number }) => side.id === unit.side)?.tech ?? {};
        expect(["isr_a4", "isr_m109"]).not.toContain(pickSprite(sprites, unit.class, unit.nation, tech, Number(scenario.start_date.slice(0, 4)), unit.model)?.id);
      }
    }
  });
  it.each([["israel", "isr_amx13"], ["egypt", "egy_t34_85"], ["egypt", "egy_is3"], ["egypt", "egy_su100"], ["syria", "syr_pz4"], ["syria", "syr_stug3"]])(
    "uses an explicit %s model %s", (nation, model) => {
      expect(pickSprite(sprites, "tank", nation, { armor: 2 }, 1967, model)?.id).toBe(model);
      expect(pickSprite(sprites, "tank", nation, {}, 1967)?.id).toBe(nation === "israel" ? "isr_centurion" : "egy_t55");
    },
  );
  it("rejects models from another nation, class or period", () => {
    expect(pickSprite(sprites, "tank", "syria", {}, 1967, "egy_is3")?.id).toBe("egy_t55");
    expect(pickSprite(sprites, "infantry", "israel", {}, 1967, "isr_amx13")?.id).toBe("isr_infantry");
    expect(pickSprite(sprites, "strike", "israel", {}, 1967, "isr_a4")?.id).toBe("isr_vautour");
  });
  it("prepares Jordan and supports miniature-only models in either style", () => {
    expect(spriteSide(sprites, "jordan")).toBe("jor");
    const ref = pickSprite(sprites, "tank", "jordan", {}, 1967)!;
    const index = miniIndex([{ side: "jor", id: "jor_m48", anchor: [0.5, 0.88] }]);
    expect(ref.url).toBe("/assets/units_minis/jor/jor_m48.png");
    expect(styledSprite(ref, "medallion", sprites, index).style).toBe("miniature");
  });
});
