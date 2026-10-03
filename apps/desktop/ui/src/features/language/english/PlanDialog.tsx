/**
 * 学习计划（每日目标）。
 *
 * 首次使用或想调整节奏时打开：设置每日分钟数、新词数与当前主课程册。
 * 保存后首页的「今日任务」与 Continue 会据此安排。
 */
import { useCallback, useEffect, useState } from "react";
import { X } from "@phosphor-icons/react";
import type { CourseBook, LearningPlan } from "../../../types";
import { errorMessage } from "../../../utils";
import { englishClient } from "./englishClient";
import { cx } from "../languageUi";

export interface PlanDialogProps {
  onClose: () => void;
  onSaved: () => void;
}

const MINUTE_OPTIONS = [20, 30, 45, 60];
const WORD_OPTIONS = [5, 10, 15, 20];

export function PlanDialog({ onClose, onSaved }: PlanDialogProps) {
  const [plan, setPlan] = useState<LearningPlan | null>(null);
  const [books, setBooks] = useState<CourseBook[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let alive = true;
    Promise.all([englishClient.planGet(), englishClient.books()])
      .then(([stored, bookList]) => {
        if (!alive) return;
        setPlan(
          stored ?? {
            language: "eng",
            course_id: "nce",
            book_id: bookList[0]?.id ?? null,
            daily_minutes: 30,
            new_words_per_day: 10,
            updated_at: 0,
          },
        );
        setBooks(bookList);
      })
      .catch((cause: unknown) => {
        if (alive) setError(errorMessage(cause));
      });
    return () => {
      alive = false;
    };
  }, []);

  const save = useCallback(async () => {
    if (!plan) return;
    setSaving(true);
    setError(null);
    try {
      await englishClient.planSave({ ...plan, updated_at: 0 });
      onSaved();
      onClose();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setSaving(false);
    }
  }, [onClose, onSaved, plan]);

  return (
    <div className="en-modal-layer" onClick={onClose}>
      <div className="en-modal en-plan" onClick={(event) => event.stopPropagation()} role="dialog" aria-label="学习计划">
        <header className="en-modal-head">
          <h2>学习计划</h2>
          <button type="button" className="en-icon-btn" onClick={onClose} aria-label="关闭">
            <X size={15} />
          </button>
        </header>

        {error ? <p className="en-inline-error">{error}</p> : null}

        {plan ? (
          <>
            <section className="en-plan-section">
              <h3>每天学多久</h3>
              <div className="en-choice-row">
                {MINUTE_OPTIONS.map((minutes) => (
                  <button
                    key={minutes}
                    type="button"
                    className={cx("en-choice", plan.daily_minutes === minutes && "is-on")}
                    onClick={() => setPlan({ ...plan, daily_minutes: minutes })}
                  >
                    {minutes} min
                  </button>
                ))}
              </div>
            </section>

            <section className="en-plan-section">
              <h3>每天新词</h3>
              <div className="en-choice-row">
                {WORD_OPTIONS.map((count) => (
                  <button
                    key={count}
                    type="button"
                    className={cx("en-choice", plan.new_words_per_day === count && "is-on")}
                    onClick={() => setPlan({ ...plan, new_words_per_day: count })}
                  >
                    {count}
                  </button>
                ))}
              </div>
            </section>

            <section className="en-plan-section">
              <h3>当前主课程</h3>
              <div className="en-choice-column">
                {books.length === 0 ? (
                  <p className="en-muted">还没有导入课程，先去导入新概念英语。</p>
                ) : (
                  books.map((book) => (
                    <button
                      key={book.id}
                      type="button"
                      className={cx("en-choice", plan.book_id === book.id && "is-on")}
                      onClick={() => setPlan({ ...plan, book_id: book.id, course_id: "nce" })}
                    >
                      {book.title}
                      <small>{book.subtitle}</small>
                    </button>
                  ))
                )}
              </div>
            </section>

            <footer className="en-modal-foot">
              <button type="button" className="en-ghost-btn" onClick={onClose}>
                取消
              </button>
              <button
                type="button"
                className="en-primary-btn"
                onClick={() => void save()}
                disabled={saving}
              >
                {saving ? "保存中…" : "保存"}
              </button>
            </footer>
          </>
        ) : (
          <p className="en-muted">加载中…</p>
        )}
      </div>
    </div>
  );
}