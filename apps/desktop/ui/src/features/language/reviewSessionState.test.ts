import { describe, expect, it } from "vitest";
import { afterReviewSubmission } from "./reviewSessionState";

describe("review session queue", () => {
  it("keeps the next card in place after a submission", () => {
    const cards = ["first", "second", "third"];

    expect(afterReviewSubmission(cards, 0)).toEqual({
      queue: ["second", "third"],
      index: 0,
    });
    expect(afterReviewSubmission(cards, 1)).toEqual({
      queue: ["first", "third"],
      index: 1,
    });
  });

  it("moves back when the submitted card was last in the queue", () => {
    expect(afterReviewSubmission(["first", "last"], 1)).toEqual({
      queue: ["first"],
      index: 0,
    });
  });

  it("returns an empty queue after submitting the only card", () => {
    expect(afterReviewSubmission(["last"], 0)).toEqual({
      queue: [],
      index: 0,
    });
  });
});
