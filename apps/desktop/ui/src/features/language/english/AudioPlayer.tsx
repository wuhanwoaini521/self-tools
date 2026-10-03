/**
 * 课时音频播放器：单一 `<audio>` 源 + 时间轴同步。
 *
 * 真实数据约束：
 * - 音源是导入时拷贝到本地数据目录的 MP3（离线可用，不依赖远程服务器）；
 * - 没有音频时（缺 MP3 / 未导入）**如实禁用**并说明原因，不播放假音频。
 *
 * 交互：
 * - 播放 / 暂停、上一句 / 下一句（A-B 区段）、倍速、进度拖动；
 * - 通过 `onTick` 把当前毫秒暴露给逐句/跟读阶段做高亮同步；
 * - 通过 `onSeekRange` 实现 A-B 复读（逐句精听）。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowCounterClockwise,
  Pause,
  Play,
  Rewind,
  FastForward,
  SpeakerHigh,
} from "@phosphor-icons/react";
import type { LessonSentence } from "../../../types";
import { cx } from "../languageUi";

export interface AudioPlayerProps {
  /** 已加载的音频 object URL；null = 本课没有音频。 */
  src: string | null;
  /** 没有音频时给用户看的原因。 */
  missingReason?: string | null;
  sentences: LessonSentence[];
  /** 起始位置（断点续学）。 */
  initialPositionMs?: number;
  /** 当前聚焦句（外部驱动 A-B 复读与高亮）。 */
  activeSeq?: number | null;
  /** 播放速率档位。 */
  rate?: number;
  onRateChange?: (rate: number) => void;
  /** 音频位置变化（节流由调用方决定）。 */
  onTick?: (positionMs: number) => void;
  /** 用户主动 seek（用于保存进度）。 */
  onSeek?: (positionMs: number) => void;
  /** A-B 复读开关状态变化。 */
  onRepeatToggle?: (enabled: boolean) => void;
  compact?: boolean;
}

const RATES = [0.75, 1, 1.25];

function formatTime(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

export function AudioPlayer({
  src,
  missingReason,
  sentences,
  initialPositionMs = 0,
  activeSeq = null,
  rate,
  onRateChange,
  onTick,
  onSeek,
  onRepeatToggle,
  compact = false,
}: AudioPlayerProps) {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(initialPositionMs);
  const [duration, setDuration] = useState(0);
  const [repeat, setRepeat] = useState(false);
  const effectiveRate = rate ?? 1;

  const activeSentence = useMemo(
    () => sentences.find((sentence) => sentence.sequence === activeSeq) ?? null,
    [activeSeq, sentences],
  );

  // ---- 断点续学：首次 metadata 就绪后跳到保存位置 ----
  const restoredRef = useRef(false);
  useEffect(() => {
    restoredRef.current = false;
    setPosition(initialPositionMs);
  }, [initialPositionMs, src]);

  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;
    audio.playbackRate = effectiveRate;
  }, [effectiveRate, src]);

  const handleLoaded = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;
    setDuration(Number.isFinite(audio.duration) ? audio.duration * 1000 : 0);
    if (!restoredRef.current && initialPositionMs > 0) {
      restoredRef.current = true;
      audio.currentTime = Math.min(initialPositionMs / 1000, audio.duration || 0);
      setPosition(initialPositionMs);
    }
  }, [initialPositionMs]);

  // ---- A-B 复读：到达当前句尾就回到句首 ----
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio || !repeat || !activeSentence) return;
    const onTimeUpdate = () => {
      const ms = audio.currentTime * 1000;
      if (ms >= activeSentence.end_ms) {
        audio.currentTime = activeSentence.start_ms / 1000;
      }
    };
    audio.addEventListener("timeupdate", onTimeUpdate);
    return () => audio.removeEventListener("timeupdate", onTimeUpdate);
  }, [repeat, activeSentence]);

  const togglePlay = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;
    if (audio.paused) {
      void audio.play().catch(() => setPlaying(false));
    } else {
      audio.pause();
    }
  }, []);

  const seekMs = useCallback(
    (ms: number) => {
      const audio = audioRef.current;
      if (!audio) return;
      const clamped = Math.max(0, ms);
      audio.currentTime = clamped / 1000;
      setPosition(clamped);
      onSeek?.(clamped);
    },
    [onSeek],
  );

  // ---- 上一句 / 下一句：跳到目标句开头并继续播放 ----
  const gotoSentence = useCallback(
    (sequence: number, autoplay = true) => {
      const target =
        sentences.find((sentence) => sentence.sequence === sequence) ?? null;
      if (!target) return;
      seekMs(target.start_ms);
      if (autoplay) {
        const audio = audioRef.current;
        void audio?.play().catch(() => undefined);
      }
    },
    [seekMs, sentences],
  );

  const currentSeq = useMemo(() => {
    const found = sentences.find(
      (sentence) => position >= sentence.start_ms && position < sentence.end_ms,
    );
    return found?.sequence ?? null;
  }, [position, sentences]);

  const toggleRepeat = useCallback(() => {
    setRepeat((previous) => {
      onRepeatToggle?.(!previous);
      return !previous;
    });
  }, [onRepeatToggle]);

  // ---- 时间轴事件：位置回传（用于保存进度）----
  const handleTimeUpdate = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;
    const ms = audio.currentTime * 1000;
    setPosition(ms);
    onTick?.(ms);
  }, [onTick]);

  useEffect(() => {
    // 进度节流回传：每 5 秒一次，避免写库过于频繁。
    const interval = window.setInterval(() => {
      const audio = audioRef.current;
      if (audio && !audio.paused) onTick?.(audio.currentTime * 1000);
    }, 5000);
    return () => window.clearInterval(interval);
  }, [onTick]);

  if (!src) {
    return (
      <div className="en-audio is-missing" role="status">
        <SpeakerHigh size={16} />
        <span>
          {missingReason ?? "本课没有音频（导入时未找到 .mp3），听力与跟读不可用"}
        </span>
      </div>
    );
  }

  const totalMs = duration || (sentences.at(-1)?.end_ms ?? 0);

  return (
    <div className={cx("en-audio", compact && "is-compact")}>
      {/* eslint-disable-next-line jsx-a11y/media-has-caption */}
      <audio
        ref={audioRef}
        src={src}
        preload="metadata"
        onLoadedMetadata={handleLoaded}
        onTimeUpdate={handleTimeUpdate}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={() => setPlaying(false)}
      />
      <div className="en-audio-controls">
        <button
          type="button"
          className="en-icon-btn"
          onClick={() => gotoSentence((currentSeq ?? 0) - 1)}
          disabled={currentSeq === null || currentSeq <= 0}
          title="上一句"
          aria-label="上一句"
        >
          <Rewind size={15} />
        </button>
        <button
          type="button"
          className="en-icon-btn is-primary"
          onClick={togglePlay}
          title={playing ? "暂停" : "播放"}
          aria-label={playing ? "暂停" : "播放"}
        >
          {playing ? <Pause size={16} weight="fill" /> : <Play size={16} weight="fill" />}
        </button>
        <button
          type="button"
          className="en-icon-btn"
          onClick={() => gotoSentence((currentSeq ?? -1) + 1)}
          disabled={currentSeq !== null && currentSeq >= sentences.length - 1}
          title="下一句"
          aria-label="下一句"
        >
          <FastForward size={15} />
        </button>
        <button
          type="button"
          className={cx("en-icon-btn", repeat && "is-on")}
          onClick={toggleRepeat}
          title={repeat ? "关闭逐句复读" : "逐句复读（自动在句尾回到句首）"}
          aria-pressed={repeat}
        >
          <ArrowCounterClockwise size={15} />
        </button>

        <span className="en-audio-time">
          {formatTime(position)} / {formatTime(totalMs)}
        </span>

        <input
          className="en-audio-seek"
          type="range"
          min={0}
          max={Math.max(1, totalMs)}
          value={Math.min(position, totalMs)}
          onChange={(event) => seekMs(Number(event.target.value))}
          aria-label="播放进度"
        />

        <div className="en-rate-group" role="group" aria-label="播放速度">
          {RATES.map((value) => (
            <button
              key={value}
              type="button"
              className={cx("en-rate", effectiveRate === value && "is-on")}
              onClick={() => onRateChange?.(value)}
              aria-pressed={effectiveRate === value}
            >
              {value}x
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}