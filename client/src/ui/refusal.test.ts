import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { refusalKey, refusalText } from "./refusal";

const ru = JSON.parse(readFileSync(new URL("../../../data/locale/ru.json", import.meta.url), "utf8")) as Record<string, string>;

describe("refusal reasons (R-UI-04 action bar)", () => {
  it("maps core errors to locale keys", () => {
    expect(refusalKey("illegal economy action: NotEnoughMpp")).toBe("refusal.economy.NotEnoughMpp");
    expect(refusalKey("illegal strategic transfer of unit 4: StartsInZoc")).toBe("refusal.transfer.StartsInZoc");
    expect(refusalKey("illegal bridge action by unit 12: NotEngineer")).toBe("refusal.bridge.NotEngineer");
    expect(refusalKey("illegal attack by unit 3: CannotBombard")).toBe("refusal.attack.CannotBombard");
    expect(refusalKey("unit 3 cannot be pinned to 9")).toBe("refusal.pin");
    expect(refusalKey("something new")).toBeNull();
  });

  it("every variant the action bar can meet has a Russian text", () => {
    const variants = {
      economy: ["ZeroSteps", "UnitActed", "NoSupply", "StepLimit", "FullStrength", "NotEnoughMpp", "PurchaseLimit"],
      transfer: ["Acted", "StartsInZoc", "NoRoute", "NotEnoughMpp"],
      bridge: ["NotEngineer", "Acted", "NoCanal", "AlreadyBridged", "NoFriendlyOnFarBank", "NoBridge"],
      attack: ["NoStrikes", "NoAp", "NotAdjacent", "OutOfRange", "NotEnemy", "NotGroundTarget", "UseBombard", "CannotBombard"],
    };
    for (const [family, names] of Object.entries(variants)) {
      for (const n of names) expect(ru[`refusal.${family}.${n}`], `${family}.${n}`).toBeTruthy();
    }
    for (const k of ["game_over", "not_your_unit", "pin", "in_combat", "combats_pending"]) expect(ru[`refusal.${k}`], k).toBeTruthy();
  });

  it("falls back to the core's text", () => {
    expect(refusalText("something new", ru)).toBe("something new");
    expect(refusalText("illegal economy action: FullStrength", ru)).not.toContain("FullStrength");
  });
});
