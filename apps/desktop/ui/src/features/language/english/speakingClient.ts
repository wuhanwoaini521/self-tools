import type { CommandTransport } from "../../../transport";
import { defaultTransport } from "../../../transport";
import type {
  ReviewImportResult,
  SpeakingAudioCapabilities,
  SpeakingFeedback,
  SpeakingScenario,
  SpeakingSessionView,
  TtsResult,
} from "../speakingPracticeTypes";

export interface SpeakingClient {
  capabilities(): Promise<SpeakingAudioCapabilities>;
  scenarios(): Promise<SpeakingScenario[]>;
  scenario(scenarioId: string): Promise<SpeakingScenario | null>;
  createSession(
    scenarioId: string,
    clientRequestId: string,
    localeAssist?: string,
    cefr?: string | null,
  ): Promise<SpeakingSessionView>;
  session(sessionId: string): Promise<SpeakingSessionView | null>;
  submitTurn(
    sessionId: string,
    text: string,
    clientRequestId: string,
  ): Promise<SpeakingSessionView>;
  submitAudio(
    sessionId: string,
    clientRequestId: string,
    payload: { transcript?: string; audioBase64?: string; mimeType?: string },
  ): Promise<SpeakingSessionView>;
  tts(sessionId: string, turnId: string): Promise<TtsResult>;
  pause(sessionId: string): Promise<SpeakingSessionView>;
  resume(sessionId: string): Promise<SpeakingSessionView>;
  complete(sessionId: string): Promise<SpeakingSessionView>;
  interrupt(sessionId: string): Promise<SpeakingSessionView>;
  addReview(sessionId: string): Promise<ReviewImportResult>;
  report(sessionId: string): Promise<SpeakingFeedback | null>;
}

function asList(raw: SpeakingScenario[] | { items?: SpeakingScenario[] } | null): SpeakingScenario[] {
  if (!raw) return [];
  return Array.isArray(raw) ? raw : (raw.items ?? []);
}

export function createSpeakingClient(
  transport: CommandTransport = defaultTransport,
): SpeakingClient {
  return {
    capabilities: () =>
      transport.invoke<SpeakingAudioCapabilities>("language_speaking_capabilities"),
    scenarios: async () => {
      const raw = await transport.invoke<SpeakingScenario[] | { items?: SpeakingScenario[] } | null>(
        "language_speaking_scenarios",
      );
      if (raw == null) {
        throw new Error("口语场景列表不可用，请确认本地服务已经包含口语接口。");
      }
      return asList(raw);
    },
    scenario: (scenarioId) =>
      transport.invoke<SpeakingScenario | null>("language_speaking_scenario", { scenarioId }),
    createSession: (scenarioId, clientRequestId, localeAssist = "zh", cefr = null) =>
      transport.invoke<SpeakingSessionView>("language_speaking_create_session", {
        scenarioId,
        clientRequestId,
        localeAssist,
        cefr,
      }),
    session: (sessionId) =>
      transport.invoke<SpeakingSessionView | null>("language_speaking_session", { sessionId }),
    submitTurn: (sessionId, text, clientRequestId) =>
      transport.invoke<SpeakingSessionView>("language_speaking_submit_turn", {
        sessionId,
        text,
        clientRequestId,
      }),
    submitAudio: (sessionId, clientRequestId, payload) =>
      transport.invoke<SpeakingSessionView>("language_speaking_submit_audio", {
        sessionId,
        clientRequestId,
        transcript: payload.transcript ?? null,
        audioBase64: payload.audioBase64 ?? null,
        mimeType: payload.mimeType ?? null,
      }),
    tts: (sessionId, turnId) =>
      transport.invoke<TtsResult>("language_speaking_tts", { sessionId, turnId }),
    pause: (sessionId) =>
      transport.invoke<SpeakingSessionView>("language_speaking_pause", { sessionId }),
    resume: (sessionId) =>
      transport.invoke<SpeakingSessionView>("language_speaking_resume", { sessionId }),
    complete: (sessionId) =>
      transport.invoke<SpeakingSessionView>("language_speaking_complete", { sessionId }),
    interrupt: (sessionId) =>
      transport.invoke<SpeakingSessionView>("language_speaking_interrupt", { sessionId }),
    addReview: (sessionId) =>
      transport.invoke<ReviewImportResult>("language_speaking_add_review", { sessionId }),
    report: (sessionId) =>
      transport.invoke<SpeakingFeedback | null>("language_speaking_report", { sessionId }),
  };
}

export const speakingClient = createSpeakingClient();
