/**
 * 跟读发音评分的前端能力层（V13 W2）。
 *
 * ## 诚实边界（本项目反复强调的那条）
 *
 * 「发音评分」必须建立在**真实的语音识别转写**上：
 * - 浏览器 `SpeechRecognition` 可用 → 录到的话转成文本，交给 Rust 打分；
 * - 不可用（Firefox / 无麦克风 / 断网时 Chrome 也会失败）→ **不给分**，
 *   只保留「录音回放 + 自评」，并如实说明为什么没有分数。
 *
 * 任何「按录音时长估算一个分数」的方案都是编造，直接不做。
 */

/** 识别能力状态（UI 据此决定显示评分按钮还是解释文字）。 */
export type SpeechCapability =
  | { supported: true }
  | { supported: false; reason: string };

interface SpeechRecognitionLike extends EventTarget {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  maxAlternatives: number;
  start(): void;
  stop(): void;
  abort(): void;
  onresult: ((event: never) => void) | null;
  onerror: ((event: { error?: string }) => void) | null;
  onend: (() => void) | null;
  onspeechend: (() => void) | null;
}

type SpeechRecognitionCtor = new () => SpeechRecognitionLike;

function recognitionCtor(): SpeechRecognitionCtor | null {
  if (typeof window === "undefined") return null;
  const scope = window as unknown as {
    SpeechRecognition?: SpeechRecognitionCtor;
    webkitSpeechRecognition?: SpeechRecognitionCtor;
  };
  return scope.SpeechRecognition ?? scope.webkitSpeechRecognition ?? null;
}

/** 麦克风是否可用（仅表示「有设备」，不表示「一定会识别成功」）。 */
export function microphoneAvailable(): boolean {
  return (
    typeof navigator !== "undefined" && Boolean(navigator.mediaDevices?.getUserMedia)
  );
}

/** 本环境能不能做识别评分。 */
export function speechCapability(): SpeechCapability {
  if (!recognitionCtor()) {
    return {
      supported: false,
      reason:
        "当前浏览器不支持语音识别（Chrome / Edge 可用，Firefox 不支持）。可以录音回放对照，但拿不到分数。",
    };
  }
  if (!microphoneAvailable()) {
    return { supported: false, reason: "没有可用的麦克风设备。" };
  }
  return { supported: true };
}

export interface ListenOutcome {
  /** 识别到的文本（可能为空）。 */
  transcript: string;
  /** 用户实际开口时长（毫秒）。 */
  durationMs: number;
  /** 识别过程中的错误码（`not-allowed` = 权限被拒）。 */
  error: string | null;
}

const ERROR_TEXT: Record<string, string> = {
  "not-allowed": "麦克风权限被拒绝，请在浏览器地址栏允许麦克风后重试。",
  "service-not-allowed": "浏览器拒绝了语音识别服务（通常是权限或策略限制）。",
  "no-speech": "没有听到声音，靠近麦克风再试一次。",
  network: "语音识别需要网络（浏览器把音频送到识别服务），当前网络不可用。",
  aborted: "识别被中断。",
  "audio-capture": "找不到麦克风设备。",
};

export function speechErrorText(code: string | null): string {
  if (!code) return "语音识别失败。";
  return ERROR_TEXT[code] ?? `语音识别失败（${code}）。`;
}

/**
 * 录一次并等识别结果。
 *
 * 流程：启动识别 → 用户朗读 → `stop()` → 汇总最终结果。
 * 识别服务不可用时**照样 resolve**（`transcript` 为空 + `error`），
 * 让调用方决定「显示无法评分」而不是抛异常把整页打断。
 */
export function listenOnce(timeoutMs = 15_000): Promise<ListenOutcome> {
  const Ctor = recognitionCtor();
  if (!Ctor) {
    return Promise.resolve({
      transcript: "",
      durationMs: 0,
      error: "unsupported",
    });
  }
  return new Promise<ListenOutcome>((resolve) => {
    const recognition = new Ctor();
    recognition.lang = "en-US";
    recognition.continuous = false;
    recognition.interimResults = false;
    recognition.maxAlternatives = 1;
    const startedAt = Date.now();
    let transcript = "";
    let error: string | null = null;
    let settled = false;

    const finish = () => {
      if (settled) return;
      settled = true;
      window.clearTimeout(timer);
      resolve({ transcript: transcript.trim(), durationMs: Date.now() - startedAt, error });
    };

    const timer = window.setTimeout(() => {
      // 超时：主动收尾，拿到多少算多少（空就返回空，让上层说「无法评分」）。
      try {
        recognition.stop();
      } catch {
        // 已经停了。
      }
      finish();
    }, timeoutMs);

    recognition.onresult = ((event: {
      results: ArrayLike<{ 0: { transcript: string }; isFinal: boolean }>;
      resultIndex: number;
    }) => {
      for (let index = event.resultIndex; index < event.results.length; index += 1) {
        const result = event.results[index];
        if (result?.isFinal) transcript += result[0].transcript;
      }
    }) as SpeechRecognitionLike["onresult"];

    recognition.onerror = ((event: { error?: string }) => {
      error = event.error ?? "unknown";
    }) as SpeechRecognitionLike["onerror"];

    recognition.onend = () => finish();
    recognition.onspeechend = () => {
      try {
        recognition.stop();
      } catch {
        finish();
      }
    };

    try {
      recognition.start();
    } catch {
      // 已经在监听中 / 被策略拒绝。
      error = "start-failed";
      finish();
    }
  });
}

/** 目标句的参考朗读时长（毫秒）：母语者约 150 wpm + 呼吸。 */
export function referenceDurationMs(text: string): number {
  const words = text.trim().split(/\s+/).filter(Boolean).length;
  return Math.round((words / 150) * 60_000) + 400;
}
