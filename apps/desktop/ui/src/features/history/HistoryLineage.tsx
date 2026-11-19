import { useEffect, useRef } from "react";
import type {
  SemanticPeriod,
  SemanticPeriodDetail,
  SemanticPeriodEvent,
  SemanticPeriodStage,
} from "./semanticTypes";
import {
  PERIOD_ERA_GROUPS,
  eventsForStage,
  rangeText,
} from "./periodDetailHelpers";
import "./lineage.css";

export function groupPeriodsByEra(periods: SemanticPeriod[]) {
  const byId = new Map(periods.map((period) => [period.id, period]));
  const grouped = PERIOD_ERA_GROUPS.map((group) => ({
    label: group.label,
    periods: group.ids
      .map((id) => byId.get(id))
      .filter((period): period is SemanticPeriod => Boolean(period)),
  })).filter((group) => group.periods.length);
  const used = new Set(grouped.flatMap((group) => group.periods.map((period) => period.id)));
  const rest = periods.filter((period) => !used.has(period.id));
  if (rest.length) grouped.push({ label: "其他", periods: rest });
  return grouped;
}

export function HistoryLineage({
  periods,
  selectedId,
  openStageIndex,
  lineageOpen,
  lineageLoading,
  details,
  onSelectPeriod,
  onSelectStage,
  onOpenEvent,
  onToggleLineage,
  onOpenPerson,
}: {
  periods: SemanticPeriod[];
  selectedId: string;
  openStageIndex: number | null;
  lineageOpen: boolean;
  lineageLoading: boolean;
  details: Record<string, SemanticPeriodDetail>;
  onSelectPeriod: (id: string) => void;
  onSelectStage: (periodId: string, stageIndex: number) => void;
  onOpenEvent: (id: string) => void;
  onToggleLineage: () => void;
  onOpenPerson: (id: string) => void;
}) {
  const eras = groupPeriodsByEra(periods);
  const selected = periods.find((period) => period.id === selectedId) ?? periods[0];
  const selectedDetail = selected ? details[selected.id] : undefined;
  const nodeRefs = useRef<Record<string, HTMLButtonElement | null>>({});

  useEffect(() => {
    if (!selectedId) return;
    nodeRefs.current[selectedId]?.scrollIntoView({
      behavior: "smooth",
      inline: "center",
      block: "nearest",
    });
  }, [selectedId]);

  return (
    <section className="history-lineage" aria-label="历史时间线脉络">
      <div className="history-lineage-toolbar">
        <button
          type="button"
          className="history-lineage-toggle sketch-raw"
          onClick={onToggleLineage}
          disabled={lineageLoading}
        >
          {lineageLoading ? "展开中…" : lineageOpen ? "收起脉络" : "展开脉络"}
        </button>
        <p>
          {lineageOpen
            ? "全部朝代的阶段已沿时间线展开。点阶段可看事件。"
            : "沿轴线滑动。点朝代展开阶段，再点阶段看事件。"}
        </p>
      </div>
      <div className="history-lineage-scroll">
        <div className="history-lineage-track">
          <div className="history-lineage-spine" aria-hidden="true" />
          <div className="history-lineage-eras">
            {eras.map((era) => (
              <div key={era.label} className="history-lineage-era">
                <div className="history-lineage-era-label">{era.label}</div>
                <div className="history-lineage-rail">
                  {era.periods.map((period) => {
                    const detail = details[period.id];
                    const open = lineageOpen || period.id === selectedId;
                    const stages = detail?.stages ?? [];
                    const fallbackEvents = !stages.length ? detail?.events ?? [] : [];
                    return (
                      <div
                        key={period.id}
                        className={`history-lineage-period${period.id === selectedId ? " is-current" : ""}${open && detail ? " is-open" : ""}`}
                      >
                        <button
                          type="button"
                          className="history-lineage-node sketch-raw"
                          ref={(node) => {
                            nodeRefs.current[period.id] = node;
                          }}
                          onClick={() => onSelectPeriod(period.id)}
                          aria-current={period.id === selectedId ? "true" : undefined}
                        >
                          <strong>{period.name_zh_cn}</strong>
                          <i className="history-lineage-dot" aria-hidden="true" />
                          <small>{rangeText(period.start_year, period.end_year)}</small>
                        </button>
                        {open && detail ? (
                          <div className="history-lineage-branch">
                            {stages.length
                              ? stages.map((stage) => (
                                  <StageBlock
                                    key={`${period.id}-${stage.index}`}
                                    periodId={period.id}
                                    stage={stage}
                                    events={eventsForStage(detail.events, stage)}
                                    current={
                                      period.id === selectedId &&
                                      openStageIndex === stage.index
                                    }
                                    showEvents={
                                      period.id === selectedId &&
                                      openStageIndex === stage.index
                                    }
                                    onSelectStage={onSelectStage}
                                    onOpenEvent={onOpenEvent}
                                  />
                                ))
                              : fallbackEvents.map((event) => (
                                  <button
                                    key={event.id}
                                    type="button"
                                    className="history-lineage-event sketch-raw"
                                    onClick={() => onOpenEvent(event.id)}
                                  >
                                    <strong>{event.name_zh_cn}</strong>
                                    <span>
                                      {rangeText(event.start_year, event.end_year)}
                                    </span>
                                  </button>
                                ))}
                          </div>
                        ) : open && !detail ? (
                          <div className="history-lineage-branch">
                            <span className="history-lineage-pending">整理阶段…</span>
                          </div>
                        ) : null}
                      </div>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
      {selected ? (
        <div className="history-lineage-focus">
          <div className="history-lineage-focus-kicker">当前朝代</div>
          <h2>{selected.name_zh_cn}</h2>
          <p className="history-lineage-focus-years">
            {rangeText(selected.start_year, selected.end_year)}
            {selected.event_count ? ` · ${selected.event_count} 事件` : ""}
          </p>
          {selected.description_zh_cn ? (
            <p className="history-lineage-focus-intro">{selected.description_zh_cn}</p>
          ) : null}
          {selectedDetail?.people.length ? (
            <div className="history-lineage-people">
              {selectedDetail.people.slice(0, 8).map((person) => (
                <button
                  key={person.person_id}
                  type="button"
                  className="sketch-raw"
                  onClick={() => onOpenPerson(person.person_id)}
                >
                  {person.canonical_name_zh_cn}
                </button>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}

function StageBlock({
  periodId,
  stage,
  events,
  current,
  showEvents,
  onSelectStage,
  onOpenEvent,
}: {
  periodId: string;
  stage: SemanticPeriodStage;
  events: SemanticPeriodEvent[];
  current: boolean;
  showEvents: boolean;
  onSelectStage: (periodId: string, stageIndex: number) => void;
  onOpenEvent: (id: string) => void;
}) {
  return (
    <div className="history-lineage-stage-wrap">
      <button
        type="button"
        className={`history-lineage-stage sketch-raw${current ? " is-current" : ""}`}
        onClick={() => onSelectStage(periodId, stage.index)}
      >
        <strong>{stage.opening_event_name || `阶段 ${stage.index + 1}`}</strong>
        <span>
          {rangeText(stage.start_year, stage.end_year)}
          {stage.event_count ? ` · ${stage.event_count} 事` : ""}
        </span>
      </button>
      {showEvents && events.length ? (
        <div className="history-lineage-events">
          {events.map((event) => (
            <button
              key={event.id}
              type="button"
              className="history-lineage-event sketch-raw"
              onClick={() => onOpenEvent(event.id)}
            >
              <strong>{event.name_zh_cn}</strong>
              <span>{rangeText(event.start_year, event.end_year)}</span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
