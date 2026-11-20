import { describe, expect, it } from "vitest";
import { generateHillTerrain } from "./generators";

describe("hill terrain", () => {
  it("has a round crest, a lower slope, then a still lower vale", () => {
    const height = generateHillTerrain(17);
    const crest = height(0, 0);
    const slope = height(0.95, 0.06);
    const vale = height(1.52, 0.1);
    expect(crest).toBeGreaterThan(slope + 0.18);
    expect(slope).toBeGreaterThan(vale + 0.08);
    expect(crest).toBeLessThan(1.4);
  });
});
