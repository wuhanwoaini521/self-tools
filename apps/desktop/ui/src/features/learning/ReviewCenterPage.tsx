import React, { useEffect, useState, useCallback, useMemo, useRef } from "react";
import {
  Brain,
  CheckCircle,
  Clock,
  Sparkle,
  ArrowRight,
  ArrowSquareOut,
  ArrowsCounterClockwise,
  BookOpen,
  Globe,
  Translate,
  Lightbulb,
  Check,
  X,
  Cards,
  Flame,
} from "@phosphor-icons/react";
import { learningClient } from "./learningClient";
import { matchAnswer, matchFeedback } from "../language/reviewMatch";
import type {
  LearningProgress,
  ReviewQueueItem,
  ReviewQueueStats,
  ReviewScheduleOutcome,
  UniversalReviewCard,
  UniversalReviewRating,
} from "../../types";
import { sourceRoute } from "./sourceNavigation";
import "./reviewCenter.css";

interface ReviewCenterPageProps {
  active: boolean;
  onNavigate?: (route: string) => void;
  onAskAi?: (prompt: string) => void;
}

/** 需要用户**写出来**的卡型（其余是选择题/回忆提示，直接揭晓即可）。 */
function needsTypedAnswer(card: { card_type: string } | null): boolean {
  if (!card) return false;
  return card.card_type === "fill_blank" || card.card_type === "qa";
}

function newReviewRequestId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `review-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function progressStatusLabel(status: LearningProgress["status"]): string {
  return {
    not_started: "未开始",
    learning: "学习中",
    familiar: "较熟悉",
    mastered: "已掌握",
  }[status];
}

function formatDueAt(timestamp: number): string {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(timestamp * 1000));
}

interface ReviewReceipt {
  card: UniversalReviewCard;
  outcome: ReviewScheduleOutcome;
  progress: LearningProgress | null;
}

const MODULE_NAMES: Record<string, { label: string; icon: React.ReactNode; color: string }> = {
  language: { label: "语言学习", icon: <Translate size={16} />, color: "var(--accent-blue, #3b82f6)" },
  history: { label: "历史时空", icon: <Clock size={16} />, color: "var(--accent-amber, #f59e0b)" },
  geography: { label: "地理百科", icon: <Globe size={16} />, color: "var(--accent-emerald, #10b981)" },
  study: { label: "专题研习", icon: <BookOpen size={16} />, color: "var(--accent-purple, #8b5cf6)" },
  news: { label: "深度阅读", icon: <Lightbulb size={16} />, color: "var(--accent-rose, #f43f5e)" },
};

export function ReviewCenterPage({ active, onNavigate, onAskAi }: ReviewCenterPageProps) {
  const [stats, setStats] = useState<ReviewQueueStats | null>(null);
  const [queue, setQueue] = useState<ReviewQueueItem[]>([]);
  const [selectedModule, setSelectedModule] = useState<string>("all");
  const [currentIndex, setCurrentIndex] = useState<number>(0);
  const [isAnswerRevealed, setIsAnswerRevealed] = useState<boolean>(false);
  /** 是否已展示提示（提示 ≠ 答案：给线索但保留答案）。 */
  const [isHintRevealed, setIsHintRevealed] = useState<boolean>(false);
  const [selectedOption, setSelectedOption] = useState<string | null>(null);
  // 主观作答（填空 / 问答 / 句子卡）：写了才算「主动回忆」，只看答案不算。
  const [typedAnswer, setTypedAnswer] = useState<string>("");
  const [loading, setLoading] = useState<boolean>(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState<boolean>(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [pendingRating, setPendingRating] = useState<UniversalReviewRating | null>(null);
  const [submissionId, setSubmissionId] = useState(newReviewRequestId);
  const [lastReceipt, setLastReceipt] = useState<ReviewReceipt | null>(null);
  const [sessionCompletedCount, setSessionCompletedCount] = useState<number>(0);
  const submittingRef = useRef(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    setLoadError(null);
    try {
      const [newStats, newQueue] = await Promise.all([
        learningClient.getReviewStats(),
        learningClient.getReviewQueue(selectedModule === "all" ? undefined : selectedModule, 50),
      ]);
      setStats(newStats);
      setQueue(newQueue);
      setCurrentIndex(0);
      setIsAnswerRevealed(false);
      setIsHintRevealed(false);
      setSelectedOption(null);
      setTypedAnswer("");
      setSubmitError(null);
      setPendingRating(null);
      setSubmissionId(newReviewRequestId());
    } catch (err) {
      setStats(null);
      setQueue([]);
      setLoadError(err instanceof Error ? err.message : "复习队列暂时无法读取，请重试。");
      console.error("Failed to load review center data:", err);
    } finally {
      setLoading(false);
    }
  }, [selectedModule]);

  useEffect(() => {
    if (active) void loadData();
  }, [active, loadData]);

  const currentItem = queue[currentIndex] ?? null;
  // `ReviewQueueItem` 是 `{ card, is_overdue, urgency_score }`（后端嵌套形状），
  // 此前这里当成扁平卡片读 `.card_id` / `.prompt`，运行时全部为 undefined。
  const currentCard = currentItem?.card ?? null;
  const currentSourceRoute = currentCard
    ? sourceRoute(currentCard.module, currentCard.entity_type, currentCard.entity_id)
    : null;
  // 作答与参考答案的**词级**比对：句子卡只差一个冠词不该判全错
  // （全等判定会把「主动回忆」变成纯挫败）。
  const typedMatch = useMemo(
    () => matchAnswer(typedAnswer, currentCard?.answer ?? ""),
    [typedAnswer, currentCard],
  );

  const handleRating = async (rating: UniversalReviewRating) => {
    if (
      !currentCard ||
      submittingRef.current ||
      (pendingRating !== null && pendingRating !== rating)
    ) return;
    const reviewedCard = currentCard;
    const requestId = submissionId;
    submittingRef.current = true;
    setSubmitting(true);
    setPendingRating(rating);
    setSubmitError(null);
    try {
      const outcome = await learningClient.submitReview(reviewedCard.id, rating, requestId);
      const entityKey = `${reviewedCard.module}:${reviewedCard.entity_type}:${reviewedCard.entity_id}`;
      const [progress, refreshedStats] = await Promise.all([
        learningClient.getProgress(entityKey).catch(() => null),
        learningClient.getReviewStats().catch(() => null),
      ]);
      setLastReceipt({ card: reviewedCard, outcome, progress });
      if (refreshedStats) setStats(refreshedStats);
      setSessionCompletedCount((count) => count + 1);
      setPendingRating(null);
      setSubmissionId(newReviewRequestId());

      if (currentIndex + 1 < queue.length) {
        setCurrentIndex((index) => index + 1);
        setIsAnswerRevealed(false);
        setIsHintRevealed(false);
        setSelectedOption(null);
        setTypedAnswer("");
      } else {
        setCurrentIndex(queue.length);
      }
    } catch (err) {
      setSubmitError(
        `评分尚未确认保存。可重试同一评分；重复请求会安全去重。${err instanceof Error ? ` ${err.message}` : ""}`,
      );
      console.error("Failed to submit review rating:", err);
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  // Keyboard navigation
  useEffect(() => {
    if (!active) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      if (!currentCard) return;

      if (e.code === "Space") {
        e.preventDefault();
        setIsAnswerRevealed((prev) => !prev);
      } else if (isAnswerRevealed) {
        if (e.key === "1") handleRating("again");
        else if (e.key === "2") handleRating("hard");
        else if (e.key === "3") handleRating("good");
        else if (e.key === "4") handleRating("easy");
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [active, currentCard, isAnswerRevealed, submitting, pendingRating, submissionId]);

  return (
    <div className="review-center-page page-shell">
      {/* 统一页面骨架：位置感（模块名 + 标题 + 说明）与操作位与其他页面对齐 */}
      <header className="page-shell-head">
        <div className="page-shell-title">
          <span className="page-shell-eyebrow">review</span>
          <h1>通用复习中心</h1>
          <p className="page-shell-desc">跨历史、地理、语言、专题研习的全模块智能复习卡片</p>
        </div>
        <div className="page-shell-actions">
          <span className="ui-chip is-accent">
            <Cards size={13} /> SRS 间隔重复
          </span>
          <button
            type="button"
            className="ui-btn"
            onClick={loadData}
            disabled={loading}
          >
            <ArrowsCounterClockwise size={15} className={loading ? "spin" : ""} />
            刷新队列
          </button>
        </div>
      </header>

      {/* Stats bar */}
      {stats && (
        <div className="review-center-metrics">
          <div className="review-center-metric">
            <span className="review-metric-label">今日待复习</span>
            <strong
              className="review-metric-value"
              data-tone={(stats.due_count ?? stats.total_due ?? 0) > 0 ? "alert" : "done"}
            >
              {stats.due_count ?? stats.total_due ?? 0}
              <small>张</small>
            </strong>
          </div>
          <div className="review-center-metric">
            <span className="review-metric-label">已掌握知识点</span>
            <strong className="review-metric-value">
              {stats.mastered_count ?? 0}
              <small>个</small>
            </strong>
          </div>
          <div className="review-center-metric">
            <span className="review-metric-label">正在学习中</span>
            <strong className="review-metric-value">
              {stats.learning_count ?? 0}
              <small>个</small>
            </strong>
          </div>
          <div className="review-center-metric">
            <span className="review-metric-label">本次已完成</span>
            <strong className="review-metric-value">
              {sessionCompletedCount}
              <small>张</small>
            </strong>
          </div>
        </div>
      )}

      {/* Module filter tabs */}
      <div className="review-module-tabs" role="tablist" aria-label="复习模块">
        <button
          type="button"
          role="tab"
          aria-selected={selectedModule === "all"}
          className={"review-module-tab" + (selectedModule === "all" ? " is-active" : "")}
          onClick={() => setSelectedModule("all")}
        >
          全部模块 ({stats?.due_count ?? stats?.total_due ?? 0})
        </button>
        {Object.entries(MODULE_NAMES).map(([modKey, modMeta]) => {
          const count = stats?.by_module?.[modKey] ?? 0;
          return (
            <button
              key={modKey}
              type="button"
              role="tab"
              aria-selected={selectedModule === modKey}
              className={"review-module-tab" + (selectedModule === modKey ? " is-active" : "")}
              onClick={() => setSelectedModule(modKey)}
            >
              {modMeta.icon}
              {modMeta.label} ({count})
            </button>
          );
        })}
      </div>

      {lastReceipt ? (
        <section className="review-receipt" role="status" aria-live="polite">
          <div className="review-receipt-copy">
            <strong>上次复习已保存</strong>
            <span>
              {lastReceipt.outcome.interval_days === 0
                ? "可立即再次复习"
                : `下次复习约 ${lastReceipt.outcome.interval_days} 天后 · ${formatDueAt(lastReceipt.outcome.due_at)}`}
            </span>
            {lastReceipt.progress ? (
              <span>
                进度：{progressStatusLabel(lastReceipt.progress.status)} · {lastReceipt.progress.mastery_score.toFixed(1)} 分 · 已复习 {lastReceipt.progress.review_count} 次
              </span>
            ) : (
              <span>复习排期已保存；进度详情暂时不可读取。</span>
            )}
          </div>
          {sourceRoute(lastReceipt.card.module, lastReceipt.card.entity_type, lastReceipt.card.entity_id) ? (
            <button
              type="button"
              className="review-receipt-source"
              onClick={() => onNavigate?.(sourceRoute(lastReceipt.card.module, lastReceipt.card.entity_type, lastReceipt.card.entity_id)!)}
            >
              <ArrowSquareOut size={14} /> 返回「{lastReceipt.card.source_title ?? lastReceipt.card.entity_id}」
            </button>
          ) : null}
        </section>
      ) : null}

      {/* Main Review Card Section */}
      {loading ? (
        <div style={{ textAlign: "center", padding: "80px 0", color: "var(--text-tertiary, #9ca3af)" }}>
          <ArrowsCounterClockwise size={32} className="spin" style={{ marginBottom: 12 }} />
          <div>正在加载复习队列...</div>
        </div>
      ) : loadError ? (
        <div role="alert" className="review-submit-error">
          <strong>复习队列未能加载。</strong> {loadError}
          <button type="button" className="ui-btn" onClick={() => void loadData()} disabled={loading}>重试读取</button>
        </div>
      ) : currentCard ? (
        <div
          className="review-card-shell"
          style={{
            background: "var(--surface-primary, #ffffff)",
            border: "1px solid var(--border-color, #e5e7eb)",
            borderRadius: 16,
            boxShadow: "0 10px 25px -5px rgba(0, 0, 0, 0.05)",
            padding: "36px 40px",
            minHeight: 380,
            display: "flex",
            flexDirection: "column",
            justifyContent: "space-between",
          }}
        >
          {/* Card Top: Progress & Module */}
          <div>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 20 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 4,
                    fontSize: 12,
                    fontWeight: 600,
                    padding: "3px 8px",
                    borderRadius: 6,
                    background: "rgba(59, 130, 246, 0.1)",
                    color: MODULE_NAMES[currentCard.module]?.color ?? "#3b82f6",
                  }}
                >
                  {MODULE_NAMES[currentCard.module]?.icon}
                  {MODULE_NAMES[currentCard.module]?.label ?? currentCard.module}
                </span>
                <span style={{ fontSize: 12, color: "var(--text-tertiary, #9ca3af)", textTransform: "uppercase" }}>
                  {currentCard.entity_type} · {currentCard.card_type}
                </span>
              </div>
              <div style={{ fontSize: 13, color: "var(--text-secondary, #6b7280)", fontWeight: 500 }}>
                {currentIndex + 1} / {queue.length}
              </div>
            </div>
            {currentSourceRoute ? (
              <button
                type="button"
                className="review-source-link"
                onClick={() => onNavigate?.(currentSourceRoute)}
              >
                <ArrowSquareOut size={14} /> 原始来源：{currentCard.source_title ?? currentCard.entity_id}
              </button>
            ) : null}

            {/* Prompt Question */}
            <div style={{ marginBottom: 28 }}>
              <div style={{ fontSize: 13, color: "var(--text-tertiary, #9ca3af)", marginBottom: 8, fontWeight: 600 }}>
                复习提问
              </div>
              <h2 style={{ fontSize: 22, fontWeight: 600, color: "var(--text-primary, #111827)", lineHeight: 1.4 }}>
                {currentCard.prompt}
              </h2>
            </div>

            {/* 主观作答：填空 / 问答（含 NCE 句子卡）。不给输入框的话，
                「复习」就退化成「再看一遍」，主动回忆就没了。 */}
            {needsTypedAnswer(currentCard) ? (
              <div style={{ marginBottom: 20 }}>
                <textarea
                  value={typedAnswer}
                  onChange={(event) => setTypedAnswer(event.target.value)}
                  placeholder="写下你的答案（写完按空格揭晓对照）"
                  rows={3}
                  style={{
                    width: "100%",
                    boxSizing: "border-box",
                    padding: "12px 14px",
                    borderRadius: 10,
                    border: "1px solid var(--border-color, #e5e7eb)",
                    background: "var(--surface-primary, #fff)",
                    color: "var(--text-primary, #111827)",
                    fontSize: 15,
                    lineHeight: 1.6,
                    resize: "vertical",
                  }}
                />
              </div>
            ) : null}

            {/* Multiple choice options if any */}
            {currentCard.options && currentCard.options.length > 0 && (
              <div className="review-choice-grid">
                {currentCard.options.map((opt, idx) => {
                  const isSelected = selectedOption === opt;
                  return (
                    <button
                      key={idx}
                      onClick={() => {
                        setSelectedOption(opt);
                        setIsAnswerRevealed(true);
                      }}
                      style={{
                        textAlign: "left",
                        padding: "12px 16px",
                        borderRadius: 10,
                        border: isSelected
                          ? "2px solid var(--accent-primary, #2563eb)"
                          : "1px solid var(--border-color, #e5e7eb)",
                        background: isSelected ? "rgba(37, 99, 235, 0.05)" : "var(--surface-secondary, #f9fafb)",
                        fontSize: 14,
                        fontWeight: 500,
                        cursor: "pointer",
                      }}
                    >
                      <span style={{ color: "var(--text-tertiary, #9ca3af)", marginRight: 8, fontWeight: 700 }}>
                        {String.fromCharCode(65 + idx)}.
                      </span>
                      {opt}
                    </button>
                  );
                })}
              </div>
            )}

            {/* Revealed Answer & Explanation */}
            {isAnswerRevealed ? (
              <div
                style={{
                  background: "var(--surface-secondary, #f8fafc)",
                  borderRadius: 12,
                  padding: "20px 24px",
                  borderLeft: "4px solid var(--accent-primary, #2563eb)",
                  marginBottom: 24,
                  animation: "fadeIn 0.2s ease-in-out",
                }}
              >
                <div style={{ fontSize: 12, fontWeight: 700, color: "#2563eb", marginBottom: 6 }}>
                  {currentCard.id.startsWith("knowledge:") ? "个人参考答案" : "参考答案"}
                </div>
                <div style={{ fontSize: 16, fontWeight: 600, color: "var(--text-primary, #1e293b)", marginBottom: 8 }}>
                  {currentCard.answer}
                </div>
                {typedAnswer.trim() && needsTypedAnswer(currentCard) ? (
                  <div style={{ fontSize: 13, marginTop: 8, lineHeight: 1.6 }}>
                    <div style={{ color: "var(--text-secondary, #64748b)" }}>你的答案：{typedAnswer}</div>
                    <div
                      style={{
                        marginTop: 4,
                        fontWeight: 600,
                        color: typedMatch.exact
                          ? "var(--success, #16a34a)"
                          : typedMatch.close
                            ? "var(--warning, #d97706)"
                            : "var(--danger, #dc2626)",
                      }}
                    >
                      {matchFeedback(typedMatch)}
                    </div>
                  </div>
                ) : null}
                {currentCard.user_note ? (
                  <div style={{ fontSize: 13, color: "var(--text-secondary, #64748b)", lineHeight: 1.5, marginTop: 8 }}>
                    <strong>个人备注：</strong>{currentCard.user_note}
                  </div>
                ) : null}
                {(currentCard.context ?? currentCard.hint) && (
                  <div style={{ fontSize: 13, color: "var(--text-secondary, #64748b)", lineHeight: 1.5, marginTop: 8 }}>
                    {currentCard.context ?? currentCard.hint}
                  </div>
                )}
                {onAskAi && (
                  <button
                    onClick={() => onAskAi(`请为我深入解析知识点：${currentCard.prompt}（答案：${currentCard.answer}）`)}
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      gap: 6,
                      marginTop: 12,
                      padding: "6px 12px",
                      borderRadius: 6,
                      background: "rgba(139, 92, 246, 0.1)",
                      color: "#7c3aed",
                      border: "none",
                      fontSize: 12,
                      fontWeight: 600,
                      cursor: "pointer",
                    }}
                  >
                    <Sparkle size={14} /> 向 Personal AI 深入提问此知识点
                  </button>
                )}
              </div>
            ) : (
              /* 两级揭示，各有各的用处（用户反馈「隐藏和提示是一样的，有啥用不知道」）：
                 - 看提示：只给线索，答案仍然藏着 —— 想先自己想想时用；
                 - 看答案：直接给答案 —— 提示也救不回来时用。
                 两者分开，语义才清楚。 */
              <div style={{ textAlign: "center", margin: "32px 0" }}>
                <div style={{ display: "flex", gap: 10, justifyContent: "center", flexWrap: "wrap" }}>
                  {currentCard.hint ? (
                    <button
                      onClick={() => setIsHintRevealed(true)}
                      style={{
                        padding: "10px 18px",
                        borderRadius: 10,
                        border: "1px solid var(--border-color, #e5e7eb)",
                        background: "var(--surface-secondary, #f9fafb)",
                        color: "var(--text-secondary, #4b5563)",
                        fontSize: 14,
                        fontWeight: 600,
                        cursor: "pointer",
                      }}
                    >
                      看提示（先自己想一下）
                    </button>
                  ) : null}
                  <button
                    onClick={() => setIsAnswerRevealed(true)}
                    style={{
                      padding: "12px 28px",
                      borderRadius: 10,
                      border: "none",
                      background: "var(--accent-primary, #2563eb)",
                      color: "#ffffff",
                      fontSize: 15,
                      fontWeight: 600,
                      cursor: "pointer",
                      boxShadow: "0 4px 12px rgba(37, 99, 235, 0.25)",
                    }}
                  >
                    {isHintRevealed ? "看答案 (空格)" : "直接看答案 (空格)"}
                  </button>
                </div>
                {isHintRevealed && currentCard.hint && !isAnswerRevealed ? (
                  <p
                    style={{
                      marginTop: 14,
                      margin: "14px auto 0",
                      maxWidth: 460,
                      padding: "10px 14px",
                      borderRadius: 8,
                      background: "rgba(245, 158, 11, 0.10)",
                      border: "1px dashed rgba(217, 119, 6, 0.35)",
                      color: "var(--text-secondary, #64748b)",
                      fontSize: 13,
                    }}
                  >
                    <b style={{ color: "#b45309" }}>提示：</b>
                    {currentCard.hint}
                  </p>
                ) : null}
              </div>
            )}
          </div>

          {/* Rating Buttons */}
          {isAnswerRevealed && (
            <div style={{ borderTop: "1px solid var(--border-subtle, #f1f5f9)", paddingTop: 20 }}>
              {submitError ? (
                <p className="review-submit-error" role="alert">{submitError}</p>
              ) : null}
              <div style={{ fontSize: 12, color: "var(--text-tertiary, #9ca3af)", textAlign: "center", marginBottom: 12 }}>
                选择掌握程度评价（支持键盘数字键 1 - 4）
              </div>
              <div className="review-rating-grid">
                <button
                  disabled={submitting || (pendingRating !== null && pendingRating !== "again")}
                  onClick={() => handleRating("again")}
                  style={{
                    padding: "12px 8px",
                    borderRadius: 10,
                    border: "1px solid rgba(239, 68, 68, 0.3)",
                    background: "rgba(239, 68, 68, 0.06)",
                    color: "#dc2626",
                    fontWeight: 600,
                    fontSize: 14,
                    cursor: "pointer",
                    textAlign: "center",
                  }}
                >
                  <div>{submitError && pendingRating === "again" ? "重试 1. 重来" : "1. 重来 (Again)"}</div>
                  <div style={{ fontSize: 11, fontWeight: 400, opacity: 0.8, marginTop: 2 }}>遗忘 · 立即再练</div>
                </button>

                <button
                  disabled={submitting || (pendingRating !== null && pendingRating !== "hard")}
                  onClick={() => handleRating("hard")}
                  style={{
                    padding: "12px 8px",
                    borderRadius: 10,
                    border: "1px solid rgba(245, 158, 11, 0.3)",
                    background: "rgba(245, 158, 11, 0.06)",
                    color: "#d97706",
                    fontWeight: 600,
                    fontSize: 14,
                    cursor: "pointer",
                    textAlign: "center",
                  }}
                >
                  <div>{submitError && pendingRating === "hard" ? "重试 2. 困难" : "2. 困难 (Hard)"}</div>
                  <div style={{ fontSize: 11, fontWeight: 400, opacity: 0.8, marginTop: 2 }}>费力想起 · 较短间隔</div>
                </button>

                <button
                  disabled={submitting || (pendingRating !== null && pendingRating !== "good")}
                  onClick={() => handleRating("good")}
                  style={{
                    padding: "12px 8px",
                    borderRadius: 10,
                    border: "1px solid rgba(59, 130, 246, 0.3)",
                    background: "rgba(59, 130, 246, 0.06)",
                    color: "#2563eb",
                    fontWeight: 600,
                    fontSize: 14,
                    cursor: "pointer",
                    textAlign: "center",
                  }}
                >
                  <div>{submitError && pendingRating === "good" ? "重试 3. 良好" : "3. 良好 (Good)"}</div>
                  <div style={{ fontSize: 11, fontWeight: 400, opacity: 0.8, marginTop: 2 }}>正常回忆 · 标准间隔</div>
                </button>

                <button
                  disabled={submitting || (pendingRating !== null && pendingRating !== "easy")}
                  onClick={() => handleRating("easy")}
                  style={{
                    padding: "12px 8px",
                    borderRadius: 10,
                    border: "1px solid rgba(16, 185, 129, 0.3)",
                    background: "rgba(16, 185, 129, 0.06)",
                    color: "#059669",
                    fontWeight: 600,
                    fontSize: 14,
                    cursor: "pointer",
                    textAlign: "center",
                  }}
                >
                  <div>{submitError && pendingRating === "easy" ? "重试 4. 熟练" : "4. 熟练 (Easy)"}</div>
                  <div style={{ fontSize: 11, fontWeight: 400, opacity: 0.8, marginTop: 2 }}>轻松回忆 · 延长间隔</div>
                </button>
              </div>
            </div>
          )}
        </div>
      ) : (
        /* Completed state */
        <div
          style={{
            background: "var(--surface-primary, #ffffff)",
            border: "1px solid var(--border-color, #e5e7eb)",
            borderRadius: 16,
            padding: "60px 40px",
            textAlign: "center",
          }}
        >
          <div
            style={{
              width: 64,
              height: 64,
              borderRadius: "50%",
              background: "rgba(16, 185, 129, 0.1)",
              color: "#10b981",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              margin: "0 auto 20px",
            }}
          >
            <CheckCircle size={36} weight="fill" />
          </div>
          <h2 style={{ fontSize: 22, fontWeight: 700, color: "var(--text-primary, #111827)", marginBottom: 8 }}>
            今日待复习已全部完成！
          </h2>
          <p style={{ color: "var(--text-secondary, #6b7280)", fontSize: 14, maxWidth: 460, margin: "0 auto 28px" }}>
            你已完成本轮复习任务。科学的间隔重复能让记忆长久驻留，继续探索新知吧！
          </p>

          <div style={{ display: "flex", justifyContent: "center", gap: 12 }}>
            <button
              onClick={() => onNavigate?.("#home")}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
                padding: "10px 20px",
                borderRadius: 8,
                border: "none",
                background: "var(--accent-primary, #2563eb)",
                color: "#ffffff",
                fontWeight: 600,
                fontSize: 14,
                cursor: "pointer",
              }}
            >
              返回今日主页 <ArrowRight size={16} />
            </button>
            <button
              onClick={() => onNavigate?.("#graph")}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
                padding: "10px 20px",
                borderRadius: 8,
                border: "1px solid var(--border-color, #e5e7eb)",
                background: "var(--surface-primary, #ffffff)",
                color: "var(--text-primary, #111827)",
                fontWeight: 600,
                fontSize: 14,
                cursor: "pointer",
              }}
            >
              <Brain size={16} /> 查看知识图谱
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
