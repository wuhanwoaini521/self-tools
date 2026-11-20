/** 场景式口语陪练契约，对应 `/api/v1/language/speaking/*`。 */

export type CefrLevel = "A1" | "A2" | "B1" | "B2" | "C1" | "C2";

export type SpeakingSessionStatus =
  | "idle"
  | "listening"
  | "transcribing"
  | "thinking"
  | "speaking"
  | "paused"
  | "interrupted"
  | "failed"
  | "completed";

export interface ScenarioGoal {
  id: string;
  description: string;
  descriptionZh: string;
  keywords: string[];
}

export interface TargetWord {
  word: string;
  glossZh: string;
}

export interface SpeakingScenario {
  id: string;
  slug: string;
  title: string;
  titleZh: string;
  description: string;
  descriptionZh: string;
  roleName: string;
  rolePrompt: string;
  cefr: CefrLevel;
  goals: ScenarioGoal[];
  targetVocab: TargetWord[];
  openingLine: string;
  openingLineZh: string | null;
  closingLine: string;
  closingLineZh: string | null;
  scriptedReplies: unknown[];
}

export interface SpeakingTurn {
  id: string;
  sessionId: string;
  seq: number;
  role: "user" | "coach";
  text: string;
  assistText: string | null;
  source: "typed" | "asr";
  clientRequestId: string | null;
  createdAt: number;
}

export interface SpeakingSession {
  id: string;
  scenarioId: string;
  status: SpeakingSessionStatus;
  cefr: CefrLevel;
  localeAssist: string;
  clientRequestId: string;
  completedGoalIds: string[];
  coachMode: string;
  createdAt: number;
  updatedAt: number;
  endedAt: number | null;
}

export interface CoachPayload {
  replyText: string;
  assistText: string | null;
  corrections: SpeakingCorrection[];
  targetWordsUsed: string[];
  llmUsed: boolean;
}

export interface SpeakingSessionView {
  session: SpeakingSession;
  scenario: SpeakingScenario;
  turns: SpeakingTurn[];
  taskProgress: { completed: string[]; remaining: string[] };
  coach: CoachPayload | null;
}

export interface SpeakingCorrection {
  span: string;
  category: string;
  suggestion: string;
  explanation?: string | null;
}

export interface SpeakingDrillItem {
  kind: string;
  prompt: string;
  promptZh: string | null;
}

export interface SpeakingDictMatch {
  word: string;
  lemma: string;
  gloss: string | null;
  inDictionary: boolean;
}

export interface SpeakingFeedback {
  sessionId: string;
  summary: string;
  expressions: string[];
  errors: SpeakingCorrection[];
  grammar: SpeakingCorrection[];
  unnatural: SpeakingCorrection[];
  pronunciationStatus: string;
  taskTotal: number;
  drillItems: SpeakingDrillItem[];
  dictMatches: SpeakingDictMatch[];
  completedGoalIds: string[];
  nextFocus?: string[];
  createdAt: number;
}

export interface ReviewImportResult {
  cardsAdded: number;
  cardIds: string[];
}

export interface SpeakingAudioCapabilities {
  audioEnabled: boolean;
  asrConfigured: boolean;
  ttsConfigured: boolean;
  asrProvider: string;
  ttsProvider: string;
  realtimeAvailable: boolean;
  bargeInAvailable: boolean;
  phonemeScoring: string;
}

export interface TtsResult {
  audioAvailable: boolean;
  provider: string;
  mimeType: string | null;
  audioBase64: string | null;
  text: string;
  turnId: string | null;
}
