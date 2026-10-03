/**
 * 课末测验（Quiz 阶段）。
 *
 * 题目全部由**后端从本课真实数据生成**（词汇选择 / 填空 / 听写 / 翻译），
 * 这里只负责呈现与收集作答；判分与 SRS 回炉由后端 `submit_quiz` 统一完成。
 *
 * 判分口径：
 * - 选择 / 填空：本地即时判定（答案就在题目数据里）；
 * - 听写：归一化后比对（忽略大小写与标点）；
 * - 翻译：**自评**（对照参考译文自己判断），AI 可用时可请 AI 点评——
 *   不做严格字符串比较，也不假装有自动评分。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CheckCircle, Circle, Play, XCircle } from "@phosphor-icons/react";
import type {
  LessonSentence,
  QuizAnswer,
  QuizItem,
  QuizResult,
  VocabWithState,
} from "../../../types";
import { errorMessage } from "../../../utils";
import { englishClient } from "./englishClient";
import { cx } from "../languageUi";

export interface QuizStageProps {
  lessonId: string;
  vocab: VocabWithState[];
  sentences: LessonSentence[];
  /** 测验结束（或用户提前离开）时回调；`result` 为 null 表示没有结果。 */
  onCompleted: (result: QuizResult | null) => void;
  onAskAi?: (prompt: string) => void;
  aiAvailable?: boolean;
}

interface Draft {
  /** 每题的用户作答（选择下标 / 文本）。 */
  values: Array<string | number | null>;
  /** 自评题（翻译）是否判为正确。 */
  selfGrade: Array<boolean | null>;
}

export function QuizStage({
  lessonId,
  sentences,
  onCompleted,
  onAskAi,
  aiAvailable = false,
}: QuizStageProps) {
  const [items, setItems] = useState<QuizItem[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [index, setIndex] = useState(0);
  const [draft, setDraft] = useState<Draft>({ values: [], selfGrade: [] });
  const [result, setResult] = useState<QuizResult | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [submitted, setSubmitted] = useState(false);
  const [revealed, setRevealed] = useState(false);
  const dictationAudioRef = useRef<HTMLAudioElement | null>(null);
  const [dictationUrl, setDictationUrl] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    englishClient
      .quiz(lessonId)
      .then((value) => {
        if (!alive) return;
        setItems(value);
        setDraft({
          values: value.map(() => null),
          selfGrade: value.map(() => null),
        });
      })
      .catch((cause: unknown) => {
        if (alive) setLoadError(errorMessage(cause));
      });
    return () => {
      alive = false;
    };
  }, [lessonId]);

  // 听写题需要音频：首次需要时加载一次。
  const ensureDictationAudio = useCallback(async () => {
    if (dictationUrl) return dictationUrl;
    try {
      const buffer = await englishClient.lessonAudio(lessonId);
      const url = URL.createObjectURL(new Blob([buffer], { type: "audio/mpeg" }));
      setDictationUrl(url);
      return url;
    } catch {
      return null;
    }
  }, [dictationUrl, lessonId]);

  useEffect(() => () => {
    if (dictationUrl) URL.revokeObjectURL(dictationUrl);
  }, [dictationUrl]);

  const current = items?.[index] ?? null;

  const playRange = useCallback(
    async (startMs: number, endMs: number) => {
      const url = await ensureDictationAudio();
      const audio = dictationAudioRef.current ?? new Audio();
      dictationAudioRef.current = audio;
      if (url) audio.src = url;
      audio.currentTime = startMs / 1000;
      void audio.play().catch(() => undefined);
      // 到句尾自动停（A-B 区段）。
      const stop = () => {
        audio.pause();
        audio.removeEventListener("timeupdate", onTick);
      };
      const onTick = () => {
        if (audio.currentTime * 1000 >= endMs) stop();
      };
      audio.addEventListener("timeupdate", onTick);
    },
    [ensureDictationAudio],
  );

  const answers = useMemo<QuizAnswer[]>(() => {
    if (!items) return [];
    return items.map((item, itemIndex) => {
      const value = draft.values[itemIndex];
      switch (item.kind) {
        case "vocabulary": {
          const correct = typeof value === "number" && value === item.answer;
          return {
            item_index: itemIndex,
            correct,
            user_answer: correct ? item.word : item.word,
          };
        }
        case "fill_blank": {
          const text = typeof value === "string" ? value.trim() : "";
          const correct = normalize(text) === normalize(item.answer);
          return {
            item_index: itemIndex,
            correct,
            user_answer: text.length > 0 ? `${item.answer}|${text}` : item.answer,
          };
        }
        case "dictation": {
          const text = typeof value === "string" ? value.trim() : "";
          return {
            item_index: itemIndex,
            correct: normalize(text) === normalize(item.answer),
            user_answer: text.length > 0 ? text : null,
          };
        }
        case "translate": {
          const text = typeof value === "string" ? value.trim() : "";
          const selfGrade = draft.selfGrade[itemIndex];
          return {
            item_index: itemIndex,
            correct: selfGrade === true && text.length > 0,
            user_answer: text.length > 0 ? text : null,
          };
        }
        default:
          return { item_index: itemIndex, correct: false, user_answer: null };
      }
    });
  }, [draft, items]);

  const answeredCount = answers.filter((answer) => {
    const item = items?.[answer.item_index];
    if (!item) return false;
    if (item.kind === "vocabulary") return typeof draft.values[answer.item_index] === "number";
    return typeof draft.values[answer.item_index] === "string" &&
      (draft.values[answer.item_index] as string).trim().length > 0;
  }).length;

  const submit = useCallback(async () => {
    if (!items) return;
    setSubmitting(true);
    setLoadError(null);
    try {
      const quizResult = await englishClient.submitQuiz(lessonId, answers);
      setResult(quizResult);
      setSubmitted(true);
      setRevealed(true);
      onCompleted(quizResult);
    } catch (cause) {
      setLoadError(errorMessage(cause));
    } finally {
      setSubmitting(false);
    }
  }, [answers, items, lessonId, onCompleted]);

  if (loadError && !items) {
    return (
      <section className="en-stage">
        <p className="en-inline-error">{loadError}</p>
        <button type="button" className="en-ghost-btn" onClick={() => onCompleted(null)}>
          返回课程
        </button>
      </section>
    );
  }

  if (!items) {
    return (
      <section className="en-stage">
        <h2>Lesson Quiz</h2>
        <p className="en-muted">正在从本课内容生成题目…</p>
      </section>
    );
  }

  if (submitted && result) {
    return (
      <section className="en-stage en-quiz-result">
        <h2>Lesson Completed</h2>
        <p className="en-quiz-score">
          <strong>{result.score}</strong> 分 · {result.correct}/{result.total} 题正确
        </p>
        {result.score < 60 ? (
          <p className="en-muted">
            低于 60 分，本课已记为「学习中」。答错的词已进入今天的复习队列。
          </p>
        ) : (
          <p className="en-muted">
            本课已完成。明天会安排一次整课复习；答错的词也会单独回到复习队列。
          </p>
        )}
        {result.wrong_words.length > 0 ? (
          <div className="en-wrong-words">
            <p className="en-muted">需要加强的词：</p>
            <div className="en-chip-row">
              {result.wrong_words.map((word) => (
                <span key={word} className="en-chip is-danger">
                  {word}
                </span>
              ))}
            </div>
          </div>
        ) : (
          <p className="en-muted">全部答对，没有错词。</p>
        )}
        <button type="button" className="en-primary-btn" onClick={() => onCompleted(null)}>
          回到 English 首页
        </button>
      </section>
    );
  }

  return (
    <section className="en-stage en-quiz-stage">
      <header className="en-stage-head">
        <div>
          <h2>Lesson Quiz</h2>
          <p className="en-muted">
            {items.length} 题 · 已作答 {answeredCount} 题 · 题目来自本课内容
          </p>
        </div>
        <div className="en-progress-track" role="progressbar" aria-valuenow={index + 1}>
          <i style={{ width: `${((index + 1) / items.length) * 100}%` }} />
        </div>
      </header>

      {current ? (
        <div className="en-quiz-card">
          <p className="en-quiz-kind">{kindLabel(current.kind)}</p>

          {current.kind === "vocabulary" ? (
            <>
              <h3>
                {current.word}
                {current.phonetic ? (
                  <span className="en-phonetic">/{current.phonetic}/</span>
                ) : null}
              </h3>
              <ul className="en-quiz-options">
                {current.options.map((option, optionIndex) => (
                  <li key={option}>
                    <button
                      type="button"
                      className={cx(
                        "en-quiz-option",
                        draft.values[index] === optionIndex && "is-picked",
                      )}
                      onClick={() =>
                        setDraft((previous) => {
                          const values = [...previous.values];
                          values[index] = optionIndex;
                          return { ...previous, values };
                        })
                      }
                    >
                      <span className="en-option-letter">
                        {String.fromCharCode(65 + optionIndex)}
                      </span>
                      {option}
                    </button>
                  </li>
                ))}
              </ul>
            </>
          ) : null}

          {current.kind === "fill_blank" ? (
            <>
              <h3 className="en-quiz-fill">{current.sentence}</h3>
              {current.chinese ? (
                <p className="en-sentence-zh">{current.chinese}</p>
              ) : null}
              <input
                className="en-input"
                type="text"
                value={(draft.values[index] as string) ?? ""}
                placeholder="填入缺少的词"
                onChange={(event) =>
                  setDraft((previous) => {
                    const values = [...previous.values];
                    values[index] = event.target.value;
                    return { ...previous, values };
                  })
                }
              />
            </>
          ) : null}

          {current.kind === "dictation" ? (
            <>
              <h3>听写：播放下面的句子并输入</h3>
              <button
                type="button"
                className="en-ghost-btn"
                onClick={() => void playRange(current.start_ms, current.end_ms)}
              >
                <Play size={14} /> 播放这一句
              </button>
              <textarea
                className="en-input"
                rows={2}
                value={(draft.values[index] as string) ?? ""}
                placeholder="你听到的内容…"
                onChange={(event) =>
                  setDraft((previous) => {
                    const values = [...previous.values];
                    values[index] = event.target.value;
                    return { ...previous, values };
                  })
                }
              />
              {revealed ? (
                <p className="en-quiz-answer">参考答案：{current.answer}</p>
              ) : null}
            </>
          ) : null}

          {current.kind === "translate" ? (
            <>
              <h3 className="en-quiz-translate">{current.chinese}</h3>
              <textarea
                className="en-input"
                rows={2}
                placeholder="用英文把这句说出来…"
                value={(draft.values[index] as string) ?? ""}
                onChange={(event) =>
                  setDraft((previous) => {
                    const values = [...previous.values];
                    values[index] = event.target.value;
                    return { ...previous, values };
                  })
                }
              />
              <div className="en-self-grade">
                <span className="en-muted">对照参考译文后自评：</span>
                <button
                  type="button"
                  className={cx("en-self-btn", draft.selfGrade[index] === true && "is-good")}
                  onClick={() =>
                    setDraft((previous) => {
                      const selfGrade = [...previous.selfGrade];
                      selfGrade[index] = true;
                      return { ...previous, selfGrade };
                    })
                  }
                >
                  <CheckCircle size={14} /> 我写对了
                </button>
                <button
                  type="button"
                  className={cx("en-self-btn", draft.selfGrade[index] === false && "is-bad")}
                  onClick={() =>
                    setDraft((previous) => {
                      const selfGrade = [...previous.selfGrade];
                      selfGrade[index] = false;
                      return { ...previous, selfGrade };
                    })
                  }
                >
                  <XCircle size={14} /> 我写错了
                </button>
              </div>
              {revealed ? (
                <>
                  <p className="en-quiz-answer">参考译文：{current.reference}</p>
                  {aiAvailable && onAskAi ? (
                    <button
                      type="button"
                      className="en-link-btn"
                      onClick={() =>
                        onAskAi(
                          [
                            `中文原句：${current.chinese}`,
                            `参考译文：${current.reference}`,
                            `我的表达：${(draft.values[index] as string) ?? "（空）"}`,
                            "请比较我的表达：意思是否正确？有哪些语法问题？更自然的说法是什么？",
                          ].join("\n"),
                        )
                      }
                    >
                      请 AI 对比我的表达
                    </button>
                  ) : null}
                </>
              ) : null}
            </>
          ) : null}

          <div className="en-quiz-nav">
            <button
              type="button"
              className="en-ghost-btn"
              onClick={() => setIndex((previous) => Math.max(0, previous - 1))}
              disabled={index === 0}
            >
              上一题
            </button>
            {index < items.length - 1 ? (
              <button
                type="button"
                className="en-primary-btn"
                onClick={() => setIndex((previous) => Math.min(items.length - 1, previous + 1))}
              >
                下一题
              </button>
            ) : (
              <button
                type="button"
                className="en-primary-btn"
                onClick={() => void submit()}
                disabled={submitting || answeredCount < items.length}
                title={
                  answeredCount < items.length ? `还有 ${items.length - answeredCount} 题没作答` : undefined
                }
              >
                {submitting ? "提交中…" : "提交"}
              </button>
            )}
          </div>
          {answeredCount < items.length ? (
            <p className="en-muted">还有 {items.length - answeredCount} 题没作答</p>
          ) : null}
        </div>
      ) : null}

      <ol className="en-quiz-dots">
        {items.map((item, dotIndex) => {
          const answered =
            typeof draft.values[dotIndex] === "number" ||
            (typeof draft.values[dotIndex] === "string" &&
              (draft.values[dotIndex] as string).trim().length > 0);
          return (
            <li key={dotIndex}>
              <button
                type="button"
                className={cx(
                  "en-quiz-dot",
                  dotIndex === index && "is-current",
                  answered && "is-answered",
                )}
                onClick={() => setIndex(dotIndex)}
                aria-label={`第 ${dotIndex + 1} 题`}
              >
                {answered ? <Circle size={8} weight="fill" /> : null}
              </button>
            </li>
          );
        })}
      </ol>
    </section>
  );
}

function kindLabel(kind: QuizItem["kind"]): string {
  switch (kind) {
    case "vocabulary":
      return "词汇";
    case "fill_blank":
      return "填空";
    case "dictation":
      return "听力";
    case "translate":
      return "翻译";
    default:
      return "";
  }
}

/** 判分归一化：忽略大小写、标点与多余空白。 */
function normalize(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9\s']/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}
