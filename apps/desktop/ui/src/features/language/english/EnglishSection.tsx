/**
 * English 子模块容器（Language 页内切换）。
 *
 * 负责：加载数据、维护视图状态（首页 / 课程库 / 册 / 工作台 / 复习 / 统计）、
 * 把各动作派发到客户端。它是唯一持有跨视图状态的地方，视图组件保持「纯展示 + 回调」。
 *
 * 视图切换全部在内存里完成（不跳路由），符合「主流程不频繁跳页」的要求。
 */
import { useCallback, useEffect, useState } from "react";
import type {
  LessonDetail,
  LessonListEntry,
  TodayDashboard,
  WordMark,
} from "../../../types";
import { errorMessage } from "../../../utils";
import { englishClient } from "./englishClient";
import { EnglishHome } from "./EnglishHome";
import { CourseLibrary } from "./CourseLibrary";
import { BookViewPage } from "./BookViewPage";
import { LessonWorkspace } from "./LessonWorkspace";
import { ReviewHub } from "./ReviewHub";
import { ProgressView } from "./ProgressView";
import { RoadmapPage } from "./RoadmapPage";
import { ImportDialog } from "./ImportDialog";
import { PlanDialog } from "./PlanDialog";

export type EnglishView =
  | { kind: "home" }
  | { kind: "library" }
  | { kind: "book"; bookId: string }
  | { kind: "lesson"; lessonId: string }
  | { kind: "review" }
  | { kind: "progress" }
  | { kind: "roadmap" };

export interface EnglishSectionProps {
  /** 打开全局 AI；缺省 / AI 不可用时隐藏 AI 入口。 */
  onAskAi?: (prompt: string) => void;
  aiAvailable?: boolean;
}

export function EnglishSection({ onAskAi, aiAvailable = false }: EnglishSectionProps) {
  const [view, setView] = useState<EnglishView>({ kind: "home" });
  const [dashboard, setDashboard] = useState<TodayDashboard | null>(null);
  const [dashboardError, setDashboardError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [detail, setDetail] = useState<LessonDetail | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [showImport, setShowImport] = useState(false);
  const [showPlan, setShowPlan] = useState(false);

  const loadDashboard = useCallback(async () => {
    setLoading(true);
    setDashboardError(null);
    try {
      setDashboard(await englishClient.today());
    } catch (cause) {
      setDashboardError(errorMessage(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadDashboard();
  }, [loadDashboard]);

  // 打开某一课：先取详情；失败给明确错误而不是空白工作台。
  useEffect(() => {
    if (view.kind !== "lesson") {
      setDetail(null);
      return;
    }
    let alive = true;
    setDetailError(null);
    englishClient
      .lesson(view.lessonId)
      .then((result) => {
        if (!alive) return;
        if (!result) {
          setDetailError("这一课不存在（可能教材被重新导入过）。");
          return;
        }
        setDetail(result);
      })
      .catch((cause: unknown) => {
        if (alive) setDetailError(errorMessage(cause));
      });
    return () => {
      alive = false;
    };
  }, [view]);

  const openLesson = useCallback((lesson: LessonListEntry) => {
    setView({ kind: "lesson", lessonId: lesson.id });
  }, []);

  const markWord = useCallback(
    async (word: string, mark: WordMark) => {
      const lessonId = view.kind === "lesson" ? view.lessonId : null;
      await englishClient.markWord(lessonId ?? "global", word, mark);
      // 标记后刷新当前课详情（单词状态 new→learning/known 立刻反映）。
      if (lessonId) {
        try {
          const fresh = await englishClient.lesson(lessonId);
          if (fresh) setDetail(fresh);
        } catch {
          // 详情刷新失败不打断学习。
        }
      }
    },
    [view],
  );

  const backToHome = useCallback(() => {
    setView({ kind: "home" });
    void loadDashboard();
  }, [loadDashboard]);

  // 视图渲染。
  if (view.kind === "lesson") {
    if (detailError) {
      return (
        <div className="en-library">
          <p className="en-inline-error">{detailError}</p>
          <button type="button" className="en-ghost-btn" onClick={backToHome}>
            返回首页
          </button>
        </div>
      );
    }
    if (!detail) {
      return (
        <div className="en-library">
          <p className="en-muted">正在打开课时…</p>
        </div>
      );
    }
    return (
      <>
        <LessonWorkspace
          detail={detail}
          onExit={backToHome}
          onCompleted={() => {
            void loadDashboard();
          }}
          onMarkWord={markWord}
          onAskAi={onAskAi}
          aiAvailable={aiAvailable}
        />
        {showImport ? (
          <ImportDialog onClose={() => setShowImport(false)} onImported={loadDashboard} />
        ) : null}
      </>
    );
  }

  if (view.kind === "library") {
    return (
      <>
        <CourseLibrary
          onOpenLesson={openLesson}
          onOpenImport={() => setShowImport(true)}
          onBack={backToHome}
        />
        {showImport ? (
          <ImportDialog onClose={() => setShowImport(false)} onImported={loadDashboard} />
        ) : null}
      </>
    );
  }

  if (view.kind === "book") {
    return (
      <BookViewPage
        bookId={view.bookId}
        onOpenLesson={openLesson}
        onBack={() => setView({ kind: "library" })}
      />
    );
  }

  if (view.kind === "review") {
    return (
      <ReviewHub
        onBack={backToHome}
        onDone={() => {
          void loadDashboard();
        }}
      />
    );
  }

  if (view.kind === "progress") {
    return <ProgressView onBack={backToHome} />;
  }

  if (view.kind === "roadmap") {
    return (
      <RoadmapPage
        onBack={backToHome}
        onStart={() => {
          // 没有学习记录时给一条真能走的路：去课程库挑第一课。
          setView({ kind: "library" });
        }}
      />
    );
  }

  // 默认：首页驾驶舱。
  return (
    <>
      <EnglishHome
        dashboard={dashboard}
        loading={loading}
        error={dashboardError}
        onReload={loadDashboard}
        onContinue={openLesson}
        onOpenBook={() => setView({ kind: "library" })}
        onOpenLibrary={() => setView({ kind: "library" })}
        onOpenReview={() => setView({ kind: "review" })}
        onOpenProgress={() => setView({ kind: "progress" })}
        onOpenRoadmap={() => setView({ kind: "roadmap" })}
        onOpenImport={() => setShowImport(true)}
        onOpenPlan={() => setShowPlan(true)}
      />
      {showImport ? (
        <ImportDialog onClose={() => setShowImport(false)} onImported={loadDashboard} />
      ) : null}
      {showPlan ? (
        <PlanDialog
          onClose={() => setShowPlan(false)}
          onSaved={() => {
            void loadDashboard();
          }}
        />
      ) : null}
    </>
  );
}