/**
 * 课程库（Course Library）：New Concept English 1–4。
 *
 * 每册展示真实进度（已完成 / 总课数 / 学习时长 / 生词数）与 Continue 入口。
 */
import { useCallback, useEffect, useState } from "react";
import { BookOpen, DownloadSimple, Play } from "@phosphor-icons/react";
import type { BookView, CourseBook, LessonListEntry } from "../../../types";
import { errorMessage } from "../../../utils";
import { englishClient } from "./englishClient";
import { formatDurationShort } from "./shared";

export interface CourseLibraryProps {
  onOpenLesson: (lesson: LessonListEntry) => void;
  onOpenImport: () => void;
  onBack: () => void;
}

export function CourseLibrary({ onOpenLesson, onOpenImport, onBack }: CourseLibraryProps) {
  const [books, setBooks] = useState<CourseBook[] | null>(null);
  const [views, setViews] = useState<Record<string, BookView>>({});
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await englishClient.books();
      setBooks(list);
      const loaded = await Promise.all(
        list.map(async (book) => [book.id, await englishClient.book(book.id)] as const),
      );
      const map: Record<string, BookView> = {};
      loaded.forEach(([id, view]) => {
        if (view) map[id] = view;
      });
      setViews(map);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const continueLessonFor = (view: BookView): LessonListEntry | null => {
    const learning = view.lessons.find((lesson) => lesson.status === "learning");
    if (learning) return learning;
    return view.lessons.find((lesson) => lesson.status === "not_started") ?? null;
  };

  return (
    <div className="en-library">
      <header className="en-page-head">
        <div>
          <p className="en-greeting-hello">Course</p>
          <h1>New Concept English</h1>
        </div>
        <div className="en-row-actions">
          <button type="button" className="en-ghost-btn" onClick={() => void load()}>
            刷新
          </button>
          <button type="button" className="en-ghost-btn" onClick={onOpenImport}>
            <DownloadSimple size={14} /> 导入 / 词典
          </button>
          <button type="button" className="en-link-btn" onClick={onBack}>
            返回首页
          </button>
        </div>
      </header>

      {error ? (
        <p className="en-inline-error">
          {error} <button type="button" className="en-link-btn" onClick={() => void load()}>重试</button>
        </p>
      ) : null}

      {loading && !books ? <p className="en-muted">加载中…</p> : null}

      {books && books.length === 0 ? (
        <section className="en-card en-onboard">
          <h2>还没有课程</h2>
          <p className="en-muted">导入本地的新概念英语文件夹后，这里会出现 NCE1–NCE4。</p>
          <button type="button" className="en-primary-btn" onClick={onOpenImport}>
            <DownloadSimple size={15} /> 导入新概念英语
          </button>
        </section>
      ) : null}

      <div className="en-book-grid">
        {(books ?? []).map((book) => {
          const view = views[book.id];
          const summary = view?.summary;
          const total = summary?.total_lessons ?? book.total_lessons ?? 0;
          const done = summary?.completed_lessons ?? 0;
          const percent = total > 0 ? Math.round((done / total) * 100) : 0;
          const next = view ? continueLessonFor(view) : null;
          return (
            <section key={book.id} className="en-card en-book-card">
              <header className="en-book-head">
                <BookOpen size={18} />
                <div>
                  <h2>{book.title}</h2>
                  {book.subtitle ? <p className="en-muted">{book.subtitle}</p> : null}
                </div>
              </header>

              <dl className="en-book-metrics">
                <div>
                  <dt>总课程</dt>
                  <dd>{total}</dd>
                </div>
                <div>
                  <dt>已完成</dt>
                  <dd>{done}</dd>
                </div>
                <div>
                  <dt>学习中</dt>
                  <dd>{summary?.learning_lessons ?? 0}</dd>
                </div>
                <div>
                  <dt>学习时长</dt>
                  <dd>{formatDurationShort(summary?.study_seconds ?? 0)}</dd>
                </div>
                <div>
                  <dt>生词</dt>
                  <dd>{summary?.vocab_total ?? 0}</dd>
                </div>
              </dl>

              <div className="en-progress-line">
                <div className="en-progress-track" role="progressbar" aria-valuenow={percent}>
                  <i style={{ width: `${percent}%` }} />
                </div>
                <span>{percent}%</span>
              </div>

              {next ? (
                <button
                  type="button"
                  className="en-primary-btn is-wide"
                  onClick={() => onOpenLesson(next)}
                >
                  <Play size={14} />
                  {view?.lessons.find((lesson) => lesson.status === "learning")
                    ? `继续 Lesson ${next.lesson_no}`
                    : `开始 Lesson ${next.lesson_no}`}
                  {next.title ? ` · ${next.title}` : ""}
                </button>
              ) : (
                <p className="en-muted">这一册已全部完成。</p>
              )}
            </section>
          );
        })}
      </div>
    </div>
  );
}