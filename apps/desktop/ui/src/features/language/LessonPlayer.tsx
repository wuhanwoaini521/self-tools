/**
 * 课程播放器。
 *
 * `lesson(lessonId)` 返回 `LessonView`：`items` 是每一步的完整学习条目，
 * `step_index` 是恢复位置（后端已 clamp 到合法范围）。
 * 每前进一步都 `saveLessonPosition(lessonId, nextIndex)`，所以中途退出再进来
 * 一定落在同一步。←/→ 切换，Esc 退出。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { X } from "@phosphor-icons/react";
import type { LessonView } from "../../types";
import { languageClient } from "./languageClient";
import { errorMessage } from "../../utils";
import { Panel, PanelBody, ProgressTrack } from "./LanguagePrimitives";
import { FocusMode } from "./FocusMode";
import type { StudyTarget } from "./ItemViews";
import { EMPTY_COPY, useAsyncPanel } from "./languageUi";

export interface LessonPlayerProps {
  lessonId: string;
  onClose: () => void;
  onAskAi?: (prompt: string) => void;
  /** 进度写回后通知外层，让「继续学习」那一块更新。 */
  onProgressSaved?: (nextIndex: number) => void;
}

export function LessonPlayer({
  lessonId,
  onClose,
  onAskAi,
  onProgressSaved,
}: LessonPlayerProps) {
  const view = useAsyncPanel<LessonView | null>(
    () => languageClient.lesson(lessonId),
    [lessonId],
  );
  const [index, setIndex] = useState<number | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  // 只有第一次拿到数据时才同步 `step_index`；之后完全由本组件推进，
  // 否则保存后的刷新会把用户位置拽回去。
  useEffect(() => {
    if (view.data && index === null) setIndex(view.data.step_index);
  }, [view.data, index]);

  const items = view.data?.items ?? [];
  const current = index === null ? null : (items[index] ?? null);

  /**
   * 切换步骤并持久化位置。
   *
   * 顺序保证：连按 ←/→ 会并发发出多个 `saveLessonPosition`，服务端"后到的写入生效"，
   * 于是存档步骤与屏幕上的步骤会分叉。因此用一个自增 token 丢弃过期响应，
   * 并在卸载后不再 setState。
   */
  const saveToken = useRef(0);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const goTo = useCallback(
    (next: number) => {
      if (items.length === 0) return;
      const clamped = Math.max(0, Math.min(items.length - 1, next));
      setIndex(clamped);
      setSaveError(null);
      const token = ++saveToken.current;
      void languageClient
        .saveLessonPosition(lessonId, clamped)
        .then(() => {
          // 只有最新一次切换才允许推进首页的 Continue 状态。
          if (!mounted.current || token !== saveToken.current) return;
          onProgressSaved?.(clamped);
        })
        .catch((err: unknown) => {
          if (!mounted.current || token !== saveToken.current) return;
          setSaveError(errorMessage(err));
        });
    },
    [items.length, lessonId, onProgressSaved],
  );

  if (view.loading) {
    return (
      <div className="lang-lesson">
        <Panel title="正在打开课程…">
          <PanelBody
            loading
            error={null}
            reload={view.reload}
            empty={null}
            skeletonRows={4}
          >
            <span />
          </PanelBody>
        </Panel>
      </div>
    );
  }

  if (view.error) {
    return (
      <div className="lang-lesson">
        <Panel title="课程打不开">
          <div className="lang-inline-error">
            <p>{view.error}</p>
            <button type="button" className="lang-link" onClick={view.reload}>
              重试
            </button>
          </div>
          <button type="button" className="lang-link" onClick={onClose}>
            返回
          </button>
        </Panel>
      </div>
    );
  }

  if (!view.data || items.length === 0) {
    return (
      <div className="lang-lesson">
        <Panel title="课程内容为空">
          <p className="lang-empty">
            这门课程还没有任何步骤。{EMPTY_COPY.lessons}
          </p>
          <button type="button" className="lang-link" onClick={onClose}>
            返回
          </button>
        </Panel>
      </div>
    );
  }

  if (!current || index === null) {
    return (
      <div className="lang-lesson">
        <Panel title="课程打不开">
          <p className="lang-empty">保存的进度超出了课程长度，请重新开始。</p>
          <button type="button" className="lang-primary" onClick={() => goTo(0)}>
            从头开始
          </button>
          <button type="button" className="lang-link" onClick={onClose}>
            返回
          </button>
        </Panel>
      </div>
    );
  }

  const target: StudyTarget = { kind: "item", item: current };

  return (
    <div className="lang-lesson">
      <div className="lang-lesson-bar">
        <div>
          <h2>{view.data.title}</h2>
          <ProgressTrack value={index + 1} total={items.length} />
          <span className="lang-muted">
            第 {index + 1} / {items.length} 步 · ← / → 切换 · Esc 退出
          </span>
        </div>
        <button type="button" className="lang-link" onClick={onClose}>
          <X size={15} /> 结束课程
        </button>
      </div>
      {saveError ? <p className="lang-inline-error-text">进度保存失败：{saveError}</p> : null}
      <FocusMode
        target={target}
        position={{ index, total: items.length }}
        onKnow={() => {
          // 这两个调用失败必须可见：否则「记住了」点下去没反应、
          // 「加入复习」失败后用户以为已加入，下次复习根本不会见到它。
          void languageClient
            .recordStudy(current.id, "complete")
            .catch((err: unknown) => setSaveError(errorMessage(err)));
          goTo(index + 1);
        }}
        onNeedReview={() => {
          void languageClient
            .addToReview(current.id)
            .catch((err: unknown) => setSaveError(errorMessage(err)));
        }}
        onBookmark={() => undefined}
        onAskAi={onAskAi}
        onNext={() => goTo(index + 1)}
        onPrev={index > 0 ? () => goTo(index - 1) : undefined}
        onClose={onClose}
        onOpenWordDetail={() => undefined}
        onOpenSentence={() => undefined}
        bookmarked={false}
      />
    </div>
  );
}