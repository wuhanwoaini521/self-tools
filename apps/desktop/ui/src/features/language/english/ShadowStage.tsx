/**
 * 跟读阶段（Shadowing）。
 *
 * 流程：**播放一句 → 停顿 → 用户跟读 → 下一句**。
 *
 * 录音能力说明（诚实边界）：
 * - 浏览器 MediaRecorder 可用时提供「录音 + 回放」，用于自己对比发音；
 * - **不提供 AI 发音打分**：项目内没有可靠的语音识别能力，
 *   编造一个「发音 92 分」比没有这个功能更糟；
 * - 未来接入 Speech Recognition 后可在同一位置增加评分。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Microphone, Pause, Play, Stop, ArrowLeft, ArrowRight } from "@phosphor-icons/react";
import type { LessonSentence } from "../../../types";
import { cx } from "../languageUi";
import { formatTimestamp } from "./shared";

export interface ShadowStageProps {
  sentences: LessonSentence[];
  startSeq: number;
  activeSeq: number | null;
  audioUrl: string | null;
  onSelect: (seq: number) => void;
  onProgressSeq: (seq: number) => void;
  onFinish: () => void;
}

type RecorderState = "idle" | "recording" | "recorded";

export function ShadowStage({
  sentences,
  startSeq,
  activeSeq,
  audioUrl,
  onSelect,
  onProgressSeq,
  onFinish,
}: ShadowStageProps) {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [index, setIndex] = useState(Math.min(Math.max(0, startSeq), Math.max(0, sentences.length - 1)));
  const [paused, setPaused] = useState(false);
  const [recorderState, setRecorderState] = useState<RecorderState>("idle");
  const [recordUrl, setRecordUrl] = useState<string | null>(null);
  const [recordError, setRecordError] = useState<string | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);

  const current = sentences[index];
  const recordingSupported =
    typeof window !== "undefined" &&
    typeof MediaRecorder !== "undefined" &&
    Boolean(navigator.mediaDevices?.getUserMedia);

  // 同步外部（播放器）选中的句子。
  useEffect(() => {
    if (activeSeq !== null && activeSeq !== index) {
      setIndex(activeSeq);
      onProgressSeq(activeSeq);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeSeq]);

  useEffect(() => {
    if (!current) return;
    onProgressSeq(current.sequence);
  }, [current, onProgressSeq]);

  // 切换句子时重置跟读状态。
  useEffect(() => {
    setPaused(false);
    setRecorderState("idle");
    setRecordUrl((previous) => {
      if (previous) URL.revokeObjectURL(previous);
      return null;
    });
  }, [index]);

  // 播放本句（自动到句尾后进入「等待跟读」）。
  const playSentence = useCallback(async () => {
    const audio = audioRef.current;
    if (!audio || !current) return;
    audio.currentTime = current.start_ms / 1000;
    await audio.play().catch(() => undefined);
  }, [current]);

  useEffect(() => {
    if (!audioUrl || !current) return;
    // 进入阶段自动播第一句（不阻塞其它交互）。
    void playSentence();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [audioUrl, current?.sequence]);

  const startRecording = useCallback(async () => {
    setRecordError(null);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const recorder = new MediaRecorder(stream);
      chunksRef.current = [];
      recorder.ondataavailable = (event) => {
        if (event.data.size > 0) chunksRef.current.push(event.data);
      };
      recorder.onstop = () => {
        const blob = new Blob(chunksRef.current, { type: "audio/webm" });
        setRecordUrl(URL.createObjectURL(blob));
        setRecorderState("recorded");
        stream.getTracks().forEach((track) => track.stop());
      };
      recorder.start();
      mediaRecorderRef.current = recorder;
      setRecorderState("recording");
    } catch (cause) {
      setRecordError(
        cause instanceof Error ? `无法录音：${cause.message}` : "无法录音（权限被拒绝？）",
      );
    }
  }, []);

  const stopRecording = useCallback(() => {
    mediaRecorderRef.current?.stop();
    mediaRecorderRef.current = null;
  }, []);

  const goto = useCallback(
    (next: number) => {
      const clamped = Math.max(0, Math.min(sentences.length - 1, next));
      setIndex(clamped);
      onSelect(clamped);
    },
    [onSelect, sentences.length],
  );

  if (!current) {
    return (
      <section className="en-stage">
        <p className="en-muted">这一课没有句子可用于跟读。</p>
        <button type="button" className="en-primary-btn" onClick={onFinish}>
          跳过，进入测验
        </button>
      </section>
    );
  }

  return (
    <section className="en-stage en-shadow-stage">
      <header className="en-stage-head">
        <div>
          <h2>Shadowing</h2>
          <p className="en-muted">
            第 {index + 1} / {sentences.length} 句 · 播放 → 跟读 → 下一句
          </p>
        </div>
      </header>

      {audioUrl ? (
        // eslint-disable-next-line jsx-a11y/media-has-caption
        <audio ref={audioRef} src={audioUrl} preload="auto" />
      ) : null}

      <div className="en-shadow-card">
        <span className="en-sentence-time">{formatTimestamp(current.start_ms)}</span>
        <p className="en-shadow-en">{current.english}</p>
        {current.chinese ? <p className="en-sentence-zh">{current.chinese}</p> : null}

        <div className="en-shadow-actions">
          <button type="button" className="en-ghost-btn" onClick={() => void playSentence()}>
            <Play size={14} /> 播放这句
          </button>

          {recordingSupported ? (
            recorderState === "recording" ? (
              <button type="button" className="en-danger-btn" onClick={stopRecording}>
                <Stop size={14} /> 停止录音
              </button>
            ) : (
              <button type="button" className="en-ghost-btn" onClick={() => void startRecording()}>
                <Microphone size={14} /> 录我的声音
              </button>
            )
          ) : (
            <span className="en-muted">当前环境不支持录音（浏览器麦克风不可用）</span>
          )}

          {recordUrl ? (
            <audio
              className="en-shadow-playback"
              controls
              src={recordUrl}
              aria-label="我的录音回放"
            />
          ) : null}
        </div>

        {recordError ? <p className="en-inline-error">{recordError}</p> : null}
        {recorderState === "recording" ? (
          <p className={cx("en-recording-live")}>
            <span className="en-rec-dot" /> 录音中… 读完点「停止录音」
          </p>
        ) : null}
      </div>

      <div className="en-shadow-foot">
        <button
          type="button"
          className="en-ghost-btn"
          onClick={() => goto(index - 1)}
          disabled={index === 0}
        >
          <ArrowLeft size={14} /> 上一句
        </button>
        <button
          type="button"
          className="en-ghost-btn"
          onClick={() => setPaused((previous) => !previous)}
        >
          {paused ? <Play size={14} /> : <Pause size={14} />}
          {paused ? "继续" : "暂停"}
        </button>
        <button
          type="button"
          className="en-primary-btn"
          onClick={() => goto(index + 1)}
          disabled={index >= sentences.length - 1}
        >
          跟读完了，下一句 <ArrowRight size={14} />
        </button>
      </div>

      <button type="button" className="en-link-btn" onClick={onFinish}>
        跳过跟读，进入测验 →
      </button>
    </section>
  );
}