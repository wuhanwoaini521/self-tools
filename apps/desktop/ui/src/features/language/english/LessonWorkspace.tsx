/**
 * Lesson 工作台：一课的学习全部在这里完成（任务书 §11 / §30）。
 *
 * 九步学习流程压进**一个页面 + 六个阶段**，切换阶段不跳路由：
 *
 * ```text
 * Vocabulary → Listening → Reading → Sentence → Shadow → Quiz
 * ```
 *
 * 关键约束：
 * - 查词 / AI / 生词 / 笔记都在本页用浮层或侧栏处理，不跳页；
 * - 音频只有**一个** `<audio>` 实例，所有阶段共用（避免多实例打架）；
 * - 进度（阶段 / 位置 / 句号 / 跟读位置 / 学习秒数）节流写后端，关闭应用后可恢复；
 * - 没有 AI 时 Lesson 仍然完整可用（AI 只是右侧可选增强）。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, Sparkle, Note, X, BookOpenText } from "@phosphor-icons/react";
import type {
  LessonDetail,
  LessonStage,
  VocabWithState,
  WordMark,
} from "../../../types";
import { errorMessage } from "../../../utils";
import { englishClient } from "./englishClient";
import { cx } from "../languageUi";
import { StageNav, nextStage, prevStage, formatTimestamp, STAGE_SEQUENCE } from "./shared";
import { AudioPlayer } from "./AudioPlayer";
import { SentenceText } from "./SentenceText";
import { WordPopover } from "./WordPopover";
import { VocabularyStage } from "./VocabularyStage";
import { ListenStage } from "./ListenStage";
import { ReadingStage } from "./ReadingStage";
import { SentenceStage } from "./SentenceStage";
import { ShadowStage } from "./ShadowStage";
import { QuizStage } from "./QuizStage";
import { ImmersiveReader } from "./ImmersiveReader";
import { MiningPanel } from "./MiningPanel";

export interface LessonWorkspaceProps {
  detail: LessonDetail;
  /** 返回课程列表 / 首页。 */
  onExit: () => void;
  /** 学习完成（进入复习环节）。 */
  onCompleted: () => void;
  /** 把任意单词加入复习（三态）。 */
  onMarkWord: (word: string, mark: WordMark) => Promise<void>;
  /** 打开全局 AI（带课文上下文）。 */
  onAskAi?: (prompt: string) => void;
  aiAvailable?: boolean;
}

export function LessonWorkspace({
  detail,
  onExit,
  onCompleted,
  onMarkWord,
  onAskAi,
  aiAvailable = false,
}: LessonWorkspaceProps) {
  const { lesson, book, sentences, vocab, progress } = detail;
  const [stage, setStage] = useState<LessonStage>(progress.stage === "done" ? "quiz" : progress.stage);
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [audioError, setAudioError] = useState<string | null>(null);
  const [activeSeq, setActiveSeq] = useState<number | null>(progress.sentence_seq || null);
  const [positionMs, setPositionMs] = useState(progress.position_ms);
  const [popover, setPopover] = useState<{ word: string; sentence: string | null } | null>(null);
  /** 沉浸式精读（V13 W1）：全屏单屏阅读，退出时回写句位与时长。 */
  const [immersive, setImmersive] = useState(false);
  const [showTranslation, setShowTranslation] = useState(false);
  const [showVocabHighlight, setShowVocabHighlight] = useState(true);
  const [rate, setRate] = useState(1);
  const [repeat, setRepeat] = useState(false);
  const [stageDone, setStageDone] = useState<Partial<Record<LessonStage, boolean>>>({
    vocabulary: progress.vocab_index > 0,
    // 已经走到更后面的阶段 → 前面的阶段算完成。
    listen: STAGE_SEQUENCE.indexOf(progress.stage) > STAGE_SEQUENCE.indexOf("listen"),
    read: STAGE_SEQUENCE.indexOf(progress.stage) > STAGE_SEQUENCE.indexOf("read"),
    sentence: STAGE_SEQUENCE.indexOf(progress.stage) > STAGE_SEQUENCE.indexOf("sentence"),
    shadow: STAGE_SEQUENCE.indexOf(progress.stage) > STAGE_SEQUENCE.indexOf("shadow"),
    quiz: progress.quiz_score !== null,
  });

  // 学习秒数累计（心跳）：进入页面开始计时，退出/卸载时写回。
  const studySecondsRef = useRef(0);
  useEffect(() => {
    const timer = window.setInterval(() => {
      studySecondsRef.current += 1;
    }, 1000);
    return () => window.clearInterval(timer);
  }, []);

  // ---- 音频加载（一次；失败如实说明）----
  useEffect(() => {
    let alive = true;
    let url: string | null = null;
    if (!lesson.audio_path) {
      setAudioUrl(null);
      return;
    }
    englishClient
      .lessonAudio(lesson.id)
      .then((buffer) => {
        if (!alive) return;
        url = URL.createObjectURL(new Blob([buffer], { type: "audio/mpeg" }));
        setAudioUrl(url);
      })
      .catch(() => {
        // 音频读不出来（文件被移动 / 网页端不提供二进制端点）→ 如实说明，不显示后端原始错误。
        if (alive) {
          setAudioError(
            "音频加载失败（文件可能已被移动；网页端不支持音频播放，桌面端可用）",
          );
        }
      });
    return () => {
      alive = false;
      if (url) URL.revokeObjectURL(url);
    };
  }, [lesson.audio_path, lesson.id]);

  // ---- 进度保存（节流 4 秒 + 关键节点立刻存）----
  const pendingPatch = useRef<{
    position_ms?: number;
    sentence_seq?: number;
    stage?: LessonStage;
    vocab_index?: number;
    shadow_seq?: number;
  }>({});
  const flushProgress = useCallback(
    (immediate = false) => {
      const patch = pendingPatch.current;
      if (Object.keys(patch).length === 0) return;
      pendingPatch.current = {};
      const studySeconds = studySecondsRef.current;
      studySecondsRef.current = 0;
      void englishClient
        .updateProgress(lesson.id, {
          ...patch,
          ...(studySeconds > 0 ? { study_seconds_delta: studySeconds } : {}),
        })
        .catch((cause: unknown) => {
          // 进度保存失败不打断学习：提示一次即可。
          setAudioError((previous) => previous ?? errorMessage(cause));
        });
      if (immediate) return;
    },
    [lesson.id],
  );

  useEffect(() => {
    const timer = window.setInterval(() => flushProgress(), 4000);
    return () => {
      window.clearInterval(timer);
      flushProgress(true);
    };
  }, [flushProgress]);

  const changeStage = useCallback(
    (next: LessonStage) => {
      setStage(next);
      pendingPatch.current.stage = next;
      setStageDone((previous) => ({ ...previous, [stage]: true }));
      flushProgress(true);
    },
    [flushProgress, stage],
  );

  const handleTick = useCallback((ms: number) => {
    pendingPatch.current.position_ms = Math.round(ms);
    setPositionMs(ms);
  }, []);

  const handleSeek = useCallback((ms: number) => {
    pendingPatch.current.position_ms = Math.round(ms);
  }, []);

  const goToSentence = useCallback(
    (seq: number) => {
      setActiveSeq(seq);
      pendingPatch.current.sentence_seq = seq;
    },
    [],
  );

  const markWord = useCallback(
    async (word: string, mark: WordMark) => {
      await onMarkWord(word, mark);
    },
    [onMarkWord],
  );

  const askAi = useCallback(
    (prompt: string) => {
      onAskAi?.(
        [
          `我在学 New Concept English ${book ? book.book_no : ""} Lesson ${lesson.lesson_no} ${lesson.title}。`,
          prompt,
        ].join("\n"),
      );
    },
    [onAskAi, book, lesson],
  );

  const activeSentence = useMemo(
    () => sentences.find((item) => item.sequence === activeSeq) ?? null,
    [activeSeq, sentences],
  );

  // 退出沉浸：句位与时长写回进度（否则关闭页面就丢了刚才读了多久）。
  const exitImmersive = useCallback(
    (sentenceIndex: number, sessionSeconds: number) => {
      setImmersive(false);
      const seq = sentences[sentenceIndex]?.sequence ?? activeSeq;
      if (seq !== null && seq !== undefined) {
        setActiveSeq(seq);
        pendingPatch.current.sentence_seq = seq;
      }
      if (sessionSeconds > 0) studySecondsRef.current += sessionSeconds;
      flushProgress(true);
    },
    [activeSeq, flushProgress, sentences],
  );

  const completedCount = sentences.filter(
    (item) => item.sequence <= (activeSeq ?? -1),
  ).length;

  if (immersive) {
    return (
      <ImmersiveReader
        sentences={sentences}
        vocab={vocab}
        audioUrl={audioUrl}
        audioMissingReason={audioError}
        lessonLabel={`NCE${book?.book_no ?? ""} Lesson ${lesson.lesson_no} · 沉浸精读`}
        lessonId={lesson.id}
        onExit={exitImmersive}
        onMark={markWord}
        onAskAi={onAskAi}
        aiAvailable={aiAvailable}
      />
    );
  }

  return (
    <div className="en-workspace">
      {/* ---- 顶部：返回 / 课程 / 阶段导航 ---- */}
      <header className="en-workspace-head">
        <div className="en-workspace-title">
          <button type="button" className="en-icon-btn" onClick={onExit} title="返回">
            <ArrowLeft size={16} />
          </button>
          <div>
            <p className="en-workspace-course">
              {book?.title ?? "New Concept English"}
            </p>
            <h1>
              Lesson {lesson.lesson_no}
              {lesson.title ? <small> · {lesson.title}</small> : null}
            </h1>
          </div>
        </div>
        <StageNav stage={stage} onChange={changeStage} completed={stageDone} />
        <button
          type="button"
          className="en-ghost-btn en-immersive-btn"
          onClick={() => setImmersive(true)}
          title="全屏沉浸精读（句级高亮 / 点词即查 / 可隐藏译文）"
        >
          <BookOpenText size={15} /> 沉浸精读
        </button>
      </header>

      {/* ---- 主体 + 右侧栏 ---- */}
      <div className="en-workspace-body">
        <main className="en-workspace-main">
          {/* 单一音频实例：所有阶段共用 */}
          <AudioPlayer
            src={audioUrl}
            missingReason={
              audioError ??
              (lesson.audio_path
                ? null
                : "本课没有音频（导入时未找到 .mp3），听力与跟读不可用")
            }
            sentences={sentences}
            initialPositionMs={progress.position_ms}
            activeSeq={activeSeq}
            rate={rate}
            onRateChange={setRate}
            onTick={handleTick}
            onSeek={handleSeek}
            onRepeatToggle={setRepeat}
          />

          {stage === "vocabulary" ? (
            <VocabularyStage
              vocab={vocab}
              startIndex={progress.vocab_index}
              onMark={(word, mark) => markWord(word, mark)}
              onProgressIndex={(index) => {
                pendingPatch.current.vocab_index = index;
              }}
              onFinish={() => changeStage("listen")}
            />
          ) : null}

          {stage === "listen" ? (
            <ListenStage
              sentences={sentences}
              showEnglish={showTranslation}
              showChinese={showTranslation}
              activeSeq={activeSeq}
              onToggleReveal={() => setShowTranslation((previous) => !previous)}
              onSentence={goToSentence}
            />
          ) : null}

          {stage === "read" ? (
            <ReadingStage
              sentences={sentences}
              vocab={vocab}
              showTranslation={showTranslation}
              showVocabHighlight={showVocabHighlight}
              onToggleTranslation={() => setShowTranslation((previous) => !previous)}
              onToggleVocab={() => setShowVocabHighlight((previous) => !previous)}
              onWordClick={(word, sentence) => setPopover({ word, sentence })}
              activeSeq={activeSeq}
              onSentence={goToSentence}
            />
          ) : null}

          {stage === "sentence" ? (
            <SentenceStage
              sentences={sentences}
              vocab={vocab}
              activeSeq={activeSeq}
              positionMs={positionMs}
              repeat={repeat}
              onWordClick={(word, sentence) => setPopover({ word, sentence })}
              onSelect={goToSentence}
              onPrev={() => goToSentence(Math.max(0, (activeSeq ?? 0) - 1))}
              onNext={() =>
                goToSentence(Math.min(sentences.length - 1, (activeSeq ?? -1) + 1))
              }
              onFinish={() => changeStage("shadow")}
            />
          ) : null}

          {stage === "shadow" ? (
            <ShadowStage
              lessonId={lesson.id}
              sentences={sentences}
              startSeq={progress.shadow_seq}
              activeSeq={activeSeq}
              audioUrl={audioUrl}
              onSelect={goToSentence}
              onProgressSeq={(seq) => {
                pendingPatch.current.shadow_seq = seq;
              }}
              onFinish={() => changeStage("quiz")}
            />
          ) : null}

          {stage === "quiz" || stage === "done" ? (
            <QuizStage
              lessonId={lesson.id}
              vocab={vocab}
              sentences={sentences}
              onCompleted={() => {
                setStageDone((previous) => ({ ...previous, quiz: true }));
                onCompleted();
              }}
              onAskAi={onAskAi}
              aiAvailable={aiAvailable}
            />
          ) : null}

          {/* 读完一课就把句子变成复习卡（V13 W3）：此时正是「我刚看过这句」的时刻 */}
          {stage === "quiz" || stage === "done" ? (
            <MiningPanel lessonId={lesson.id} />
          ) : null}

          {/* 阶段切换：底部一行，前后一致 */}
          <footer className="en-workspace-foot">
            <button
              type="button"
              className="en-ghost-btn"
              disabled={!prevStage(stage)}
              onClick={() => {
                const target = prevStage(stage);
                if (target) changeStage(target);
              }}
            >
              上一步
            </button>
            <span className="en-muted">
              {completedCount > 0 ? `已逐句 ${completedCount}/${sentences.length}` : null}
              {" · "}
              {formatTimestamp(positionMs)}
            </span>
            <button
              type="button"
              className="en-primary-btn"
              disabled={!nextStage(stage) || stage === "done"}
              onClick={() => {
                const target = nextStage(stage);
                if (target) changeStage(target);
              }}
            >
              下一步
            </button>
          </footer>
        </main>

        {/* ---- 右侧栏：AI Tutor / 本课生词 ---- */}
        <aside className="en-workspace-side">
          <section className="en-side-section">
            <header>
              <h4>
                <Sparkle size={14} /> AI Tutor
              </h4>
            </header>
            {aiAvailable && onAskAi ? (
              <div className="en-ai-actions">
                {activeSentence ? (
                  <>
                    <button
                      type="button"
                      className="en-link-btn"
                      onClick={() =>
                        askAi(
                          `请解释这句话：\n"${activeSentence.english}"\n${
                            activeSentence.chinese ? `参考译文：${activeSentence.chinese}` : ""
                          }\n请从语法、词汇、语气三个角度说明，并指出学习者容易错的地方。`,
                        )
                      }
                    >
                      解释这句话
                    </button>
                    <button
                      type="button"
                      className="en-link-btn"
                      onClick={() =>
                        askAi(
                          `请给我 3 个类似 "${activeSentence.english}" 的英文例句，并解释它们的差别。`,
                        )
                      }
                    >
                      给我类似例句
                    </button>
                    <button
                      type="button"
                      className="en-link-btn"
                      onClick={() =>
                        askAi(
                          `这一课是 New Concept English Lesson ${lesson.lesson_no}。请从课文中挑 3 个语法点，结合课文原句讲解，并给我 2 道练习题。`,
                        )
                      }
                    >
                      讲讲本课语法
                    </button>
                  </>
                ) : (
                  <p className="en-muted">
                    先在下方选择一句话，AI 就能结合当前句子讲解。
                  </p>
                )}
              </div>
            ) : (
              <p className="en-muted">
                AI 未配置。整课学习（单词 / 听力 / 精读 / 跟读 / 测验）不依赖 AI，可正常使用。
              </p>
            )}
          </section>

          <section className="en-side-section">
            <header>
              <h4>
                <Note size={14} /> 本课生词（{vocab.length}）
              </h4>
            </header>
            <ul className="en-side-vocab">
              {vocab.slice(0, 40).map((item: VocabWithState) => (
                <li key={item.word}>
                  <button
                    type="button"
                    className="en-side-vocab-item"
                    onClick={() =>
                      setPopover({ word: item.word, sentence: item.context ?? null })
                    }
                  >
                    <span className={cx("en-word-dot", `is-${item.state}`)} />
                    <span className="en-side-vocab-word">{item.word}</span>
                    <span className="en-side-vocab-zh">
                      {item.translation_zh ? item.translation_zh.split("\n")[0] : "—"}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        </aside>
      </div>

      {/* ---- 查词浮层（覆盖层，不跳页）---- */}
      {popover ? (
        <div className="en-popover-layer" onClick={() => setPopover(null)}>
          <div onClick={(event) => event.stopPropagation()}>
            <WordPopover
              word={popover.word}
              sentence={popover.sentence}
              lessonId={lesson.id}
              onMark={markWord}
              onAskAi={askAi}
              aiAvailable={aiAvailable}
              onClose={() => setPopover(null)}
            />
          </div>
        </div>
      ) : null}

      {stageDone.quiz && stage === "quiz" ? (
        <button
          type="button"
          className="en-floating-close"
          onClick={onExit}
          title="回到课程"
        >
          <X size={14} />
        </button>
      ) : null}
    </div>
  );
}