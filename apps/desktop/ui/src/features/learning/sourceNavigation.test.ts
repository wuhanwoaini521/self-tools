import { describe, expect, it } from "vitest";
import { sourceRoute } from "./sourceNavigation";

describe("sourceRoute", () => {
  it("reopens History and Geography by their stable source IDs", () => {
    expect(sourceRoute("history", "event", "event-anlu-luoyang")).toBe(
      "#history?event=event-anlu-luoyang",
    );
    expect(sourceRoute("history", "person", "person-li-longji")).toBe(
      "#history?person=person-li-longji",
    );
    expect(sourceRoute("history", "story", "story-an-lushan-rebellion")).toBe(
      "#history?story=story-an-lushan-rebellion",
    );
    expect(sourceRoute("geography", "place", "luoyang")).toBe("#geography?id=luoyang");
  });

  it("encodes IDs and fails closed for unsupported source types", () => {
    expect(sourceRoute("history", "event", "event with space")).toBe(
      "#history?event=event%20with%20space",
    );
    expect(sourceRoute("language", "word", "jmdict:1")).toBeNull();
    expect(sourceRoute("history", "work", "book-1")).toBeNull();
  });
});
