/**
 * 专注学习模式（Focus Learning Mode）。
 *
 * 覆盖整个 Dashboard：没有侧栏、没有 Tab 栏、没有语言选择器，只剩
 * 「当前条目 + 进度 + 主操作」。Esc / 关闭按钮退出。
 *
 * 快捷键：←/→ 上下步，Esc 退出；打字时不触发。
 */
import { useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  BookmarkSimple,
  CheckCircle,
  Sparkle,
  X,
} from "@phosphor-icons/react";
import type { LearningProgress } from "../../types";
import { languageClient } from "./languageClient";
import { errorMessage, isTypingTarget } from "../../utils";
import {
  Action,
  Chip,
  MasteryBar,
  ProgressTrack,
} from "./LanguagePrimitives";
import { StudyBody, type StudyTarget } from "./ItemViews";
import {
  DIFFICULTY_LABELS,
  ITEM_TYPE_LABELS,
  STATUS_LABELS,
  cx,
} from "./languageUi";

export interface FocusModeProps {
  target: StudyTarget;
  /** 第 n 步 / 共 m 步；非课程场景（单条专注学）传 null。 */
  position: { index: number; total: number } | null;
  onKnow: () => void;
  onNeedReview: () => void;
  /** 「已收藏」状态的外部通知；缺省时收藏按钮直接调用 `addToReview`。 */
  onBookmark?: () => void;
  onAskAi?: (prompt: string) => void;
  onClose: () => void;
  /** 拆解块点开的词典详情；缺省时不渲染该入口。 */
  onOpenWordDetail?: (entityId: string) => void;
  /** 文章里点开的一句：交给外层去 `sentenceStudy`。 */
  onOpenSentence?: (text: string) => void;
  /** 上下步导航（课程 / 多条目场景）。缺省时不渲染箭头按钮。 */
  onNext?: () => void;
  onPrev?: () => void;
  /** 该条目是否已在复习队列里。 */
  bookmarked?: boolean;
}

/**
 * 焦点模式本体。`onKnow` / `onNeedReview` / `onBookmark` 会把学习事件写回
 * 后端（`recordStudy` / `addToReview`）；进度条上的数字来自 `progress()`，
 * 不在前端伪造。进度查询失败只是不显示进度，不影响学习。
 */
export function FocusMode({
  target,
  position,
  onKnow,
  onNeedReview,
  onBookmark,
  onAskAi,
  onNext,
  onPrev,
  onClose,
  onOpenWordDetail,
  onOpenSentence,
  bookmarked,
}: FocusModeProps) {
  const hasAi = typeof onAskAi === "function";
  const entityId = target.kind === "item" ? target.item.id : null;
  const [record, setRecord] = useState<LearningProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 这条内容的学习记录。失败时静默降级为「还没有记录」。
  useEffect(() => {
    let alive = true;
    if (!entityId) {
      setRecord(null);
      return undefined;
    }
    languageClient
      .progress(200)
      .then((rows) => {
        if (!alive) return;
        setRecord(rows.find((row) => row.entity_id === entityId) ?? null);
      })
      .catch(() => {
        if (alive) setRecord(null);
      });
    return () => {
      alive = false;
    };
  }, [entityId]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (isTypingTarget(event.target)) return;
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      } else if (event.key === "ArrowRight" && onNext) {
        event.preventDefault();
        onNext();
      } else if (event.key === "ArrowLeft" && onPrev) {
        event.preventDefault();
        onPrev();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [onClose, onNext, onPrev]);

  // 关闭浮层后仍在飞行中的请求不得再 setState（否则对已卸载组件写状态）。
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const act = async (action: "view" | "study" | "complete") => {
    if (!entityId) return;
    setBusy(true);
    try {
      const next = await languageClient.recordStudy(entityId, action);
      if (alive.current) setRecord(next);
    } catch (err) {
      if (alive.current) setError(errorMessage(err));
    } finally {
      if (alive.current) setBusy(false);
    }
  };

  const bookmark = async () => {
    if (!entityId) return;
    setBusy(true);
    try {
      await languageClient.addToReview(entityId);
      if (alive.current) onBookmark?.();
    } catch (err) {
      if (alive.current) setError(errorMessage(err));
    } finally {
      if (alive.current) setBusy(false);
    }
  };

  const prompt =
    target.kind === "sentence"
      ? `请讲解这句话：${target.sentence.original}`
      : `请帮我讲解：${target.item.content}`;

  return (
    <div className="lang-focus" role="dialog" aria-modal="true">
      <header className="lang-focus-top">
        <div className="lang-focus-steps">
          <span>
            {position
              ? `第 ${position.index + 1} / ${position.total} 步`
              : "专注学习"}
          </span>
          {position ? (
            <ProgressTrack value={position.index + 1} total={position.total} />
          ) : null}
        </div>
        <div className="lang-focus-top-actions">
          {target.kind === "item" ? (
            <>
              <Chip tone="accent">{ITEM_TYPE_LABELS[target.item.type]}</Chip>
              <Chip
                tone={target.item.difficulty === "hard" ? "danger" : "plain"}
              >
                {DIFFICULTY_LABELS[target.item.difficulty]}
              </Chip>
            </>
          ) : (
            <Chip tone="accent">句子</Chip>
          )}
          <button
            type="button"
            className={cx("lang-fav", bookmarked && "on")}
            onClick={() => void bookmark()}
            disabled={busy || !entityId}
            title="加入复习队列"
          >
            <BookmarkSimple size={15} />
          </button>
          <button
            type="button"
            className="lang-focus-close"
            onClick={onClose}
            title="退出专注模式 (Esc)"
          >
            <X size={16} />
          </button>
        </div>
      </header>

      <div className="lang-focus-body">
        <StudyBody
          target={target}
          onOpenWord={onOpenWordDetail}
          onOpenSentence={onOpenSentence}
          onOpenDetail={
            onOpenWordDetail && target.kind === "item"
              ? () => onOpenWordDetail(target.item.id)
              : undefined
          }
          onAskAi={onAskAi}
          hasAi={hasAi}
        />
      </div>

      <footer className="lang-focus-actions">
        <div className="lang-focus-progress">
          {record ? (
            <>
              <MasteryBar score={record.mastery_score} />
              <span className="lang-muted">
                {STATUS_LABELS[record.status]} · 学过 {record.study_count} 次 · 复习{" "}
                {record.review_count} 次
              </span>
            </>
          ) : (
            <span className="lang-muted">
              还没有这条内容的学习记录。学一遍就会出现在这里。
            </span>
          )}
          {error ? <span className="lang-inline-error-text">{error}</span> : null}
        </div>
        <div className="lang-focus-buttons">
          {onPrev ? (
            <Action variant="ghost" onClick={onPrev}>
              <ArrowLeft size={14} /> 上一步
            </Action>
          ) : null}
          <Action
            variant="ghost"
            onClick={() => void act("study")}
            disabled={busy || !entityId}
          >
            标记已学
          </Action>
          <Action
            variant="danger"
            onClick={() => {
              onNeedReview();
              void act("view");
            }}
          >
            需要复习
          </Action>
          <Action
            onClick={() => {
              onKnow();
              void act("complete");
            }}
          >
            <CheckCircle size={14} /> 记住了
          </Action>
          {onNext ? (
            <Action variant="ghost" onClick={onNext}>
              下一条 <ArrowRight size={14} />
            </Action>
          ) : null}
        </div>
        {hasAi ? (
          <button
            type="button"
            className="lang-link lang-focus-ai"
            onClick={() => onAskAi?.(prompt)}
          >
            <Sparkle size={13} /> 问 AI
          </button>
        ) : (
          <p className="lang-muted lang-focus-ai">
            AI 增强不可用，词典内容照常学习。
          </p>
        )}
      </footer>
    </div>
  );
}