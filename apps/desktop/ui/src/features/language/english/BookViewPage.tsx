/**
 * 册视图（BookView）：一册的全部课时 + 状态（未开始 / 学习中 / 已完成）。
 *
 * 不是「一个列表」而是「可继续的进度视图」：顶部有汇总与 Continue，
 * 列表里每课显示句数、生词数、音频可用性与进度。
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowLeft, Play, SpeakerSlash } from "@phosphor-icons/react";
import type { BookView, LessonListEntry } from "../../../types";
import { errorMessage } from "../../../utils";
import { englishClient } from "./englishClient";
import { formatDurationShort, statusLabel } from "./shared";
import { cx } from "../languageUi";

export interface BookViewPageProps {
  bookId: string;
  onOpenLesson: (lesson: LessonListEntry) => void;
  onBack: () => void;
}

type Filter = "all" | "learning" | "completed" | "not_started";

export function BookViewPage({ bookId, onOpenLesson, onBack }: BookViewPageProps) {
  const [view, setView] = useState<BookView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>("all");

  const load = useCallback(async () => {
    setError(null);
    try {
      setView(await englishClient.book(bookId));
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }, [bookId]);

  useEffect(() => {
    void load();
  }, [load]);

  const lessons = useMemo(() => {
    if (!view) return [];
    if (filter === "all") return view.lessons;
    return view.lessons.filter((lesson) => lesson.status === filter);
  }, [filter, view]);

  const next = useMemo(() => {
    if (!view) return null;
    return (
      view.lessons.find((lesson) => lesson.status === "learning") ??
      view.lessons.find((lesson) => lesson.status === "not_started") ??
      null
    );
  }, [view]);

  if (error) {
    return (
      <div className="en-library">
        <p className="en-inline-error">{error}</p>
        <button type="button" className="en-link-btn" onClick={() => void load()}>
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

  const summary = view.summary;

  return (
    <div className="en-library">
      <header className="en-page-head">
        <div className="en-title-row">
          <button type="button" className="en-icon-btn" onClick={onBack} title="返回">
            <ArrowLeft size={16} />
          </button>
          <div>
            <p className="en-greeting-hello">{view.book.subtitle ?? "Book"}</p>
            <h1>{view.book.title}</h1>
          </div>
        </div>
        <div className="en-row-actions">
          {next ? (
            <button
              type="button"
              className="en-primary-btn"
              onClick={() => onOpenLesson(next)}
            >
              <Play size={14} />
              {view.lessons.find((lesson) => lesson.status === "learning")
                ? `继续 Lesson ${next.lesson_no}`
                : `开始 Lesson ${next.lesson_no}`}
            </button>
          ) : null}
        </div>
      </header>

      <section className="en-metric-row">
        <div className="en-metric">
          <span className="en-metric-label">总课程</span>
          <strong>{summary.total_lessons}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">已完成</span>
          <strong>{summary.completed_lessons}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">学习中</span>
          <strong>{summary.learning_lessons}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">学习时长</span>
          <strong>{formatDurationShort(summary.study_seconds)}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">已学单词</span>
          <strong>{summary.vocab_total}</strong>
        </div>
      </section>

      <nav className="en-filter-row" aria-label="课时筛选">
        {(
          [
            ["all", `全部 ${view.lessons.length}`],
            ["learning", `学习中 ${summary.learning_lessons}`],
            ["not_started", `未开始 ${view.lessons.filter((l) => l.status === "not_started").length}`],
            ["completed", `已完成 ${summary.completed_lessons}`],
          ] as Array<[Filter, string]>
        ).map(([key, label]) => (
          <button
            key={key}
            type="button"
            className={cx("en-filter-chip", filter === key && "is-on")}
            onClick={() => setFilter(key)}
          >
            {label}
          </button>
        ))}
      </nav>

      <ul className="en-lesson-list">
        {lessons.map((lesson) => (
          <li key={lesson.id}>
            <button
              type="button"
              className={cx("en-lesson-row", `is-${lesson.status}`)}
              onClick={() => onOpenLesson(lesson)}
            >
              <span className="en-lesson-no">{lesson.lesson_no}</span>
              <span className="en-lesson-main">
                <strong>{lesson.title || `Lesson ${lesson.lesson_no}`}</strong>
                <span className="en-lesson-meta">
                  {lesson.sentence_count} 句 · {lesson.vocab_count} 词
                  {lesson.audio_path ? null : (
                    <em className="en-lesson-noaudio">
                      <SpeakerSlash size={11} /> 无音频
                    </em>
                  )}
                  {lesson.duration_ms ? ` · ${formatDurationShort(lesson.duration_ms / 1000)}` : null}
                </span>
              </span>
              <span className="en-lesson-status">
                {statusLabel(lesson.status)}
                {lesson.status === "learning" && lesson.percent > 0 ? (
                  <span className="en-lesson-percent">{lesson.percent}%</span>
                ) : null}
              </span>
            </button>
          </li>
        ))}
        {lessons.length === 0 ? (
          <li className="en-muted">没有符合条件的课时。</li>
        ) : null}
      </ul>
    </div>
  );
}