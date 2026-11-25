import { describe, expect, it } from "vitest";

import { savePath } from "./save-path";

describe("editor save path", () => {
  it("allows only map.json and scenario.json of a valid scenario id", () => {
    expect(savePath("/repo", "golan67", "map.json")).toBe("/repo/data/scenarios/golan67/map.json");
    expect(savePath("/repo", "golan67", "scenario.json")).toBe("/repo/data/scenarios/golan67/scenario.json");
    expect(savePath("/repo", "golan67", "events.json")).toBeNull();
    expect(savePath("/repo", "../etc", "map.json")).toBeNull();
    expect(savePath("/repo", "golan67", "../../../x.json")).toBeNull();
    expect(savePath("/repo", "Golan", "map.json")).toBeNull();
  });
});
