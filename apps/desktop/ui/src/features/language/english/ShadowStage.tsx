/**
 * 跟读阶段（Shadowing）——**开口的练习场**（V13 W2）。
 *
 * 流程：**播放一句 → 停顿 → 你跟读 → 看反馈 → 下一句**。
 *
 * ## 反馈从哪来（诚实说明）
 *
 * - 环境支持语音识别（Chrome / Edge + 麦克风 + 网络）→ 「朗读评分」按钮：
 *   识别你说出的话 → 发给后端用 `core::language::speaking` 打分
 *   （准确度 / 完整度 / 流利度 + 词级差异）→ 展示**漏了哪个词、哪个词说错**；
 * - 环境不支持 → **不给分**，只保留「录音 + 回放」自我对照，并说明原因。
 *   编一个「发音 92 分」比没有这个功能更糟。
 *
 * 目标句由**服务端查库**得到（前端不传目标句），否则等于自己给自己判分。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import {
  Microphone,
  Pause,
  Play,
  Stop,
  ArrowLeft,
  ArrowRight,
  Waveform,
} from "@phosphor-icons/react";
import type { LessonSentence } from "../../../types";
import { cx } from "../languageUi";
import { formatTimestamp } from "./shared";
import { englishClient } from "./englishClient";
import {
  listenOnce,
  referenceDurationMs,
  speechCapability,
  speechErrorText,
} from "../speech";
import type { ShadowScoreResult } from "../speakingTypes";

export interface ShadowStageProps {
  lessonId: string;
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
  lessonId,
  sentences,
  startSeq,
  activeSeq,
  audioUrl,
  onSelect,
  onProgressSeq,
  onFinish,
}: ShadowStageProps) {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [index, setIndex] = useState(
    Math.min(Math.max(0, startSeq), Math.max(0, sentences.length - 1)),
  );
  const [paused, setPaused] = useState(false);
  const [recorderState, setRecorderState] = useState<RecorderState>("idle");
  const [recordUrl, setRecordUrl] = useState<string | null>(null);
  const [recordError, setRecordError] = useState<string | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);

  // 评分状态（V13 W2）
  const capability = speechCapability();
  const [scoring, setScoring] = useState(false);
  const [score, setScore] = useState<ShadowScoreResult | null>(null);
  const [scoreNote, setScoreNote] = useState<string | null>(null);

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

  // 切换句子时重置跟读与评分状态。
  useEffect(() => {
    setPaused(false);
    setRecorderState("idle");
    setScore(null);
    setScoreNote(null);
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

  /**
   * 朗读评分：识别 → 后端打分 → 展示词级差异。
   *
   * 识别失败/无结果时**只说明原因**，不给任何分数。
   */
  const scoreSpeech = useCallback(async () => {
    if (!current) return;
    setScoring(true);
    setScoreNote(null);
    setScore(null);
    try {
      const outcome = await listenOnce();
      if (outcome.error || !outcome.transcript) {
        setScoreNote(
          outcome.error
            ? speechErrorText(outcome.error)
            : "没有识别到内容，这次不给分数（再试一次，或先听一遍再跟读）。",
        );
        return;
      }
      const result = await englishClient.shadowScore({
        lessonId,
        sentenceSeq: current.sequence,
        transcript: outcome.transcript,
        durationMs: outcome.durationMs,
        targetMs: referenceDurationMs(current.english),
        longPausesMs: [],
      });
      setScore(result);
    } catch (cause) {
      setScoreNote(
        cause instanceof Error ? `评分失败：${cause.message}` : "评分失败（服务不可用？）",
      );
    } finally {
      setScoring(false);
    }
  }, [current, lessonId]);

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
            第 {index + 1} / {sentences.length} 句 · 播放 → 跟读 → 看反馈 → 下一句
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

          {capability.supported ? (
            <button
              type="button"
              className="en-primary-btn"
              onClick={() => void scoreSpeech()}
              disabled={scoring}
              title="朗读这句，识别你的发音并给出准确度 / 完整度 / 流利度"
            >
              <Waveform size={14} />
              {scoring ? "在听…读完自动结束" : "朗读评分"}
            </button>
          ) : null}

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

        {/* 评分反馈：三个分项 + 词级差异，不给「综合分」了事 */}
        {score ? (
          <div className="en-score-card" role="status">
            <div className="en-score-row">
              <ScoreChip label="准确度" value={score.accuracy} />
              <ScoreChip label="完整度" value={score.completeness} />
              <ScoreChip label="流利度" value={score.fluency} />
            </div>
            <p className="en-muted">
              识别到：「{score.transcript}」
            </p>
            <div className="en-score-diff">
              {score.missing.length > 0 ? (
                <p>
                  <span className="en-diff-tag is-missing">漏说</span> {score.missing.join(" · ")}
                </p>
              ) : null}
              {score.wrong.length > 0 ? (
                <p>
                  <span className="en-diff-tag is-wrong">说错</span> {score.wrong.join(" · ")}
                </p>
              ) : null}
              {score.extra.length > 0 ? (
                <p>
                  <span className="en-diff-tag is-extra">多说</span> {score.extra.join(" · ")}
                </p>
              ) : null}
              {score.missing.length === 0 &&
              score.wrong.length === 0 &&
              score.extra.length === 0 ? (
                <p className="en-muted">逐词都对上了 —— 下一句。</p>
              ) : null}
            </div>
          </div>
        ) : null}

        {!capability.supported ? (
          <p className="en-muted">
            {capability.reason} 你仍然可以「录我的声音」回放对照 —— 只是这一句拿不到分数。
          </p>
        ) : null}
        {scoreNote ? <p className="en-inline-error">{scoreNote}</p> : null}
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

/** 一个分项（0–100，带颜色分层：≥80 好 / ≥60 还行 / 更低要再练）。 */
function ScoreChip({ label, value }: { label: string; value: number }) {
  const tone = value >= 80 ? "good" : value >= 60 ? "mid" : "low";
  return (
    <span className={cx("en-score-chip", `is-${tone}`)}>
      <strong>{value}</strong>
      <span>{label}</span>
    </span>
  );
}
