import { describe, expect, it } from "vitest";
import type { GeoHistoryCrosswalk } from "../exploration/explorationClient";
import type {
  SemanticEventDetail,
  SemanticEventStory,
  SemanticStoryEvent,
} from "../history/semanticTypes";
import {
  formatHistoricalRange,
  formatHistoricalYear,
  reviewedEventStories,
  reviewedLocationEvents,
  reviewedStoryEvents,
} from "./geoTimeline";

const crosswalk: GeoHistoryCrosswalk = {
  id: "geography:luoyang|history:cbdb-place-14693",
  geography_entity_id: "luoyang",
  history_place_id: "cbdb-place-14693",
  relation_type: "modern_urban_area_anchor",
  status: "reviewed",
  evidence_time_scope: { start_year: 756, end_year: 756, precision: "year" },
  geography_source_ids: ["geography-wikidata-q187136"],
  history_source_ids: ["source-curated-backbone-v1"],
};

function locationEvent(
  id: string,
  placeId: string,
  startYear: number | null,
  overrides: Partial<SemanticEventDetail["places"][number]> = {},
): SemanticEventDetail {
  return {
    event: {
      id,
      name_zh_cn: "洛阳失守",
      event_type: "war",
      start_year: startYear,
      end_year: startYear,
      date_precision: "year",
      quality_status: "reviewed",
    },
    related_stories: [],
    people: [],
    places: [{
      event_id: id,
      place_id: placeId,
      place_name: "洛阳",
      source_id: "source-curated-backbone-v1",
      quality_status: "reviewed",
      link_status: "linked",
      link_quality_status: "reviewed",
      ...overrides,
    }],
    relations: [],
    historical_texts: [],
    evidences: [],
    sources: [],
  };
}

function eventStory(overrides: Partial<SemanticEventStory> = {}): SemanticEventStory {
  return {
    event_id: "event-anlu-luoyang",
    story_id: "story-an-lushan-rebellion",
    sequence: 3,
    link_quality_status: "reviewed",
    story: {
      id: "story-an-lushan-rebellion",
      title_zh_cn: "安史之乱",
      start_year: 751,
      end_year: 763,
      quality_status: "reviewed",
      usable: true,
    },
    ...overrides,
  };
}

function storyEvent(
  eventId: string,
  startYear: number | null,
  sequence: number,
  linkStatus = "reviewed",
): SemanticStoryEvent {
  return {
    story_id: "story-an-lushan-rebellion",
    event_id: eventId,
    sequence,
    quality_status: linkStatus,
    name_zh_cn: eventId,
    start_year: startYear,
    end_year: startYear,
    event_quality_status: "reviewed",
  };
}

describe("Geography history timeline trust boundaries", () => {
  it("requires an exact reviewed place ID, reviewed links, source, and evidence scope", () => {
    const valid = locationEvent("event-anlu-luoyang", "cbdb-place-14693", 756);
    const similarNameOnly = locationEvent("event-name-match", "cbdb-place-100409", 756);
    const outsideEvidenceScope = locationEvent("event-old-capital", "cbdb-place-14693", 690);
    const unreviewedLink = locationEvent("event-pending", "cbdb-place-14693", 756, {
      link_status: "needs_linking",
    });
    const unknownDate = locationEvent("event-undated", "cbdb-place-14693", null);

    expect(reviewedLocationEvents(
      [similarNameOnly, outsideEvidenceScope, unreviewedLink, unknownDate, valid],
      [crosswalk],
    )).toEqual([valid]);
  });

  it("fails closed on stories with unreviewed links or unusable records", () => {
    const valid = eventStory();
    const pending = eventStory({ event_id: "event-pending", link_quality_status: "candidate" });
    const unusable = eventStory({
      event_id: "event-unusable",
      story: { ...valid.story, usable: false },
    });

    expect(reviewedEventStories([
      { ...locationEvent("event-anlu-luoyang", "cbdb-place-14693", 756), related_stories: [valid, pending, unusable] },
    ])).toEqual([valid]);
  });

  it("sorts reviewed story events by known date and keeps undated items last", () => {
    expect(reviewedStoryEvents([
      storyEvent("later", 763, 9),
      storyEvent("unknown", null, 1),
      storyEvent("early", 751, 1),
      storyEvent("pending", 752, 2, "candidate"),
    ]).map((event) => event.event_id)).toEqual(["early", "later", "unknown"]);
  });

  it("formats BCE, CE, approximate, range, and unknown years without treating zero as a date", () => {
    expect(formatHistoricalYear(-756)).toBe("公元前756年");
    expect(formatHistoricalYear(756)).toBe("公元756年");
    expect(formatHistoricalYear(0)).toBe("年代待考");
    expect(formatHistoricalRange(751, 763, "range")).toBe("公元751年 — 公元763年");
    expect(formatHistoricalRange(-205, -204, "approximate")).toBe("约公元前205年 — 公元前204年");
    expect(formatHistoricalRange(null, null, null)).toBe("年代待考");
  });
});
