import { describe, expect, it } from "vitest";
import { completedStages } from "./shared";

describe("completedStages", () => {
  it("does not mark the stage you are still on", () => {
    expect(completedStages("vocabulary", false)).toEqual({
      vocabulary: false,
      listen: false,
      read: false,
      sentence: false,
      shadow: false,
      quiz: false,
    });
  });

  it("checks only stages before the saved frontier", () => {
    const done = completedStages("sentence", false);
    expect(done.vocabulary).toBe(true);
    expect(done.listen).toBe(true);
    expect(done.read).toBe(true);
    expect(done.sentence).toBe(false);
    expect(done.quiz).toBe(false);
  });

  it("checks the quiz when a score exists even if the frontier is still the quiz", () => {
    expect(completedStages("quiz", true).quiz).toBe(true);
    expect(completedStages("done", false).quiz).toBe(true);
  });
});
