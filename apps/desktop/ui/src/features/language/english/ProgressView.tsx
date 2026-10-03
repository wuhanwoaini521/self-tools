/**
 * 学习统计（Progress）。
 *
 * 只展示**可解释、有用**的数据：完成课程 / 学过的词 / 复习掌握度 / 学习时长 /
 * 连续天数 / 每册进度。没有装饰性图表，也没有假 KPI（任务书 §27）。
 */
import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, Fire } from "@phosphor-icons/react";
import type { EnglishProgress } from "../../../types";
import { errorMessage } from "../../../utils";
import { englishClient } from "./englishClient";
import { formatDurationShort } from "./shared";

export interface ProgressViewProps {
  onBack: () => void;
}

export function ProgressView({ onBack }: ProgressViewProps) {
  const [data, setData] = useState<EnglishProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      setData(await englishClient.progress());
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

  if (!data) {
    return (
      <div className="en-library">
        <p className="en-muted">加载中…</p>
      </div>
    );
  }

  const currentBook = data.books[0];

  return (
    <div className="en-library">
      <header className="en-page-head">
        <div className="en-title-row">
          <button type="button" className="en-icon-btn" onClick={onBack} title="返回">
            <ArrowLeft size={16} />
          </button>
          <div>
            <p className="en-greeting-hello">Progress</p>
            <h1>学习统计</h1>
          </div>
        </div>
      </header>

      <section className="en-metric-row">
        <div className="en-metric">
          <span className="en-metric-label">已完成课程</span>
          <strong>{data.lessons_completed}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">学习中</span>
          <strong>{data.lessons_learning}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">学过单词</span>
          <strong>{data.words_learned}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">已掌握</span>
          <strong>{data.words_mastered}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">复习掌握度</span>
          <strong>{Math.round(data.review_mastery)}%</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">待复习</span>
          <strong>{data.due_reviews}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">听力 / 精读时长</span>
          <strong>{formatDurationShort(data.study_seconds_total)}</strong>
        </div>
        <div className="en-metric">
          <span className="en-metric-label">
            <Fire size={13} /> 连续学习
          </span>
          <strong>{data.streak_days} 天</strong>
        </div>
      </section>

      {currentBook ? (
        <section className="en-card">
          <header className="en-card-head">
            <h3>{currentBook.book.title}</h3>
            <span className="en-muted">
              {currentBook.summary.completed_lessons}/{currentBook.summary.total_lessons} 课
            </span>
          </header>
          <div className="en-progress-line">
            <div
              className="en-progress-track"
              role="progressbar"
              aria-valuenow={currentBook.summary.completed_lessons}
            >
              <i
                style={{
                  width: `${
                    currentBook.summary.total_lessons > 0
                      ? (currentBook.summary.completed_lessons /
                          currentBook.summary.total_lessons) *
                        100
                      : 0
                  }%`,
                }}
              />
            </div>
          </div>
          <p className="en-muted">
            学习时长 {formatDurationShort(currentBook.summary.study_seconds)} · 生词{" "}
            {currentBook.summary.vocab_total}
          </p>
        </section>
      ) : null}

      <section className="en-card">
        <header className="en-card-head">
          <h3>各册进度</h3>
        </header>
        <ul className="en-book-progress-list">
          {data.books.map((item) => {
            const total = item.summary.total_lessons || 1;
            const percent = Math.round((item.summary.completed_lessons / total) * 100);
            return (
              <li key={item.book.id}>
                <span className="en-book-progress-title">
                  {item.book.title}
                  <small>
                    {item.summary.completed_lessons}/{item.summary.total_lessons}
                  </small>
                </span>
                <span className="en-progress-track">
                  <i style={{ width: `${percent}%` }} />
                </span>
                <span className="en-muted">
                  {formatDurationShort(item.summary.study_seconds)}
                </span>
              </li>
            );
          })}
        </ul>
      </section>
    </div>
  );
}