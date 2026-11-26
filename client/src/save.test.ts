import { describe, expect, it } from "vitest";

import { decodeSave, encodeSave } from "./save";

describe("save (R-UI-05, R-UI-03 facing)", () => {
  it("round-trips commands and facing", () => {
    const text = encodeSave('[{"type":"end_phase"}]', new Map([[3, "right"], [7, "left"]]));
    const s = decodeSave(text)!;
    expect(s.commands).toEqual([{ type: "end_phase" }]);
    expect([...s.facing]).toEqual([[3, "right"], [7, "left"]]);
  });

  it("loads an old save (bare command array) with default facing", () => {
    const s = decodeSave('[{"type":"end_phase"}]')!;
    expect(s.commands).toHaveLength(1);
    expect(s.facing.size).toBe(0);
  });

  it("drops bad facing entries and rejects non-saves", () => {
    const s = decodeSave('{"commands":[],"facing":{"1":"up","x":"left","2":"right"}}')!;
    expect([...s.facing]).toEqual([[2, "right"]]);
    expect(decodeSave("{}")).toBeNull();
    expect(decodeSave("not json")).toBeNull();
  });
});
