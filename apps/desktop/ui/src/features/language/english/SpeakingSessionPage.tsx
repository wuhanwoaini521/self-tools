import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, Microphone, Pause, Play, SpeakerHigh, Stop } from "@phosphor-icons/react";
import { errorMessage } from "../../../utils";
import type {
  SpeakingAudioCapabilities,
  SpeakingCorrection,
  SpeakingFeedback,
  SpeakingScenario,
  SpeakingSessionView,
} from "../speakingPracticeTypes";
import { correctionLabel, nextGoalHint, speakingStatusLabel } from "./speakingConversation";
import { speakingClient } from "./speakingClient";
import {
  SPEAKING_RATES,
  captureBrowserTranscript,
  localSpeechBanner,
  localSpeechEnvironment,
  playCoachAudio,
  startMicRecorder,
  stopCoachPlayback,
  type SpeakingRate,
} from "./speakingAudio";

export interface SpeakingSessionPageProps {
  scenario: SpeakingScenario;
  onBack: () => void;
}

function requestId(prefix: string): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `${prefix}-${crypto.randomUUID()}`;
  }
  return `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function SpeakingSessionPage({ scenario, onBack }: SpeakingSessionPageProps) {
  const [view, setView] = useState<SpeakingSessionView | null>(null);
  const [report, setReport] = useState<SpeakingFeedback | null>(null);
  const [caps, setCaps] = useState<SpeakingAudioCapabilities | null>(null);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [assist, setAssist] = useState(true);
  const [recording, setRecording] = useState(false);
  const [reviewNote, setReviewNote] = useState<string | null>(null);
  const [hintOpen, setHintOpen] = useState(false);
  const [typing, setTyping] = useState(false);
  const [turnNotes, setTurnNotes] = useState<Record<string, SpeakingCorrection[]>>({});
  const [rate, setRate] = useState<SpeakingRate>(1);
  const [autoListen, setAutoListen] = useState(true);
  const spokenTurnRef = useRef<string | null>(null);
  const threadRef = useRef<HTMLOListElement | null>(null);
  const typingDefaulted = useRef(false);
  const recorderRef = useRef<ReturnType<typeof startMicRecorder> | null>(null);
  const finishingRef = useRef(false);
  const env = useMemo(() => localSpeechEnvironment(), []);

  const start = useCallback(async () => {
    setError(null);
    setBusy(true);
    try {
      const [nextCaps, nextView] = await Promise.all([
        speakingClient.capabilities(),
        speakingClient.createSession(scenario.id, requestId("session"), "zh", scenario.cefr),
      ]);
      setCaps(nextCaps);
      setView(nextView);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }, [scenario.cefr, scenario.id]);

  useEffect(() => {
    void start();
    return () => stopCoachPlayback();
  }, [start]);

  const completed = view?.session.status === "completed";

  useEffect(() => {
    if (!completed || !view) return;
    let alive = true;
    speakingClient
      .report(view.session.id)
      .then((value) => {
        if (alive) setReport(value);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [completed, view]);

  useEffect(() => {
    if (!view) return;
    const lastCoach = [...view.turns].reverse().find((turn) => turn.role === "coach");
    if (!lastCoach || spokenTurnRef.current === lastCoach.id) return;
    spokenTurnRef.current = lastCoach.id;
    void speakingClient
      .tts(view.session.id, lastCoach.id)
      .then((audio) => playCoachAudio(audio, rate))
      .catch(() => undefined);
  }, [rate, view]);

  const hint = useMemo(
    () => (view ? nextGoalHint(view.scenario.goals, view.taskProgress.remaining) : null),
    [view],
  );

  useEffect(() => {
    const lastUser = view?.turns.filter((turn) => turn.role === "user").at(-1);
    const notes = view?.coach?.corrections ?? [];
    if (!lastUser || notes.length === 0) return;
    setTurnNotes((previous) =>
      previous[lastUser.id] ? previous : { ...previous, [lastUser.id]: notes },
    );
  }, [view]);

  useEffect(() => {
    threadRef.current?.scrollTo({ top: threadRef.current.scrollHeight });
  }, [view?.turns.length, busy, recording]);

  useEffect(() => {
    if (!caps || typingDefaulted.current) return;
    typingDefaulted.current = true;
    if (!(caps.asrConfigured ? env.recording : env.recognition)) setTyping(true);
  }, [caps, env.recognition, env.recording]);

  const canType = view?.session.status === "listening" || view?.session.status === "failed";
  const audioEnabled = caps?.audioEnabled !== false;
  const canSpeak =
    Boolean(canType && audioEnabled && !busy && !completed) &&
    (caps?.asrConfigured ? env.recording : env.recognition);
  const speechNote = caps ? localSpeechBanner(env, caps.asrConfigured) : null;

  const send = useCallback(async () => {
    if (!view || !draft.trim()) return;
    setBusy(true);
    setError(null);
    try {
      setView(await speakingClient.submitTurn(view.session.id, draft, requestId("turn")));
      setDraft("");
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }, [draft, view]);

  const finishRecording = useCallback(async () => {
    if (!view || !recorderRef.current || finishingRef.current) return;
    finishingRef.current = true;
    setBusy(true);
    setError(null);
    try {
      const clip = await recorderRef.current.stop();
      recorderRef.current = null;
      setRecording(false);
      setView(
        await speakingClient.submitAudio(view.session.id, requestId("audio"), {
          audioBase64: clip.audioBase64,
          mimeType: clip.mimeType,
        }),
      );
    } catch (cause) {
      setError(errorMessage(cause));
      recorderRef.current = null;
      setRecording(false);
    } finally {
      finishingRef.current = false;
      setBusy(false);
    }
  }, [view]);

  const beginListening = useCallback(() => {
    if (!view || !canSpeak || recording || finishingRef.current) return;
    setError(null);
    stopCoachPlayback();
    if (caps?.asrConfigured && env.recording) {
      try {
        recorderRef.current = startMicRecorder(() => {
          void finishRecording();
        });
        setRecording(true);
      } catch (cause) {
        setError(errorMessage(cause));
      }
      return;
    }
    setBusy(true);
    void captureBrowserTranscript()
      .then(async (transcript) => {
        setView(
          await speakingClient.submitAudio(view.session.id, requestId("audio"), { transcript }),
        );
      })
      .catch((cause: unknown) => setError(errorMessage(cause)))
      .finally(() => setBusy(false));
  }, [canSpeak, caps?.asrConfigured, env.recording, finishRecording, recording, view]);

  const speakTurn = useCallback(async () => {
    if (recording) {
      await finishRecording();
      return;
    }
    beginListening();
  }, [beginListening, finishRecording, recording]);

  const pauseOrResume = useCallback(async () => {
    if (!view) return;
    setBusy(true);
    setError(null);
    try {
      setView(
        view.session.status === "paused"
          ? await speakingClient.resume(view.session.id)
          : await speakingClient.pause(view.session.id),
      );
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }, [view]);

  const finish = useCallback(async () => {
    if (!view) return;
    setBusy(true);
    setError(null);
    try {
      setView(await speakingClient.complete(view.session.id));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }, [view]);

  const cutPlayback = useCallback(() => {
    stopCoachPlayback();
    if (autoListen && canSpeak && !recording && !typing) beginListening();
  }, [autoListen, beginListening, canSpeak, recording, typing]);

  const addReview = useCallback(async () => {
    if (!view) return;
    setBusy(true);
    setError(null);
    try {
      const result = await speakingClient.addReview(view.session.id);
      setReviewNote(`已加入复习 ${result.cardsAdded} 张卡`);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }, [view]);

  const replayLastCoach = useCallback(async () => {
    if (!view) return;
    const lastCoach = [...view.turns].reverse().find((turn) => turn.role === "coach");
    if (!lastCoach) return;
    try {
      await playCoachAudio(await speakingClient.tts(view.session.id, lastCoach.id), rate);
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }, [rate, view]);

  if (error && !view) {
    return (
      <div className="en-library">
        <p className="en-inline-error">{error}</p>
        <button type="button" className="en-ghost-btn" onClick={onBack}>
          返回
        </button>
      </div>
    );
  }

  if (!view) {
    return (
      <div className="en-library">
        <p className="en-muted">正在进入场景…</p>
      </div>
    );
  }

  const paused = view.session.status === "paused";
  const statusLabel = speakingStatusLabel(view.session.status, recording, busy);
  const coachName = view.scenario.roleName;

  return (
    <div className="en-library en-speak-app">
      <header className="en-speak-head">
        <button type="button" className="en-icon-btn" onClick={onBack} title="返回">
          <ArrowLeft size={16} />
        </button>
        <div className="en-speak-who">
          <strong>{coachName}</strong>
          <span>{view.scenario.titleZh}</span>
        </div>
        <p className="en-speak-status">{statusLabel}</p>
      </header>

      <ul className="en-speaking-goals" aria-label="这轮要完成的事">
        {view.scenario.goals.map((goal) => (
          <li
            key={goal.id}
            className={view.taskProgress.completed.includes(goal.id) ? "is-done" : undefined}
          >
            {goal.descriptionZh}
          </li>
        ))}
      </ul>

      <ol className="en-speak-thread" ref={threadRef} aria-live="polite">
        {view.turns.map((turn) => {
          const notes = turn.role === "user" ? turnNotes[turn.id] : undefined;
          return (
            <li key={turn.id} className={`en-speak-bubble is-${turn.role}`}>
              <span className="en-speak-name">{turn.role === "coach" ? coachName : "你"}</span>
              <p>{turn.text}</p>
              {assist && turn.assistText ? <p className="en-speak-assist">{turn.assistText}</p> : null}
              {notes?.length ? (
                <ul className="en-speak-fixes">
                  {notes.map((item, index) => (
                    <li key={`${item.span}-${index}`}>
                      <span>{correctionLabel(item.category)}</span>
                      {item.span ? <s>{item.span}</s> : null}
                      <em>{item.suggestion}</em>
                      {item.explanation ? <p className="en-speak-why">{item.explanation}</p> : null}
                    </li>
                  ))}
                </ul>
              ) : null}
            </li>
          );
        })}
        {busy && !recording ? (
          <li className="en-speak-bubble is-coach is-pending">
            <span className="en-speak-name">{coachName}</span>
            <p>……</p>
          </li>
        ) : null}
      </ol>

      {report ? (
        <section className="en-card">
          <h2>本场总结</h2>
          <p>{report.summary}</p>
          <p className="en-muted">
            任务 {report.completedGoalIds.length}/{report.taskTotal || view.scenario.goals.length}
          </p>
          <div className="en-speaking-report-grid">
            <div>
              <h3>语法</h3>
              {report.grammar?.length ? (
                <ul>
                  {report.grammar.map((item, index) => (
                    <li key={`g-${index}`}>
                      {item.span} → {item.suggestion}
                      {item.explanation ? <small> {item.explanation}</small> : null}
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="en-muted">本场没有记下语法纠错。</p>
              )}
            </div>
            <div>
              <h3>不自然</h3>
              {report.unnatural?.length ? (
                <ul>
                  {report.unnatural.map((item, index) => (
                    <li key={`u-${index}`}>
                      {item.span} → {item.suggestion}
                      {item.explanation ? <small> {item.explanation}</small> : null}
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="en-muted">本场没有记下表达建议。</p>
              )}
            </div>
            <div>
              <h3>发音</h3>
              <p className="en-muted">未测评（还没有音素级评分，不会用模型编分数）。</p>
            </div>
          </div>
          {report.drillItems?.length ? (
            <div>
              <h3>复练</h3>
              <ul>
                {report.drillItems.map((item) => (
                  <li key={`${item.kind}-${item.prompt}`}>
                    {item.promptZh ?? item.prompt}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
          {report.dictMatches?.length ? (
            <p className="en-muted">
              词典：
              {report.dictMatches
                .map((item) =>
                  item.inDictionary ? `${item.word}（${item.lemma}）` : `${item.word}（未入词典）`,
                )
                .join(" · ")}
            </p>
          ) : null}
          {report.expressions.length > 0 ? (
            <p className="en-muted">值得记住：{report.expressions.join(" · ")}</p>
          ) : null}
          {report.nextFocus?.length ? (
            <p className="en-muted">下场优先练：{report.nextFocus.join(" · ")}</p>
          ) : null}
          <button type="button" className="en-primary-btn" onClick={() => void addReview()} disabled={busy}>
            加入复习
          </button>
          {reviewNote ? <p className="en-muted">{reviewNote}</p> : null}
        </section>
      ) : null}

      {error ? <p className="en-inline-error">{error}</p> : null}

      {completed ? null : (
        <footer className="en-speak-dock">
          {hintOpen && hint ? (
            <p className="en-speak-hint">
              可以试着：{hint.zh}
              <span>{hint.en}</span>
            </p>
          ) : null}
          {typing ? (
            <textarea
              value={draft}
              rows={2}
              disabled={!canType || busy || recording}
              placeholder={paused ? "已暂停" : "用英语打一句"}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault();
                  void send();
                }
              }}
            />
          ) : null}
          <div className="en-speak-dock-main">
            <button
              type="button"
              className="en-ghost-btn"
              onClick={() => setHintOpen((open) => !open)}
              disabled={!hint}
            >
              {hintOpen ? "收起提示" : "不知道怎么说"}
            </button>
            <button
              type="button"
              className={`en-speak-mic${recording ? " is-live" : ""}`}
              onClick={() => void speakTurn()}
              disabled={!canSpeak && !recording}
              aria-label={recording ? "说完了，现在发送" : "开口说，停顿后会自动发送"}
            >
              {recording ? <Stop size={22} /> : <Microphone size={22} />}
            </button>
            <button
              type="button"
              className="en-primary-btn"
              onClick={() => {
                if (!typing) {
                  setTyping(true);
                  return;
                }
                void send();
              }}
              disabled={typing && (!canType || busy || recording || !draft.trim())}
            >
              {typing ? "发送" : "打字"}
            </button>
          </div>
          {speechNote ? <p className="en-speaking-degrade">{speechNote}</p> : null}
          <div className="en-speak-dock-meta">
            <label className="en-speaking-assist">
              <input
                type="checkbox"
                checked={assist}
                onChange={(event) => setAssist(event.target.checked)}
              />
              字幕
            </label>
            <label className="en-speaking-assist">
              <input
                type="checkbox"
                checked={autoListen}
                onChange={(event) => setAutoListen(event.target.checked)}
              />
              说完自动发
            </label>
            <span className="en-speaking-assist">
              语速
              {SPEAKING_RATES.map((item) => (
                <button
                  key={item}
                  type="button"
                  className={item === rate ? "en-ghost-btn is-on" : "en-ghost-btn"}
                  onClick={() => setRate(item)}
                >
                  {item}x
                </button>
              ))}
            </span>
            <button type="button" className="en-ghost-btn" onClick={() => void replayLastCoach()} disabled={busy}>
              <SpeakerHigh size={14} /> 再听一遍
            </button>
            <button type="button" className="en-ghost-btn" onClick={cutPlayback}>
              打断朗读
            </button>
            <button type="button" className="en-ghost-btn" onClick={() => void pauseOrResume()} disabled={busy}>
              {paused ? <Play size={14} /> : <Pause size={14} />}
              {paused ? "继续" : "暂停"}
            </button>
            <button type="button" className="en-ghost-btn" onClick={() => void finish()} disabled={busy}>
              结束对话
            </button>
          </div>
        </footer>
      )}
    </div>
  );
}
