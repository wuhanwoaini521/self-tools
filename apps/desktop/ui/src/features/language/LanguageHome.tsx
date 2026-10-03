/**
 * Language 首页（Today）。
 *
 * 六个区块各自是一个独立的 `AsyncPanel`：任何一个请求失败只在自己那一块
 * 显示「重试」，绝不会把整页变成空白或整页 spinner。
 *
 * 所有数字都来自后端：
 * - 今日计数 ← `reviewQueue()` / `mistakes()` / `continueLessons()`
 * - 复习分组 ← `reviewQueue()` 的 `is_overdue` / `card.repetition_count`
 * - 薄弱项  ← `weakItems()`
 * - 错题    ← `mistakes()`
 * - 最近活动 ← `progress()` 的 `last_studied_at`
 */
import { useMemo } from "react";
import {
  ArrowRight,
  Brain,
  CheckCircle,
  Clock,
  Lightning,
  Warning,
} from "@phosphor-icons/react";
import type {
  ContinueLesson,
  LearningProgress,
  Mistake,
  ReviewQueueItem,
  WeakItem,
} from "../../types";
import { languageClient } from "./languageClient";
import {
  Chip,
  MasteryBar,
  Panel,
  PanelBody,
  ProgressTrack,
} from "./LanguagePrimitives";
import { MistakesPanel } from "./ReviewSession";
import {
  DIFFICULTY_LABELS,
  EMPTY_COPY,
  STATUS_LABELS,
  formatAgo,
  groupRecentByDay,
  summarizeReviewQueue,
  useAsyncPanel,
} from "./languageUi";

export interface LanguageHomeProps {
  onOpenLesson: (lessonId: string) => void;
  /** 从薄弱项 / 最近活动点开一条内容 → 进入专注模式。 */
  onStudyItem: (entityId: string) => void;
  onStartReview: () => void;
  onOpenMistakes: () => void;
  /** 复习完成 / 错题解决后刷新本页数据。 */
  refreshToken: number;
}

/**
 * 首页。`refreshToken` 由页面在「提交复习 / 解决错题 / 记录学习」后自增，
 * 让所有面板重新拉一次——比逐面板手工刷新更不容易漏。
 */
export function LanguageHome({
  onOpenLesson,
  onStudyItem,
  onStartReview,
  onOpenMistakes,
  refreshToken,
}: LanguageHomeProps) {
  const queue = useAsyncPanel<ReviewQueueItem[]>(
    () => languageClient.reviewQueue(50),
    [refreshToken],
  );
  const mistakes = useAsyncPanel<Mistake[]>(
    () => languageClient.mistakes(20),
    [refreshToken],
  );
  const lessons = useAsyncPanel<ContinueLesson[]>(
    () => languageClient.continueLessons(5),
    [refreshToken],
  );
  const weak = useAsyncPanel<WeakItem[]>(
    () => languageClient.weakItems(8),
    [refreshToken],
  );
  const progress = useAsyncPanel<LearningProgress[]>(
    () => languageClient.progress(30),
    [refreshToken],
  );

  const breakdown = useMemo(
    () => summarizeReviewQueue(queue.data ?? []),
    [queue.data],
  );
  const activity = useMemo(
    () => groupRecentByDay(progress.data ?? [], (row) => row.last_studied_at),
    [progress.data],
  );

  return (
    <div className="lang-home">
      {/* -------------------------------------------------- Today */}
      <Panel
        title="今天"
        hint="来自复习队列、错题与未完成课程"
        actions={
          <>
            <button type="button" className="lang-primary" onClick={onStartReview}>
              <Lightning size={14} /> 开始复习
            </button>
            <button type="button" className="lang-link" onClick={onOpenMistakes}>
              <Warning size={13} /> 错题本
            </button>
          </>
        }
      >
        <ul className="lang-today-stats">
          <li>
            <b>{breakdown.total}</b>
            <span>复习到期</span>
          </li>
          <li>
            <b>{breakdown.overdue}</b>
            <span>其中逾期</span>
          </li>
          <li>
            <b>{breakdown.fresh}</b>
            <span>新卡</span>
          </li>
          <li>
            <b>{mistakes.data?.length ?? 0}</b>
            <span>错题待解决</span>
          </li>
          <li>
            <b>{lessons.data?.length ?? 0}</b>
            <span>课程进行中</span>
          </li>
        </ul>
        <p className="lang-daily-text">
          {breakdown.total > 0
            ? `今天有 ${breakdown.total} 张卡片到期，其中 ${breakdown.fresh} 张是第一次见。空格揭晓答案，1–4 打分。`
            : EMPTY_COPY.review}
        </p>
      </Panel>

      {/* -------------------------------------------------- Continue Learning */}
      <Panel title="继续学习" hint="从中断处接着学">
        <PanelBody
          loading={lessons.loading}
          error={lessons.error}
          reload={lessons.reload}
          empty={lessons.data?.length === 0 ? EMPTY_COPY.continue : null}
        >
          <ul className="lang-continue-list">
            {(lessons.data ?? []).map((entry) => (
              <li key={entry.lesson.id}>
                <div className="lang-continue-main">
                  <b>{entry.lesson.title}</b>
                  <span className="lang-muted">
                    {entry.lesson.description ??
                      `${entry.total_steps} 步课程`}
                  </span>
                  <ProgressTrack
                    value={entry.completed_steps}
                    total={entry.total_steps}
                  />
                  <span className="lang-muted">
                    已完成 {entry.completed_steps} / {entry.total_steps} 步 ·
                    上次学到第 {entry.step_index + 1} 步 ·{" "}
                    {formatAgo(entry.last_studied_at)}
                  </span>
                </div>
                <button
                  type="button"
                  className="lang-primary"
                  onClick={() => onOpenLesson(entry.lesson.id)}
                >
                  继续 <ArrowRight size={14} />
                </button>
              </li>
            ))}
          </ul>
        </PanelBody>
      </Panel>

      {/* -------------------------------------------------- Review */}
      <Panel
        title="复习"
        hint="到期 / 逾期 / 新卡"
        actions={
          <button type="button" className="lang-link" onClick={onStartReview}>
            打开复习 <ArrowRight size={13} />
          </button>
        }
      >
        <PanelBody
          loading={queue.loading}
          error={queue.error}
          reload={queue.reload}
          empty={breakdown.total === 0 ? EMPTY_COPY.review : null}
        >
          <ul className="lang-review-breakdown">
            <li>
              <Chip tone="accent">到期 {breakdown.due}</Chip>
            </li>
            <li>
              <Chip tone="danger">逾期 {breakdown.overdue}</Chip>
            </li>
            <li>
              <Chip>新卡 {breakdown.fresh}</Chip>
            </li>
          </ul>
          <ul className="lang-queue-preview">
            {(queue.data ?? []).slice(0, 5).map((entry) => (
              <li key={entry.card.id}>
                <span>{entry.card.prompt}</span>
                <Chip tone={entry.is_overdue ? "danger" : "plain"}>
                  {entry.is_overdue ? "逾期" : "到期"} · 复习{" "}
                  {entry.card.repetition_count} 次
                </Chip>
              </li>
            ))}
          </ul>
        </PanelBody>
      </Panel>

      {/* -------------------------------------------------- Weak Items */}
      <Panel title="薄弱项" hint="掌握度偏低、且最近学过">
        <PanelBody
          loading={weak.loading}
          error={weak.error}
          reload={weak.reload}
          empty={weak.data?.length === 0 ? EMPTY_COPY.weak : null}
        >
          <ul className="lang-weak-list">
            {(weak.data ?? []).map((item) => (
              <li key={item.entity_id}>
                <button
                  type="button"
                  className="lang-row-button"
                  onClick={() => onStudyItem(item.entity_id)}
                >
                  <span className="lang-row-main">
                    <b>{item.content}</b>
                    <small>{item.translation ?? "暂无译文"}</small>
                  </span>
                  <span className="lang-row-meta">
                    <MasteryBar score={item.mastery_score} />
                    <Chip tone={item.difficulty === "hard" ? "danger" : "plain"}>
                      {DIFFICULTY_LABELS[item.difficulty]}
                    </Chip>
                    <Chip>{STATUS_LABELS[item.status]}</Chip>
                    <Chip tone="warn">错 {item.incorrect_count} 次</Chip>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </PanelBody>
      </Panel>

      {/* -------------------------------------------------- Mistakes */}
      <Panel
        title="需要重做的题"
        hint="答错过的内容"
        actions={
          <button type="button" className="lang-link" onClick={onOpenMistakes}>
            全部错题 <ArrowRight size={13} />
          </button>
        }
      >
        <PanelBody
          loading={mistakes.loading}
          error={mistakes.error}
          reload={mistakes.reload}
          empty={mistakes.data?.length === 0 ? EMPTY_COPY.mistakes : null}
        >
          <div className="lang-mistake-intro">
            <p className="lang-muted">
              「Got it」会对这条内容对应的复习卡提交一次正确复习，由后端清除错题记录——
              不会在前端把这一行删掉。
            </p>
            <button
              type="button"
              className="lang-link"
              onClick={() => onStudyItem(mistakes.data?.[0]?.item_id ?? "")}
              disabled={!mistakes.data || mistakes.data.length === 0}
            >
              <Brain size={13} /> 专注复习第一条
            </button>
          </div>
          <MistakesPanel
            mistakes={mistakes.data ?? []}
            queue={queue.data ?? []}
            error={null}
            reload={mistakes.reload}
          />
        </PanelBody>
      </Panel>

      {/* -------------------------------------------------- Recent Activity */}
      <Panel title="最近活动" hint="今天 / 昨天 / 本周">
        <PanelBody
          loading={progress.loading}
          error={progress.error}
          reload={progress.reload}
          empty={activity.length === 0 ? EMPTY_COPY.activity : null}
        >
          <div className="lang-activity">
            {activity.map((group) => (
              <section key={group.key}>
                <h4>
                  {group.key === "today" ? (
                    <Clock size={13} />
                  ) : (
                    <CheckCircle size={13} />
                  )}{" "}
                  {group.label}
                </h4>
                <ul>
                  {group.items.map((row) => (
                    <li key={row.entity_key}>
                      <button
                        type="button"
                        className="lang-row-button"
                        onClick={() => onStudyItem(row.entity_id)}
                      >
                        <span className="lang-row-main">
                          <b>{row.entity_title || row.entity_id}</b>
                          <small>{formatAgo(row.last_studied_at)}</small>
                        </span>
                        <span className="lang-row-meta">
                          <MasteryBar score={row.mastery_score} />
                          <Chip>{STATUS_LABELS[row.status]}</Chip>
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            ))}
          </div>
        </PanelBody>
      </Panel>
    </div>
  );
}
