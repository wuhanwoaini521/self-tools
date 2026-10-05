/**
 * 沉浸式精读（Immersive Reader）——「打开就想读下去」的那一屏。
 *
 * 设计取自现代阅读器与沉浸式学习 App 的共同做法（Language Reactor / Migaku 的
 * 精读模式、Duolingo 的单任务屏、精听法的「隐藏原文」）：
 * - **一屏一件事**：全屏、无侧栏、无导航，只剩课文与当前句；
 * - **跟着嘴走**：句级高亮跟随音频（LRC 时间轴），空格/←/→ 就能控制；
 * - **i+1 输入**：译文可「隐藏 / 逐句 / 全显」——想沉浸就全隐藏，卡住再看当前句；
 * - **点词即查**：浮层，不跳页，Esc 关闭；
 * - **诚实降级**：没有音频时用浏览器 TTS 逐句朗读（并说明原因），不是静默无声。
 *
 * 状态回写：当前句下标 + 本次会话秒数，由 LessonWorkspace 决定怎么存。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Eye, EyeSlash, GearSix, SpeakerHigh, X } from "@phosphor-icons/react";
import type { LessonSentence, VocabWithState, WordMark } from "../../../types";
import { speak, stopSpeaking, speechSupported } from "../tts";
import { cx } from "../languageUi";
import { WordPopover } from "./WordPopover";
import { SentenceText } from "./SentenceText";
import {
  DEFAULT_PREFS,
  estimateSentenceMs,
  findSentenceIndexAt,
  formatSessionClock,
  loadPrefs,
  nextTranslationMode,
  savePrefs,
  shouldShowTranslation,
  translationModeLabel,
  type ReadingPrefs,
  type TranslationMode,
} from "./immersive";

export interface ImmersiveReaderProps {
  sentences: LessonSentence[];
  vocab: VocabWithState[];
  /** 课程音频 URL；没有时用浏览器 TTS 逐句朗读。 */
  audioUrl: string | null;
  /** 音频为什么不可用（如实展示，不静默）。 */
  audioMissingReason?: string | null;
  lessonLabel: string;
  /** 课程 id（查词要记录「在哪学的」）。 */
  lessonId: string;
  /** 退出时的回调，参数为「最后停留的句子下标」与「本次会话秒数」。 */
  onExit: (sentenceIndex: number, sessionSeconds: number) => void;
  /** 三态标记单词。 */
  onMark: (word: string, mark: WordMark) => Promise<void>;
  /** 打开 AI（可选）。 */
  onAskAi?: (prompt: string) => void;
  aiAvailable?: boolean;
}

export function ImmersiveReader({
  sentences,
  vocab,
  audioUrl,
  audioMissingReason,
  lessonLabel,
  lessonId,
  onExit,
  onMark,
  onAskAi,
  aiAvailable = false,
}: ImmersiveReaderProps) {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const startedAtRef = useRef(Date.now());
  const [activeIndex, setActiveIndex] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [rate, setRate] = useState(1);
  const [prefs, setPrefs] = useState<ReadingPrefs>(DEFAULT_PREFS);
  const [showSettings, setShowSettings] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [popover, setPopover] = useState<{ word: string; sentence: string | null } | null>(null);

  useEffect(() => {
    setPrefs(loadPrefs(window.localStorage));
  }, []);
  // 偏好改动即时落盘（字号是「手感」，刷新后要还在）。
  useEffect(() => {
    savePrefs(prefs, window.localStorage);
  }, [prefs]);

  const active = sentences[activeIndex] ?? null;

  // 会话计时（顶部时钟）：只统计本次沉浸时间。
  useEffect(() => {
    const timer = window.setInterval(() => {
      setElapsed(Math.floor((Date.now() - startedAtRef.current) / 1000));
    }, 1000);
    return () => window.clearInterval(timer);
  }, []);

  const exit = useCallback(() => {
    const seconds = Math.floor((Date.now() - startedAtRef.current) / 1000);
    stopSpeaking();
    onExit(activeIndex, seconds);
  }, [activeIndex, onExit]);

  // ---- 播放：优先音频，其次浏览器 TTS ----
  const playSentence = useCallback(
    async (index: number) => {
      const sentence = sentences[index];
      if (!sentence) return;
      setActiveIndex(index);
      const audio = audioRef.current;
      if (audio && audioUrl) {
        audio.currentTime = sentence.start_ms / 1000;
        audio.playbackRate = rate;
        await audio.play().catch(() => undefined);
        setPlaying(true);
        return;
      }
      // 无音频：TTS 兜底（并已在界面上说明原因）。
      if (speechSupported()) {
        speak(sentence.english, "eng");
        setPlaying(true);
        const expected = estimateSentenceMs(sentence.english) / 1000 / Math.max(0.5, rate);
        window.setTimeout(() => setPlaying(false), expected * 1000);
      }
    },
    [audioUrl, rate, sentences],
  );

  const togglePlay = useCallback(() => {
    const audio = audioRef.current;
    if (audio && audioUrl) {
      if (audio.paused) {
        void audio.play().catch(() => undefined);
        setPlaying(true);
      } else {
        audio.pause();
        setPlaying(false);
      }
      return;
    }
    if (playing) {
      stopSpeaking();
      setPlaying(false);
    } else {
      void playSentence(activeIndex);
    }
  }, [activeIndex, audioUrl, playSentence, playing]);

  const step = useCallback(
    (delta: number) => {
      const next = Math.max(0, Math.min(sentences.length - 1, activeIndex + delta));
      void playSentence(next);
    },
    [activeIndex, playSentence, sentences.length],
  );

  // 音频时间 → 当前句（卡拉OK式高亮）。
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;
    const onTime = () => {
      const ms = audio.currentTime * 1000;
      const index = findSentenceIndexAt(sentences, ms);
      if (index >= 0) setActiveIndex((current) => (current === index ? current : index));
    };
    const onEnded = () => setPlaying(false);
    audio.addEventListener("timeupdate", onTime);
    audio.addEventListener("ended", onEnded);
    return () => {
      audio.removeEventListener("timeupdate", onTime);
      audio.removeEventListener("ended", onEnded);
    };
  }, [audioUrl, sentences]);

  // ---- 键盘流：沉浸模式不该让人去够鼠标 ----
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      // 输入框里打字时不劫持按键。
      if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      switch (event.key) {
        case " ":
          event.preventDefault();
          togglePlay();
          break;
        case "ArrowRight":
          event.preventDefault();
          step(1);
          break;
        case "ArrowLeft":
          event.preventDefault();
          step(-1);
          break;
        case "e":
        case "E":
          setPrefs((previous) => ({
            ...previous,
            translationMode: previous.translationMode === "hidden" ? "current" : "hidden",
          }));
          break;
        case "c":
        case "C":
          setPrefs((previous) => ({
            ...previous,
            translationMode: nextTranslationMode(previous.translationMode),
          }));
          break;
        case "Escape":
          event.preventDefault();
          if (popover) setPopover(null);
          else exit();
          break;
        default:
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [exit, popover, step, togglePlay]);

  const cycleMode = useCallback(() => {
    setPrefs((previous) => ({
      ...previous,
      translationMode: nextTranslationMode(previous.translationMode),
    }));
  }, []);

  const style = useMemo(
    () =>
      ({
        "--im-font-size": `${prefs.fontSize}px`,
        "--im-line-height": String(prefs.lineHeight),
        "--im-measure": `${prefs.measure}ch`,
      }) as React.CSSProperties,
    [prefs],
  );

  const markedCount = useMemo(
    () => sentences.filter((_, index) => index < activeIndex).length,
    [activeIndex, sentences],
  );

  return (
    <div className="im-reader" style={style} role="dialog" aria-label="沉浸式精读">
      {audioUrl ? (
        // eslint-disable-next-line jsx-a11y/media-has-caption
        <audio ref={audioRef} src={audioUrl} preload="auto" hidden />
      ) : null}

      <header className="im-top">
        <span className="im-label">{lessonLabel}</span>
        <span className="im-clock" title="本次沉浸时长">
          {formatSessionClock(elapsed)} · {markedCount}/{sentences.length} 句
        </span>
        <div className="im-top-actions">
          <button
            type="button"
            className={cx("im-btn", playing && "is-on")}
            onClick={togglePlay}
            title="播放 / 暂停（空格）"
          >
            <SpeakerHigh size={16} />
          </button>
          <button
            type="button"
            className="im-btn"
            onClick={cycleMode}
            title={`${translationModeLabel(prefs.translationMode)}（C 键循环，E 键快速隐藏）`}
          >
            {prefs.translationMode === "hidden" ? <EyeSlash size={16} /> : <Eye size={16} />}
            <span className="im-btn-text">{translationModeLabel(prefs.translationMode)}</span>
          </button>
          <label className="im-rate" title="语速">
            <span>{rate.toFixed(1)}×</span>
            <input
              type="range"
              min={0.6}
              max={1.5}
              step={0.1}
              value={rate}
              onChange={(event) => {
                const next = Number(event.target.value);
                setRate(next);
                const audio = audioRef.current;
                if (audio) audio.playbackRate = next;
              }}
              aria-label="语速"
            />
          </label>
          <button
            type="button"
            className="im-btn"
            onClick={() => setShowSettings((previous) => !previous)}
            title="阅读设置"
          >
            <GearSix size={16} />
          </button>
          <button type="button" className="im-btn" onClick={exit} title="退出（Esc）">
            <X size={16} />
          </button>
        </div>
      </header>

      {!audioUrl && audioMissingReason ? (
        <p className="im-audio-note">{audioMissingReason}（已改用浏览器朗读）</p>
      ) : null}

      {showSettings ? (
        <div className="im-settings">
          <label>
            字号 {prefs.fontSize}px
            <input
              type="range"
              min={14}
              max={34}
              step={1}
              value={prefs.fontSize}
              onChange={(event) =>
                setPrefs((previous) => ({ ...previous, fontSize: Number(event.target.value) }))
              }
            />
          </label>
          <label>
            行距 {prefs.lineHeight.toFixed(1)}
            <input
              type="range"
              min={1.2}
              max={2.6}
              step={0.1}
              value={prefs.lineHeight}
              onChange={(event) =>
                setPrefs((previous) => ({ ...previous, lineHeight: Number(event.target.value) }))
              }
            />
          </label>
          <label>
            栏宽 {prefs.measure}ch
            <input
              type="range"
              min={40}
              max={100}
              step={2}
              value={prefs.measure}
              onChange={(event) =>
                setPrefs((previous) => ({ ...previous, measure: Number(event.target.value) }))
              }
            />
          </label>
        </div>
      ) : null}

      <main className="im-body">
        <article className="im-text">
          {sentences.map((sentence, index) => {
            const isActive = index === activeIndex;
            const showZh = shouldShowTranslation(prefs.translationMode, index, activeIndex);
            return (
              <p
                key={sentence.id}
                className={cx("im-sentence", isActive && "is-active")}
                data-seq={index}
                onClick={() => void playSentence(index)}
              >
                <SentenceText
                  text={sentence.english}
                  vocab={vocab}
                  onWordClick={(word) => {
                    setPopover({ word, sentence: sentence.english });
                  }}
                  sentenceId={sentence.id}
                  active={isActive}
                />
                {showZh && sentence.chinese ? (
                  <span className="im-sentence-zh">{sentence.chinese}</span>
                ) : null}
              </p>
            );
          })}
        </article>
      </main>

      <footer className="im-foot">
        <button type="button" className="im-btn" onClick={() => step(-1)} title="上一句（←）">
          ←
        </button>
        <span className="im-muted">
          空格 播放/暂停 · ←/→ 换句 · E 隐藏译文 · C 切换译文模式 · Esc 退出
        </span>
        <button type="button" className="im-btn" onClick={() => step(1)} title="下一句（→）">
          →
        </button>
      </footer>

      {popover ? (
        <div className="im-popover-layer" onClick={() => setPopover(null)}>
          <div onClick={(event) => event.stopPropagation()}>
            <WordPopover
              word={popover.word}
              sentence={popover.sentence}
              lessonId={lessonId}
              onMark={onMark}
              onAskAi={
                onAskAi && aiAvailable
                  ? (prompt: string) => onAskAi(`关于「${popover.word}」：${prompt}`)
                  : undefined
              }
              aiAvailable={aiAvailable}
              onClose={() => setPopover(null)}
            />
          </div>
        </div>
      ) : null}
    </div>
  );
}

export type { TranslationMode };
