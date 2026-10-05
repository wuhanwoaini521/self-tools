/**
 * 26 周能力路线图（V13 W6）——「半年能交流」这件事的进度表。
 *
 * ## 为什么单独做一个页面
 *
 * 其它页面回答「今天做什么」，这个页面回答**「我在第几周、离能交流还差什么」**。
 * 差别很重要：课时数会让人以为自己在进步，而能不能交流要靠**能力检查**。
 *
 * ## 诚实边界
 *
 * - 每个检查项都显示 `当前值 / 目标值`，不达标就显示不达标；
 * - **没有学习记录就不显示「第几周」** —— 没有起点就没有进度可言
 *   （页面会直接说明，并指向「先去学一课」）；
 * - 标为自评的项（敢不敢说、能不能听懂）不参与自动判定，界面写明「这项你自己填」；
 * - 数字全部来自真实统计（课程进度 / 跟读记录 / 平台复习 / 句子卡），没有估算值。
 */
import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, Flag, Target } from "@phosphor-icons/react";
import { errorMessage } from "../../../utils";
import { englishClient } from "./englishClient";
import type { RoadmapCheckpoint, RoadmapView } from "../roadmapTypes";

export interface RoadmapViewProps {
  onBack: () => void;
  /** 直接开始学（没有记录时给一个下一步，而不是让人对着空计划发呆）。 */
  onStart: () => void;
}

export function RoadmapPage({ onBack, onStart }: RoadmapViewProps) {
  const [view, setView] = useState<RoadmapView | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      setView(await englishClient.roadmap());
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (error) {
    return (
      <div className="en-library">
        <p className="en-inline-error">{error}</p>
        <button type="button" className="en-ghost-btn" onClick={() => void load()}>
          重试
        </button>
      </div>
    );
  }

  if (!view) {
    return (
      <div className="en-library">
        <p className="en-muted">加载中…</p>
      </div>
    );
  }

  const started = view.current_week !== null;
  const metrics = view.metrics;

  return (
    <div className="en-library">
      <header className="en-page-head">
        <div className="en-title-row">
          <button type="button" className="en-icon-btn" onClick={onBack} title="返回">
            <ArrowLeft size={16} />
          </button>
          <div>
            <p className="en-greeting-hello">Roadmap</p>
            <h1>26 周交流路线图</h1>
          </div>
        </div>
      </header>

      <section className="en-card">
        <header className="en-card-head">
          <h3>
            <Target size={15} /> 真实指标
          </h3>
          <span className="en-muted">进度页上的数字都来自这里</span>
        </header>
        <div className="en-metric-row">
          <div className="en-metric">
            <span className="en-metric-label">完成课时</span>
            <strong>{metrics.lessons_completed}</strong>
          </div>
          <div className="en-metric">
            <span className="en-metric-label">开口时长</span>
            <strong>{metrics.spoken_minutes} min</strong>
          </div>
          <div className="en-metric">
            <span className="en-metric-label">复习掌握度</span>
            <strong>{Math.round(metrics.review_mastery)}%</strong>
          </div>
          <div className="en-metric">
            <span className="en-metric-label">句子卡</span>
            <strong>{metrics.sentence_cards}</strong>
          </div>
          <div className="en-metric">
            <span className="en-metric-label">学过的词</span>
            <strong>{metrics.words_learned}</strong>
          </div>
          <div className="en-metric">
            <span className="en-metric-label">连续天数</span>
            <strong>{metrics.streak_days}</strong>
          </div>
        </div>
        {!started ? (
          <p className="en-muted">
            还没有学习记录，所以**看不出第几周** —— 没有起点就没有进度。
            学完第一课后这里会自动开始计时。
          </p>
        ) : (
          <p className="en-muted">
            现在是第 <strong>{view.current_week}</strong> 周
            {view.days_to_next !== null ? ` · 距下一个能力检查还有 ${view.days_to_next} 天` : " · 已到最后一次检查"}
          </p>
        )}
        {!started ? (
          <button type="button" className="en-primary-btn" onClick={onStart}>
            去学第一课
          </button>
        ) : null}
      </section>

      {view.current_checkpoint ? (
        <section className="en-card en-roadmap-current">
          <header className="en-card-head">
            <h3>
              <Flag size={15} /> 第 {view.current_checkpoint.week} 周检查点
            </h3>
            <span className="en-muted">{view.current_checkpoint.focus}</span>
          </header>
          <p className="en-roadmap-can-do">到这一周应该能做到：{view.current_checkpoint.can_do}</p>
          <CheckList checkpoint={view.current_checkpoint} />
        </section>
      ) : null}

      <section className="en-card">
        <header className="en-card-head">
          <h3>全部检查点</h3>
          <span className="en-muted">每 2–4 周一次，共 {view.checkpoints.length} 次</span>
        </header>
        <ol className="en-roadmap-list">
          {view.checkpoints.map((checkpoint) => (
            <li
              key={checkpoint.week}
              className={
                "en-roadmap-item" +
                (checkpoint.complete ? " is-complete" : "") +
                (view.current_checkpoint?.week === checkpoint.week ? " is-current" : "")
              }
            >
              <div className="en-roadmap-week">
                <strong>W{checkpoint.week}</strong>
                <span>{checkpoint.focus}</span>
              </div>
              <p className="en-roadmap-can-do">{checkpoint.can_do}</p>
              <CheckList checkpoint={checkpoint} compact />
            </li>
          ))}
        </ol>
      </section>
    </div>
  );
}

/** 检查项列表：`当前 / 目标`，达标打勾，自评项单独标注。 */
function CheckList({
  checkpoint,
  compact = false,
}: {
  checkpoint: RoadmapCheckpoint;
  compact?: boolean;
}) {
  return (
    <ul className={compact ? "en-roadmap-checks is-compact" : "en-roadmap-checks"}>
      {checkpoint.checks.map((check) => (
        <li key={check.label} className={check.met ? "is-met" : ""}>
          <span className="en-roadmap-mark" aria-hidden="true">
            {check.met ? "✓" : "○"}
          </span>
          <span className="en-roadmap-check-label">{check.label}</span>
          {check.auto ? (
            <span className="en-roadmap-value">
              {check.current} / {check.threshold} {check.unit}
            </span>
          ) : (
            <span className="en-roadmap-self">这项你自己评</span>
          )}
        </li>
      ))}
    </ul>
  );
}
