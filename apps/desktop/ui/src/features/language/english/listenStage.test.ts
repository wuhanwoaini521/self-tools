import { describe, expect, it } from "vitest";
import { similarity } from "./ListenStage";

describe("listening dictation comparison", () => {
  it("scores matching words while ignoring case and punctuation", () => {
    expect(similarity("Don't hesitate to ask!", "don't hesitate to ask")).toBe(1);
  });

  it("reports partial overlap against the transcript", () => {
    expect(similarity("I would like some coffee", "I like coffee")).toBe(0.6);
  });

  it("returns zero when no transcript words are available", () => {
    expect(similarity("", "hello there")).toBe(0);
  });
});
