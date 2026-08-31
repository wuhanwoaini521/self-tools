import { invoke } from "@tauri-apps/api/core";
import { Microphone, Play, Stop } from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { LanguageCode, SentenceRecord, SpeakingScore } from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import { estimateSpeechMs, speak } from "./tts";

interface SpeakPanelProps {
  language: LanguageCode;
  setNotice: (message: string) => void;
}

/**
 * Speak（#67/#68）：目标句 → Play TTS → 录音 → 转写（V1 为手动输入或 Web Speech 自动填充）→
 * 打分为 Accuracy/Completeness/Fluency（Missing/Wrong/Extra + 时长），不做口音分。
 */
export function SpeakPanel({ language, setNotice }: SpeakPanelProps) {
  const [target, setTarget] = useState<SentenceRecord | null>(null);
  const [transcript, setTranscript] = useState("");
  const [recordedMs, setRecordedMs] = useState(0);
  const [score, setScore] = useState<SpeakingScore | null>(null);
  const [recording, setRecording] = useState(false);
  const recorder = useRef<MediaRecorder | null>(null);
  const chunks = useRef<Blob[]>([]);
  const startedAt = useRef<number>(0);

  const pickTarget = useCallback(async (next?: SentenceRecord) => {
    if (!isTauriRuntime()) return;
    if (next) {
      setTarget(next);
      setTranscript("");
      setScore(null);
      setRecordedMs(0);
      return;
    }
    try {
      const rows = await invoke<SentenceRecord[]>("language_sentences", { language, limit: 12 });
      const sentence = rows[Math.floor(Math.random() * Math.max(1, rows.length))] ?? null;
      setTarget(sentence);
      setTranscript("");
      setScore(null);
      setRecordedMs(0);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [language, setNotice]);

  useEffect(() => { void pickTarget(); }, [pickTarget, language]);

  const startRecording = async () => {
    if (!navigator.mediaDevices?.getUserMedia) {
      setNotice("当前浏览器不支持录音；可直接输入你的转写文本进行评分。");
      return;
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const media = new MediaRecorder(stream);
      chunks.current = [];
      media.ondataavailable = (event) => { if (event.data.size > 0) chunks.current.push(event.data); };
      media.onstop = () => {
        stream.getTracks().forEach((track) => track.stop());
        setRecordedMs(Date.now() - startedAt.current);
      };
      recorder.current = media;
      startedAt.current = Date.now();
      media.start();
      setRecording(true);
    } catch {
      setNotice("无法访问麦克风（已授权？）；可直接输入转写文本评分。");
    }
  };

  const stopRecording = () => {
    recorder.current?.stop();
    setRecording(false);
  };

  const evaluate = async () => {
    if (!target) return;
    const targetMs = estimateSpeechMs(target.text, target.language);
    try {
      const result = await invoke<SpeakingScore>("language_speaking_feedback", {
        request: {
          target: target.text,
          transcript: transcript.trim() || target.text,
          durationMs: recordedMs || targetMs,
          targetMs,
          longPausesMs: [],
        },
      });
      setScore(result);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  };

  if (!target) {
    return <div className="lang-panel">
      <section className="lang-card">
        <h3>口语 <small>Speaking</small></h3>
        <p className="lang-muted">该语言暂无例句（请先安装 Starter Pack 或导入 Tatoeba）。</p>
      </section>
    </div>;
  }

  return <div className="lang-panel lang-speak">
    <section className="lang-card">
      <header className="lang-card-head">
        <h3>口语练习 <small>Speaking · Tatoeba #{target.sentence_id}</small></h3>
        <button className="lang-link" onClick={() => void pickTarget()}>换一句</button>
      </header>

      <p className="lang-review-word">{target.text}</p>
      <small className="lang-license">{target.license}{target.author ? ` · ${target.author}` : ""}</small>

      <div className="lang-row-actions">
        <button className="lang-primary-icon" onClick={() => void speak(target.text, target.language)} title="播放目标句（TTS）">
          <Play size={16} /> 播放
        </button>
        {recording
          ? <button className="lang-danger" onClick={stopRecording}><Stop size={16} /> 停止</button>
          : <button className="lang-primary" onClick={() => void startRecording()}><Microphone size={16} /> 录音</button>}
      </div>

      <label className="lang-field">
        <span>转写文本（V1：手动输入，或将来接入本地/云端 STT Provider）</span>
        <textarea
          value={transcript}
          onChange={(event) => setTranscript(event.target.value)}
          rows={2}
          placeholder="把你说的内容打在这里，例如：I would like to make a reservation"
        />
      </label>
      {recordedMs > 0 ? <small className="lang-muted">录音时长 {Math.round(recordedMs / 1000)}s</small> : null}

      <button className="lang-primary" onClick={() => void evaluate()}>评分</button>

      {score ? <section className="lang-score">
        <h4>反馈</h4>
        <ul>
          <li>准确性 Accuracy <b>{score.accuracy}</b></li>
          <li>完整性 Completeness <b>{score.completeness}</b></li>
          <li>流利度 Fluency <b>{score.fluency}</b></li>
        </ul>
        <p className="lang-muted">基于 Missing / Wrong / Extra Word 与录音时长；不做口音评分（V1 范围）。</p>
      </section> : null}
    </section>
  </div>;
}