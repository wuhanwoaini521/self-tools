import {
  ArrowRight,
  BookOpen,
  Clock,
  MapPin,
  SealCheck,
  WarningCircle,
} from "@phosphor-icons/react";
import { useEffect, useMemo, useState } from "react";
import type { GeoEntity } from "../../types";
import { errorMessage } from "../../utils";
import { historyClient } from "../history/historyClient";
import type {
  SemanticEventDetail,
  SemanticEventEvidence,
  SemanticEventStory,
  SemanticStoryDetail,
  SemanticStoryEvent,
} from "../history/semanticTypes";
import type { GeoHistoryCrosswalk, LocationHistoryContext } from "../exploration/explorationClient";
import {
  formatHistoricalRange,
  isReviewedStatus,
  reviewedEventStories,
  reviewedLocationEvents,
  reviewedStoryEvents,
  sourceIds,
} from "./geoTimeline";

interface LocationHistoryTimelineProps {
  entity: GeoEntity;
  context: LocationHistoryContext | null;
  crosswalks: GeoHistoryCrosswalk[];
  onSelectionChange?: (selection: {
    eventId: string;
    storyId: string | null;
    year: number | null;
  } | null) => void;
  onOpenHistoryEvent?: (eventId: string) => void;
  onOpenHistoryStory?: (storyId: string) => void;
  onAddToTravel?: (entity: GeoEntity, event?: SemanticEventDetail) => void;
}

interface TimelineItem {
  id: string;
  name: string;
  startYear: number | null;
  endYear: number | null;
  precision: string | null;
  summary: string | null;
  sequence: number | null;
  sourceIds: string | null;
  directDetail: SemanticEventDetail | null;
}

function chronological(items: TimelineItem[]): TimelineItem[] {
  return items.slice().sort((left, right) => {
    const year = (value: number | null) =>
      value == null || value === 0 ? Number.POSITIVE_INFINITY : value;
    const yearDifference = year(left.startYear) - year(right.startYear);
    if (yearDifference !== 0) return yearDifference;
    const sequenceDifference =
      (left.sequence ?? Number.MAX_SAFE_INTEGER) -
      (right.sequence ?? Number.MAX_SAFE_INTEGER);
    return sequenceDifference || left.id.localeCompare(right.id);
  });
}

function evidenceForSelection(
  selectedEventId: string | null,
  directDetail: SemanticEventDetail | null,
  storyDetail: SemanticStoryDetail | null,
): SemanticEventEvidence[] {
  const evidence = [
    ...(directDetail?.evidences ?? []),
    ...(storyDetail?.evidences.filter((item) => item.event_id === selectedEventId) ?? []),
  ];
  const seen = new Set<string>();
  return evidence.filter((item) => {
    if (
      item.event_id !== selectedEventId ||
      item.link_status !== "linked" ||
      !isReviewedStatus(item.quality_status) ||
      !isReviewedStatus(item.link_quality_status) ||
      !item.source_id ||
      seen.has(item.id)
    ) return false;
    seen.add(item.id);
    return true;
  });
}

function evidenceLabel(item: SemanticEventEvidence): string {
  return [item.work, item.term ?? item.chapter_hint, item.source_id]
    .filter((part): part is string => Boolean(part))
    .join(" · ");
}

export function LocationHistoryTimeline({
  entity,
  context,
  crosswalks,
  onSelectionChange,
  onOpenHistoryEvent,
  onOpenHistoryStory,
  onAddToTravel,
}: LocationHistoryTimelineProps) {
  const locationEvents = useMemo(
    () => reviewedLocationEvents(context?.events ?? [], crosswalks),
    [context?.events, crosswalks],
  );
  const storyLinks = useMemo(() => reviewedEventStories(locationEvents), [locationEvents]);
  const [selectedStoryId, setSelectedStoryId] = useState<string | null>(null);
  const [selectedEventId, setSelectedEventId] = useState<string | null>(null);
  const [storyDetail, setStoryDetail] = useState<SemanticStoryDetail | null>(null);
  const [storyLoading, setStoryLoading] = useState(false);
  const [storyError, setStoryError] = useState<string | null>(null);
  const [retryCount, setRetryCount] = useState(0);

  useEffect(() => {
    if (!storyLinks.some((link) => link.story.id === selectedStoryId)) {
      setSelectedStoryId(storyLinks[0]?.story.id ?? null);
    }
  }, [selectedStoryId, storyLinks]);

  const activeStoryLink: SemanticEventStory | null =
    storyLinks.find((link) => link.story.id === selectedStoryId) ?? storyLinks[0] ?? null;
  const activeStoryId = activeStoryLink?.story.id ?? null;

  useEffect(() => {
    if (!activeStoryId) {
      setStoryDetail(null);
      setStoryError(null);
      setStoryLoading(false);
      return;
    }
    let cancelled = false;
    setStoryDetail(null);
    setStoryError(null);
    setStoryLoading(true);
    void historyClient.storyDetail(activeStoryId).then(
      (detail) => {
        if (cancelled) return;
        if (!detail || detail.story.id !== activeStoryId) {
          setStoryError("History 暂未返回这条已审核的故事记录；已保留地点事件。");
          return;
        }
        setStoryDetail(detail);
      },
      (error: unknown) => {
        if (!cancelled) setStoryError(errorMessage(error));
      },
    ).finally(() => {
      if (!cancelled) setStoryLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [activeStoryId, retryCount]);

  const activeStoryDetail = storyDetail?.story.id === activeStoryId ? storyDetail : null;
  const reviewedStoryNodes = useMemo(
    () => reviewedStoryEvents(activeStoryDetail?.events ?? [])
      .filter((event) => event.story_id === activeStoryId),
    [activeStoryDetail, activeStoryId],
  );
  const timelineItems = useMemo(() => {
    const directById = new Map(locationEvents.map((detail) => [detail.event.id, detail]));
    const fromStory: TimelineItem[] = reviewedStoryNodes.map((event: SemanticStoryEvent) => ({
      id: event.event_id,
      name: event.name_zh_cn,
      startYear: event.start_year,
      endYear: event.end_year,
      precision: event.date_precision ?? null,
      summary: event.summary_zh_cn ?? event.result_zh_cn ?? null,
      sequence: event.sequence,
      sourceIds: event.source_ids ?? null,
      directDetail: directById.get(event.event_id) ?? null,
    }));
    const included = new Set(fromStory.map((event) => event.id));
    const remainingLocationEvents: TimelineItem[] = locationEvents
      .filter((detail) => !included.has(detail.event.id))
      .map((detail) => ({
        id: detail.event.id,
        name: detail.event.name_zh_cn,
        startYear: detail.event.start_year,
        endYear: detail.event.end_year,
        precision: detail.event.date_precision ?? null,
        summary: detail.event.summary_zh_cn ?? detail.event.result_zh_cn ?? null,
        sequence: null,
        sourceIds: detail.event.source_ids ?? null,
        directDetail: detail,
      }));
    return chronological([...fromStory, ...remainingLocationEvents]);
  }, [locationEvents, reviewedStoryNodes]);

  useEffect(() => {
    const anchor = locationEvents.find((detail) =>
      !activeStoryId || detail.related_stories?.some((link) => link.story_id === activeStoryId),
    );
    const fallbackId = anchor?.event.id ?? timelineItems[0]?.id ?? null;
    if (!timelineItems.some((event) => event.id === selectedEventId)) {
      setSelectedEventId(fallbackId);
    }
  }, [activeStoryId, locationEvents, selectedEventId, timelineItems]);

  const selectedItem = timelineItems.find((event) => event.id === selectedEventId) ?? null;
  const selectedLocationEvent = selectedItem?.directDetail ?? null;
  const selectedStoryIdForEvent = selectedItem && activeStoryLink &&
    (activeStoryDetail?.events.some((event) => event.event_id === selectedItem.id) ?? false)
    ? activeStoryLink.story.id
    : null;
  const selectedEvidence = useMemo(
    () => evidenceForSelection(selectedEventId, selectedLocationEvent, activeStoryDetail),
    [activeStoryDetail, selectedEventId, selectedLocationEvent],
  );
  const allSources = [
    ...(activeStoryDetail?.sources ?? []),
    ...(selectedLocationEvent?.sources ?? []),
  ];
  const sourceById = new Map(allSources.map((source) => [source.id, source]));
  const selectedSourceIds = Array.from(new Set([
    ...sourceIds(selectedItem?.sourceIds),
    ...selectedEvidence.flatMap((evidence) => evidence.source_id ? [evidence.source_id] : []),
    ...(selectedLocationEvent?.sources.map((source) => source.id) ?? []),
  ]));
  const currentPlace = selectedLocationEvent?.places.find((place) =>
    crosswalks.some((crosswalk) =>
      crosswalk.status === "reviewed" &&
      crosswalk.history_place_id === place.place_id &&
      place.link_status === "linked" &&
      isReviewedStatus(place.quality_status) &&
      isReviewedStatus(place.link_quality_status),
    ),
  ) ?? null;

  useEffect(() => {
    if (!onSelectionChange) return;
    if (!selectedItem) {
      onSelectionChange(null);
      return;
    }
    onSelectionChange({
      eventId: selectedItem.id,
      storyId: selectedStoryIdForEvent,
      year: selectedItem.startYear && selectedItem.startYear !== 0
        ? selectedItem.startYear
        : null,
    });
  }, [onSelectionChange, selectedItem, selectedStoryIdForEvent]);

  if (!locationEvents.length) return null;

  return (
    <section className="geo-history-timeline" aria-labelledby="geo-history-timeline-title">
      <header className="geo-history-timeline-head">
        <div>
          <span className="geo-section-label">HISTORY · REVIEWED TIMELINE</span>
          <h4 id="geo-history-timeline-title">
            {activeStoryLink?.story.title_zh_cn ?? `${entity.name}的已审核地点事件`}
          </h4>
          <p>
            {activeStoryLink
              ? `${formatHistoricalRange(activeStoryLink.story.start_year, activeStoryLink.story.end_year, "range")} · 故事关联与事件记录均已审核`
              : "按稳定地点标识展示；未标注的年代保持待考。"}
          </p>
        </div>
        {activeStoryLink ? (
          <div className="geo-history-timeline-story-actions">
            <span><SealCheck size={14} /> 事件—故事关系已审核</span>
            {onOpenHistoryStory ? (
              <button type="button" onClick={() => onOpenHistoryStory(activeStoryLink.story.id)}>
                <BookOpen size={14} /> 在 History 阅读故事
              </button>
            ) : null}
          </div>
        ) : null}
      </header>

      {storyLinks.length > 1 ? (
        <label className="geo-history-story-picker">
          <span>切换已审核故事</span>
          <select
            value={activeStoryLink?.story.id ?? ""}
            onChange={(event) => setSelectedStoryId(event.target.value)}
          >
            {storyLinks.map((link) => (
              <option key={link.story.id} value={link.story.id}>
                {link.story.title_zh_cn} · {formatHistoricalRange(link.story.start_year, link.story.end_year, "range")}
              </option>
            ))}
          </select>
        </label>
      ) : null}

      {storyLoading ? (
        <p className="geo-history-timeline-status" role="status">
          <Clock size={15} /> 正在载入已审核的故事时间线…
        </p>
      ) : null}
      {storyError ? (
        <div className="geo-history-timeline-error" role="alert">
          <WarningCircle size={16} />
          <span>{storyError}</span>
          <button type="button" onClick={() => setRetryCount((count) => count + 1)}>重试</button>
        </div>
      ) : null}

      {timelineItems.length ? (
        <ol className="geo-history-timeline-list" aria-label={`${entity.name}历史时间线`}>
          {timelineItems.map((item) => {
            const active = item.id === selectedEventId;
            const placeAnchor = Boolean(item.directDetail);
            return (
              <li key={item.id}>
                <button
                  type="button"
                  className={`geo-history-timeline-entry${active ? " selected" : ""}${placeAnchor ? " place-anchor" : ""}`}
                  aria-pressed={active}
                  onClick={() => setSelectedEventId(item.id)}
                >
                  <span className="geo-history-timeline-date">
                    {formatHistoricalRange(item.startYear, item.endYear, item.precision)}
                  </span>
                  <span className="geo-history-timeline-marker" aria-hidden="true" />
                  <span className="geo-history-timeline-copy">
                    <strong>{item.name}</strong>
                    <small>
                      {placeAnchor
                        ? "此事件与当前地点存在已审核的稳定 ID 关联"
                        : "已审核的故事时间线节点；不代表该事件发生于当前地点"}
                    </small>
                    {item.summary ? <span className="geo-history-timeline-summary">{item.summary}</span> : null}
                  </span>
                </button>
              </li>
            );
          })}
        </ol>
      ) : !storyLoading && !storyError ? (
        <p className="geo-history-timeline-status" role="status">
          故事中没有可展示的已审核事件；仍保留上方稳定地点关联与来源依据。
        </p>
      ) : null}

      {selectedItem ? (
        <article className="geo-history-timeline-selected" aria-live="polite">
          <div className="geo-history-selected-date">
            <Clock size={14} />
            <span>{formatHistoricalRange(selectedItem.startYear, selectedItem.endYear, selectedItem.precision)}</span>
            {isReviewedStatus(selectedLocationEvent?.event.quality_status) ? (
              <span className="geo-history-reviewed-badge">History 已审核</span>
            ) : null}
          </div>
          <h5>{selectedItem.name}</h5>
          {selectedItem.summary ? <p>{selectedItem.summary}</p> : <p>该事件暂无摘要。</p>}
          {currentPlace ? (
            <p className="geo-history-place-proof">
              <MapPin size={14} /> Geography <code>{entity.id}</code> ↔ History place <code>{currentPlace.place_id}</code>
              {currentPlace.source_id ? ` · 来源 ${currentPlace.source_id}` : ""}
            </p>
          ) : !selectedLocationEvent ? (
            <p className="geo-history-place-caveat">
              这是同一已审核故事中的关联事件；当前数据没有证明它与 {entity.name} 是同一地点。
            </p>
          ) : null}

          {selectedEvidence.length ? (
            <details className="geo-history-evidence-index">
              <summary>史料索引 · {selectedEvidence.length} 条已审核关联</summary>
              <ul>
                {selectedEvidence.map((evidence) => (
                  <li key={evidence.id}>
                    <strong>{evidenceLabel(evidence) || "已审核史料记录"}</strong>
                    {evidence.review_note ? <small>{evidence.review_note}</small> : null}
                  </li>
                ))}
              </ul>
            </details>
          ) : selectedSourceIds.length ? (
            <p className="geo-history-source-ids">
              来源记录：{selectedSourceIds.map((id) => sourceById.get(id)?.dataset ?? id).join(" · ")}
            </p>
          ) : (
            <p className="geo-history-source-ids">本地资料中暂无可展示的来源记录。</p>
          )}

          <div className="geo-history-selected-actions">
            {onOpenHistoryEvent ? (
              <button type="button" onClick={() => onOpenHistoryEvent(selectedItem.id)}>
                打开 History 事件详情 <ArrowRight size={14} />
              </button>
            ) : null}
            {activeStoryLink && onOpenHistoryStory ? (
              <button type="button" className="secondary" onClick={() => onOpenHistoryStory(activeStoryLink.story.id)}>
                打开故事详情 <ArrowRight size={14} />
              </button>
            ) : null}
            {selectedLocationEvent && entity.entity_type === "city" && onAddToTravel ? (
              <button type="button" className="travel" onClick={() => onAddToTravel(entity, selectedLocationEvent)}>
                将 {entity.name} 与此已审核事件加入 Travel <ArrowRight size={14} />
              </button>
            ) : null}
          </div>
        </article>
      ) : null}
    </section>
  );
}
