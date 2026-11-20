import { describe, expect, it } from "vitest";
import {
  OVERFLOW_NAV_GROUPS,
  PAGE_IDS,
  PAGE_SHORTCUT_MAP,
  parseHashPath,
  primaryNavIdForPage,
  resolvePrimaryNavigation,
  shortcutDigitForPage,
} from "./productNav";

describe("productNav", () => {
  it("keeps legacy page hashes plus the learn hub", () => {
    expect(PAGE_IDS).toContain("learn");
    expect(PAGE_IDS).toContain("history");
    expect(PAGE_IDS).toContain("rss");
    expect(PAGE_IDS).toContain("knowledge");
    expect(parseHashPath("#history?event=e1")).toBe("history");
    expect(parseHashPath("#geography?id=beijing")).toBe("geography");
    expect(parseHashPath("#travel?city=京都&days=3")).toBe("travel");
    expect(parseHashPath("#rss")).toBe("rss");
    expect(parseHashPath("#learn")).toBe("learn");
    expect(parseHashPath("#nope")).toBeNull();
  });

  it("highlights primary nav for section pages", () => {
    expect(primaryNavIdForPage("home")).toBe("home");
    expect(primaryNavIdForPage("history")).toBe("learn");
    expect(primaryNavIdForPage("study-board")).toBe("learn");
    expect(primaryNavIdForPage("review")).toBe("learn");
    expect(primaryNavIdForPage("rss")).toBe("news");
    expect(primaryNavIdForPage("news")).toBe("news");
    expect(primaryNavIdForPage("travel")).toBe("travel");
    expect(primaryNavIdForPage("markdown")).toBeNull();
    expect(primaryNavIdForPage("server")).toBeNull();
  });

  it("does not bounce away from a section page when re-clicking its primary item", () => {
    expect(resolvePrimaryNavigation("geography", "learn")).toBe("geography");
    expect(resolvePrimaryNavigation("home", "learn")).toBe("learn");
    expect(resolvePrimaryNavigation("rss", "news")).toBe("rss");
    expect(resolvePrimaryNavigation("home", "news")).toBe("news");
    expect(resolvePrimaryNavigation("travel", "travel")).toBe("travel");
  });

  it("maps digits 1-4 to the four primary entries", () => {
    expect(PAGE_SHORTCUT_MAP).toEqual({
      "1": "home",
      "2": "learn",
      "3": "travel",
      "4": "news",
    });
    expect(shortcutDigitForPage("home")).toBe("1");
    expect(shortcutDigitForPage("learn")).toBe("2");
    expect(shortcutDigitForPage("markdown")).toBeNull();
    expect(shortcutDigitForPage("knowledge")).toBeNull();
  });

  it("keeps overflow to learning extras and search, not removal candidates", () => {
    const ids = OVERFLOW_NAV_GROUPS.flatMap((group) => group.items.map((item) => item.id));
    expect(ids).toEqual(["review", "collections", "search"]);
    expect(parseHashPath("#markdown")).toBe("markdown");
    expect(parseHashPath("#knowledge")).toBe("knowledge");
    expect(parseHashPath("#server")).toBe("server");
  });
});
