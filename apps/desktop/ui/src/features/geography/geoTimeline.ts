import type { GeoHistoryCrosswalk } from "../exploration/explorationClient";
import type {
  SemanticEventDetail,
  SemanticEventStory,
  SemanticStoryEvent,
} from "../history/semanticTypes";

const REVIEWED = new Set(["reviewed", "verified", "curated"]);

export function isReviewedStatus(value: string | null | undefined): boolean {
  return value != null && REVIEWED.has(value);
}

function eventFitsEvidenceScope(
  event: SemanticEventDetail["event"],
  crosswalk: GeoHistoryCrosswalk,
): boolean {
  const scope = crosswalk.evidence_time_scope;
  if (!scope) return true;
  const start = event.start_year;
  const end = event.end_year ?? start;
  if (start == null || end == null || start === 0 || end === 0) return false;
  return start >= scope.start_year && end <= scope.end_year;
}

/**
 * Keep only events connected through the exact reviewed History place ID, source,
 * link status, and (when present) the crosswalk's reviewed evidence time scope.
 */
export function reviewedLocationEvents(
  events: SemanticEventDetail[],
  crosswalks: GeoHistoryCrosswalk[],
): SemanticEventDetail[] {
  const seen = new Set<string>();
  return events.filter((detail) => {
    if (!isReviewedStatus(detail.event.quality_status) || seen.has(detail.event.id)) {
      return false;
    }
    const matched = crosswalks.some((crosswalk) => {
      if (crosswalk.status !== "reviewed" || !eventFitsEvidenceScope(detail.event, crosswalk)) {
        return false;
      }
      return detail.places.some((place) =>
        place.place_id === crosswalk.history_place_id &&
        place.link_status === "linked" &&
        isReviewedStatus(place.quality_status) &&
        isReviewedStatus(place.link_quality_status) &&
        Boolean(place.source_id) &&
        (crosswalk.history_source_ids.length === 0 || crosswalk.history_source_ids.includes(place.source_id!)),
      );
    });
    if (matched) seen.add(detail.event.id);
    return matched;
  });
}

/** Only expose exact reviewed event-to-story links with a usable reviewed story. */
export function reviewedEventStories(
  locationEvents: SemanticEventDetail[],
): SemanticEventStory[] {
  const seen = new Set<string>();
  const links: SemanticEventStory[] = [];
  for (const detail of locationEvents) {
    for (const link of detail.related_stories ?? []) {
      if (
        link.event_id !== detail.event.id ||
        link.story_id !== link.story.id ||
        !isReviewedStatus(link.link_quality_status) ||
        !isReviewedStatus(link.story.quality_status) ||
        link.story.usable !== true ||
        seen.has(link.story.id)
      ) continue;
      seen.add(link.story.id);
      links.push(link);
    }
  }
  return links;
}

export function reviewedStoryEvents(events: SemanticStoryEvent[]): SemanticStoryEvent[] {
  return events
    .filter((event) =>
      isReviewedStatus(event.quality_status) &&
      isReviewedStatus(event.event_quality_status),
    )
    .slice()
    .sort((left, right) => {
      const year = (value: number | null) =>
        value == null || value === 0 ? Number.POSITIVE_INFINITY : value;
      const yearDifference = year(left.start_year) - year(right.start_year);
      if (yearDifference !== 0) return yearDifference;
      const sequenceDifference = (left.sequence ?? Number.MAX_SAFE_INTEGER) -
        (right.sequence ?? Number.MAX_SAFE_INTEGER);
      return sequenceDifference || left.event_id.localeCompare(right.event_id);
    });
}

export function formatHistoricalYear(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value) || value === 0) return "年代待考";
  return value < 0 ? `公元前${Math.abs(value)}年` : `公元${value}年`;
}

export function formatHistoricalRange(
  start: number | null | undefined,
  end: number | null | undefined,
  precision?: string | null,
): string {
  const hasStart = start != null && Number.isFinite(start) && start !== 0;
  const hasEnd = end != null && Number.isFinite(end) && end !== 0;
  const approximate = ["approximate", "approx", "circa"].includes(
    (precision ?? "").toLocaleLowerCase(),
  );
  if (!hasStart && !hasEnd) return "年代待考";
  if (!hasStart) return `始于年代待考 · 止于${formatHistoricalYear(end)}`;
  if (!hasEnd || start === end) {
    return `${approximate ? "约" : ""}${formatHistoricalYear(start)}`;
  }
  return `${approximate ? "约" : ""}${formatHistoricalYear(start)} — ${formatHistoricalYear(end)}`;
}

export function sourceIds(value: string | null | undefined): string[] {
  if (!value) return [];
  try {
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed)
      ? parsed.filter((id): id is string => typeof id === "string" && id.length > 0)
      : [];
  } catch {
    return [];
  }
}
