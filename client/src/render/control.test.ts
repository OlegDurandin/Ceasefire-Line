import { describe, expect, it } from "vitest";

import { frontSides } from "./control";

describe("frontSides (R-MOVE-06, R-UI-02)", () => {
  it("marks sides between two owners once, ignores own and unowned neighbours", () => {
    // Odd-q: [0,0] and [1,0] are neighbours, so are [1,0] and [2,0].
    const owners = new Map([
      ["0,0", 0],
      ["1,0", 0],
      ["2,0", 1],
      ["0,3", 1],
    ]);
    const front = frontSides(owners);
    expect(front).toEqual([
      [
        [1, 0],
        [2, 0],
      ],
    ]);
  });
});
