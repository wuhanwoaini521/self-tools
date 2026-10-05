/**
 * 学习统计（Progress）。
 *
 * 只展示**可解释、有用**的数据：完成课程 / 学过的词 / 复习掌握度 / 学习时长 /
 * 连续天数 / 每册进度 / **开口指标**。没有装饰性图表，也没有假 KPI（任务书 §27）。
 *
 * 「开口」是能不能交流的直接证据（V13 W2）：读了多少课不代表会说话，
 * 说了多久、说得准不准才是。所以统计里有单独一块，取自**真实跟读记录**；
 * 没有任何记录时明确说「还没有跟读记录」，不显示 0 分假装练过。
 */
import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, Fire } from "@phosphor-icons/react";
import type { EnglishProgress } from "../../../types";
import { errorMessage } from "../../../utils";
import { englishClient } from "./englishClient";
import { formatDurationShort } from "./shared";
import type { ShadowStats } from "../speakingTypes";

export interface ProgressViewProps {
  onBack: () => void;
}

export function ProgressView({ onBack }: ProgressViewProps) {
  const [data, setData] = useState<EnglishProgress | null>(null);
  const [shadow, setShadow] = useState<ShadowStats | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      setData(await englishClient.progress());
    } catch (cause) {
      setError(errorMessage(cause));
    }
    // 跟读统计独立取：它失败不该把整页变成错误页（进度数据仍然有用）。
    englishClient
      .shadowStats(null, 0)
      .then(setShadow)
      .catch(() => setShadow(null));
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

      <section className="en-card">
        <header className="en-card-head">
          <h3>开口（跟读记录）</h3>
          <span className="en-muted">说了多久 · 说得准不准</span>
        </header>
        {!shadow || shadow.attempts === 0 ? (
          <p className="en-muted">
            还没有跟读评分记录。在课内进入 <strong>Shadowing</strong> 阶段，点「朗读评分」
            就会被记录（需要 Chrome / Edge + 麦克风 + 网络）。
          </p>
        ) : (
          <div className="en-metric-row">
            <div className="en-metric">
              <span className="en-metric-label">开口时长</span>
              <strong>{formatDurationShort(shadow.spoken_seconds)}</strong>
            </div>
            <div className="en-metric">
              <span className="en-metric-label">跟读次数</span>
              <strong>{shadow.attempts}</strong>
            </div>
            <div className="en-metric">
              <span className="en-metric-label">平均准确度</span>
              <strong>{shadow.avg_accuracy}%</strong>
            </div>
            <div className="en-metric">
              <span className="en-metric-label">平均完整度</span>
              <strong>{shadow.avg_completeness}%</strong>
            </div>
            <div className="en-metric">
              <span className="en-metric-label">平均流利度</span>
              <strong>{shadow.avg_fluency}%</strong>
            </div>
            <div className="en-metric">
              <span className="en-metric-label">说得不错（≥80）</span>
              <strong>{shadow.strong_attempts}</strong>
            </div>
          </div>
        )}
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