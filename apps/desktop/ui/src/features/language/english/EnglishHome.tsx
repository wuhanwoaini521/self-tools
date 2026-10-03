/**
 * English 首页 = 学习驾驶舱（任务书 §3）。
 *
 * 只回答一个问题：**我今天该学什么**。
 * 布局：问候 → 概览数字 → Continue Learning → 今日任务 → 到期复习 → 最近学习。
 * 不展示「没用的大数字」和装饰性图表。
 */
import { useEffect, useMemo, useState } from "react";
import {
  ArrowRight,
  Books,
  CalendarBlank,
  Clock,
  Fire,
  GearSix,
} from "@phosphor-icons/react";
import type {
  BookView,
  CourseBook,
  DataStatus,
  LessonListEntry,
  TodayDashboard,
} from "../../../types";
import { greetingByHour } from "../../../utils";
import { englishClient } from "./englishClient";
import { cx } from "../languageUi";
import { formatDurationShort, StageDots } from "./shared";

export interface EnglishHomeProps {
  dashboard: TodayDashboard | null;
  loading: boolean;
  error: string | null;
  onReload: () => void;
  onContinue: (lesson: LessonListEntry) => void;
  onOpenBook: (bookId: string) => void;
  onOpenLibrary: () => void;
  onOpenReview: () => void;
  onOpenProgress: () => void;
  onOpenImport: () => void;
  onOpenPlan: () => void;
}

export function EnglishHome({
  dashboard,
  loading,
  error,
  onReload,
  onContinue,
  onOpenBook,
  onOpenLibrary,
  onOpenReview,
  onOpenProgress,
  onOpenImport,
  onOpenPlan,
}: EnglishHomeProps) {
  const [recentLessons, setRecentLessons] = useState<LessonListEntry[] | null>(null);
  const [dataStatus, setDataStatus] = useState<DataStatus | null>(null);

  // 资料现状只用于「提醒补齐数据」，失败不打扰学习。
  useEffect(() => {
    let alive = true;
    englishClient
      .dataStatus()
      .then((value) => {
        if (alive) setDataStatus(value);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [dashboard]);

  // 「最近学习」从课程列表兜底（dashboard 已带 recent_lessons；这里仅在缺失时补）。
  useEffect(() => {
    if (dashboard?.recent_lessons?.length) {
      setRecentLessons(dashboard.recent_lessons);
      return;
    }
    let alive = true;
    englishClient
      .books()
      .then(async (books) => {
        if (!alive) return;
        const views = await Promise.all(
          books.slice(0, 1).map((book: CourseBook) => englishClient.book(book.id)),
        );
        const entries = views
          .filter((view): view is NonNullable<typeof view> => view !== null)
          .flatMap((view: BookView) =>
            view.lessons.filter((lesson: LessonListEntry) => lesson.status !== "not_started"),
          )
          .slice(0, 6);
        if (alive) setRecentLessons(entries);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [dashboard]);

  const tasks = useMemo(() => {
    if (!dashboard) return [];
    const summary = dashboard.book_summary;
    const lesson = dashboard.continue_lesson ?? dashboard.next_lesson;
    const items: Array<{ label: string; done: boolean; action?: () => void }> = [];
    if (dashboard.due_reviews > 0) {
      items.push({
        label: `复习 ${dashboard.due_reviews} 个到期词`,
        done: false,
        action: onOpenReview,
      });
    }
    if (lesson) {
      const stage = dashboard.continue_lesson ? lesson.percent : 0;
      items.push({
        label: `Lesson ${lesson.lesson_no} 单词预习`,
        done: stage > 0,
      });
      items.push({ label: "听力精听", done: stage > 40 });
      items.push({ label: "课文精读", done: stage > 60 });
      items.push({ label: "跟读复述", done: stage > 75 });
      items.push({ label: "Lesson Quiz", done: stage >= 100 });
    }
    if (summary) {
      items.push({
        label: `本册已完成 ${summary.completed_lessons}/${summary.total_lessons} 课`,
        done: false,
      });
    }
    return items;
  }, [dashboard, onOpenReview]);

  if (error) {
    return (
      <div className="en-home">
        <p className="en-inline-error">{error}</p>
        <button type="button" className="en-link-btn" onClick={onReload}>
          重试
        </button>
      </div>
    );
  }

  // ---- 未导入课程：给出唯一的下一步（不要十几个入口）----
  if (!loading && dashboard && !dashboard.imported) {
    return (
      <div className="en-home">
        <header className="en-greeting">
          <p className="en-greeting-hello">{greetingByHour(new Date().getHours())}</p>
          <h1>Today's English</h1>
        </header>
        <section className="en-card en-onboard">
          <h2>从新概念英语开始</h2>
          <p className="en-muted">
            self-tools 的英语主线是新概念英语 1–4。教材、音频与字幕都在你自己的电脑上，
            导入一次即可离线学习；生词、复习与学习记录会自动进入统一词汇系统。
          </p>
          <ol className="en-onboard-steps">
            <li>选择包含 NCE1–NCE4 的文件夹（每课一个 .lrc + .mp3）</li>
            <li>导入 ECDICT 词库（可选但强烈建议，用于音标与中文释义）</li>
            <li>设置每日目标，系统自动安排今天该学的课</li>
          </ol>
          <div className="en-onboard-actions">
            <button type="button" className="en-primary-btn" onClick={onOpenImport}>
              <Books size={15} /> 导入新概念英语
            </button>
          </div>
        </section>
      </div>
    );
  }

  const lesson = dashboard?.continue_lesson ?? dashboard?.next_lesson ?? null;
  const book = dashboard?.current_book;
  const summary = dashboard?.book_summary;
  const percent = lesson?.percent ?? 0;

  return (
    <div className="en-home">
      <header className="en-greeting">
        <div>
          <p className="en-greeting-hello">{greetingByHour(new Date().getHours())}</p>
          <h1>Today's English</h1>
        </div>
        <div className="en-row-actions">
          <button type="button" className="en-link-btn" onClick={onOpenLibrary}>
            课程库
          </button>
          <button type="button" className="en-icon-btn" onClick={onOpenPlan} title="学习计划">
            <GearSix size={15} />
          </button>
        </div>
      </header>

      {/* ---- 概览：只放与「今天做什么」直接相关的数字 ---- */}
      <section className="en-metric-row" aria-label="今日概览">
        <div className="en-metric">
          <span className="en-metric-label">新概念课程</span>
          <strong>{lesson ? `Lesson ${lesson.lesson_no}` : "未导入"}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">今日新词</span>
          <strong>{dashboard?.plan?.new_words_per_day ?? "—"}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">待复习</span>
          <strong>{dashboard?.due_reviews ?? 0}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">今日学习</span>
          <strong>{formatDurationShort(dashboard?.study_seconds_today ?? 0)}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">连续天数</span>
          <strong>
            <Fire size={14} /> {dashboard?.streak_days ?? 0}
          </strong>
        </div>
      </section>

      {/* ---- Continue Learning：唯一的主行动 ---- */}
      {lesson ? (
        <section className="en-card en-continue">
          <div className="en-continue-head">
            <span className="en-continue-kicker">
              {dashboard?.continue_lesson ? "Continue Learning" : "Start Today"}
            </span>
          </div>
          <h2>
            {book?.title ?? "New Concept English"}
            <small>
              Lesson {lesson.lesson_no}
              {lesson.title ? ` · ${lesson.title}` : ""}
            </small>
          </h2>
          {lesson.sentence_count > 0 ? (
            <p className="en-muted">
              {lesson.sentence_count} 句 · {lesson.vocab_count} 个生词
              {lesson.duration_ms
                ? ` · ${formatDurationShort(lesson.duration_ms / 1000)}`
                : " · 无音频"}
            </p>
          ) : null}
          <div className="en-progress-line">
            <div className="en-progress-track" role="progressbar" aria-valuenow={percent}>
              <i style={{ width: `${percent}%` }} />
            </div>
            <span>{percent}%</span>
          </div>
          {lesson ? <StageDots status={lesson.status} /> : null}
          <button
            type="button"
            className="en-primary-btn is-wide"
            onClick={() => onContinue(lesson)}
          >
            {dashboard?.continue_lesson ? "继续学习" : "开始这一课"}
            <ArrowRight size={15} />
          </button>
        </section>
      ) : null}

      {/* ---- 今日任务清单 ---- */}
      <section className="en-card">
        <header className="en-card-head">
          <h3>今日任务</h3>
        </header>
        <ul className="en-task-list">
          {tasks.length === 0 ? (
            <li className="en-muted">
              {loading ? "加载中…" : "还没有任务，先设置每日目标或选一本书开始。"}
            </li>
          ) : (
            tasks.map((task) => (
              <li key={task.label} className={cx(task.done && "is-done")}>
                <span className="en-task-mark" aria-hidden="true">
                  {task.done ? "✓" : "○"}
                </span>
                {task.action ? (
                  <button type="button" className="en-link-btn" onClick={task.action}>
                    {task.label}
                  </button>
                ) : (
                  <span>{task.label}</span>
                )}
              </li>
            ))
          )}
        </ul>
      </section>

      {/* ---- 复习入口 ---- */}
      <section className="en-card en-review-card">
        <div>
          <h3>Review</h3>
          <p className="en-muted">
            {dashboard && dashboard.due_reviews > 0
              ? `${dashboard.due_reviews} 个词到期 · 约 ${Math.max(1, Math.ceil(dashboard.due_reviews * 0.25))} 分钟`
              : "今天没有到期的复习"}
          </p>
        </div>
        <button
          type="button"
          className="en-ghost-btn"
          onClick={onOpenReview}
          disabled={!dashboard || dashboard.due_reviews === 0}
        >
          开始复习
        </button>
      </section>

      {/* ---- 最近学习 ---- */}
      <section className="en-card">
        <header className="en-card-head">
          <h3>Recent Learning</h3>
          <div className="en-row-actions">
            {book ? (
              <button
                type="button"
                className="en-link-btn"
                onClick={() => onOpenBook(book.id)}
              >
                全部课程
              </button>
            ) : null}
            <button type="button" className="en-link-btn" onClick={onOpenProgress}>
              学习统计
            </button>
          </div>
        </header>
        {recentLessons && recentLessons.length > 0 ? (
          <ul className="en-recent-list">
            {recentLessons.map((item) => (
              <li key={item.id}>
                <button
                  type="button"
                  className="en-recent-item"
                  onClick={() => onContinue(item)}
                >
                  <span className="en-recent-title">
                    Lesson {item.lesson_no}
                    {item.title ? ` · ${item.title}` : ""}
                  </span>
                  <span className="en-recent-meta">
                    <Clock size={12} /> {item.sentence_count} 句
                    <CalendarBlank size={12} />
                    {item.status === "completed" ? "已完成" : item.status === "learning" ? `${item.percent}%` : "未开始"}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="en-muted">
            {loading ? "加载中…" : "还没有学习记录。学完第一课后这里会显示最近学过的内容。"}
          </p>
        )}
      </section>

      {dashboard && !dashboard.plan ? (
        <section className="en-card en-plan-nudge">
          <div>
            <h3>设置每日目标</h3>
            <p className="en-muted">
              告诉系统你每天想学多久、能吃下多少新词，它会把课程安排成每天一小步。
            </p>
          </div>
          <button type="button" className="en-ghost-btn" onClick={onOpenPlan}>
            去设置
          </button>
        </section>
      ) : null}

      {dataStatus && dataStatus.nce_lessons > 0 && dataStatus.nce_lessons < 60 ? (
        <section className="en-card en-data-nudge">
          <div>
            <h3>教材只有 {dataStatus.nce_lessons} 课（样本）</h3>
            <p className="en-muted">
              完整的新概念 1–4 一共 276 课。要学完，请在「学习资料」里导入完整教材文件夹。
            </p>
          </div>
          <button type="button" className="en-ghost-btn" onClick={onOpenImport}>
            打开学习资料
          </button>
        </section>
      ) : null}

      {dashboard && !dashboard.dict_ready ? (
        <section className="en-card en-data-nudge">
          <div>
            <h3>还没有词典</h3>
            <p className="en-muted">
              导入 ECDICT 后，生词卡会有音标、中文释义与词频（现在只有英文单词）。
            </p>
          </div>
          <button type="button" className="en-ghost-btn" onClick={onOpenImport}>
            导入词典
          </button>
        </section>
      ) : null}
    </div>
  );
}
