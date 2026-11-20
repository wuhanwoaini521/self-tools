import { listenOnce, microphoneAvailable, speechCapability, speechErrorText } from "../speech";
import { speechSupported, stopSpeaking } from "../tts";
import type { TtsResult } from "../speakingPracticeTypes";

export interface LocalSpeechEnvironment {
  secureContext: boolean;
  microphone: boolean;
  recognition: boolean;
  synthesis: boolean;
  recording: boolean;
}

export const SPEAKING_RATES = [0.8, 1, 1.15] as const;
export type SpeakingRate = (typeof SPEAKING_RATES)[number];

export function localSpeechEnvironment(): LocalSpeechEnvironment {
  const recognition = speechCapability();
  return {
    secureContext: typeof window === "undefined" ? true : window.isSecureContext,
    microphone: microphoneAvailable(),
    recognition: recognition.supported,
    synthesis: speechSupported(),
    recording:
      typeof window !== "undefined" &&
      typeof MediaRecorder !== "undefined" &&
      microphoneAvailable(),
  };
}

export function localSpeechBanner(
  env: LocalSpeechEnvironment,
  asrConfigured: boolean,
): string | null {
  if (!env.secureContext) {
    return "当前不是安全上下文，浏览器不允许开麦克风。请用 HTTPS 或 localhost，或改用键盘。";
  }
  if (asrConfigured && env.recording) return null;
  if (env.recognition) return null;
  if (!env.microphone) {
    return "没有可用的麦克风。可以继续用键盘完成场景。";
  }
  if (!env.recognition && !asrConfigured) {
    return "浏览器不能语音识别，后端也未配置 ASR。开口按钮不可用，请用键盘输入。";
  }
  return null;
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const data = String(reader.result ?? "");
      const comma = data.indexOf(",");
      resolve(comma >= 0 ? data.slice(comma + 1) : data);
    };
    reader.onerror = () => reject(reader.error ?? new Error("无法读取录音"));
    reader.readAsDataURL(blob);
  });
}

export async function captureBrowserTranscript(): Promise<string> {
  const outcome = await listenOnce();
  if (outcome.error) {
    throw new Error(speechErrorText(outcome.error));
  }
  if (!outcome.transcript) {
    throw new Error("没有识别到内容，靠近麦克风再试，或改用键盘。");
  }
  return outcome.transcript;
}

export interface MicCapture {
  stop: () => Promise<{ audioBase64: string; mimeType: string }>;
}

/** 开口录音。停顿约 1.1 秒后自动结束这一句，不必再按发送。 */
export function startMicRecorder(onSilence?: () => void): MicCapture {
  let stopFn: (() => void) | null = null;
  let requestedStop = false;
  const pending = (async () => {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    const mimeType = MediaRecorder.isTypeSupported("audio/webm") ? "audio/webm" : "";
    const recorder = mimeType
      ? new MediaRecorder(stream, { mimeType })
      : new MediaRecorder(stream);
    const chunks: Blob[] = [];
    recorder.ondataavailable = (event) => {
      if (event.data.size > 0) chunks.push(event.data);
    };
    const finished = new Promise<Blob>((resolve, reject) => {
      recorder.onerror = () => reject(new Error("录音失败"));
      recorder.onstop = () => {
        stream.getTracks().forEach((track) => track.stop());
        resolve(new Blob(chunks, { type: recorder.mimeType || "audio/webm" }));
      };
    });
    const analyser = watchSilence(stream, () => {
      if (recorder.state === "recording") {
        onSilence?.();
        recorder.stop();
      }
    });
    recorder.start();
    stopFn = () => {
      analyser.stop();
      if (recorder.state === "recording") recorder.stop();
    };
    if (requestedStop) stopFn();
    const blob = await finished;
    analyser.stop();
    if (blob.size === 0) throw new Error("没有录到声音");
    return {
      audioBase64: await blobToBase64(blob),
      mimeType: blob.type || "audio/webm",
    };
  })();

  return {
    stop: async () => {
      requestedStop = true;
      stopFn?.();
      return pending;
    },
  };
}

function watchSilence(stream: MediaStream, onSilence: () => void): { stop: () => void } {
  const audioContext = new AudioContext();
  const source = audioContext.createMediaStreamSource(stream);
  const analyser = audioContext.createAnalyser();
  analyser.fftSize = 2048;
  source.connect(analyser);
  const data = new Uint8Array(analyser.fftSize);
  let heard = false;
  let silentSince: number | null = null;
  let timer = 0;
  const tick = () => {
    analyser.getByteTimeDomainData(data);
    let sum = 0;
    for (const sample of data) {
      const centered = (sample - 128) / 128;
      sum += centered * centered;
    }
    const rms = Math.sqrt(sum / data.length);
    const now = performance.now();
    if (rms > 0.04) {
      heard = true;
      silentSince = null;
    } else if (heard) {
      silentSince ??= now;
      if (now - silentSince > 1100) {
        onSilence();
        return;
      }
    }
    timer = window.setTimeout(tick, 80);
  };
  timer = window.setTimeout(tick, 80);
  return {
    stop: () => {
      window.clearTimeout(timer);
      void audioContext.close();
    },
  };
}

let activeAudio: HTMLAudioElement | null = null;

export function stopCoachPlayback(): void {
  if (activeAudio) {
    activeAudio.pause();
    activeAudio.src = "";
    activeAudio = null;
  }
  stopSpeaking();
}

export async function playCoachAudio(result: TtsResult, rate: SpeakingRate = 1): Promise<boolean> {
  stopCoachPlayback();
  if (result.audioAvailable && result.audioBase64) {
    const bytes = Uint8Array.from(atob(result.audioBase64), (char) => char.charCodeAt(0));
    const blob = new Blob([bytes], { type: result.mimeType || "audio/wav" });
    const url = URL.createObjectURL(blob);
    const audio = new Audio(url);
    audio.playbackRate = rate;
    activeAudio = audio;
    audio.onended = () => {
      if (activeAudio === audio) activeAudio = null;
      URL.revokeObjectURL(url);
    };
    try {
      await audio.play();
      return true;
    } catch {
      URL.revokeObjectURL(url);
      activeAudio = null;
      return speakAtRate(result.text, rate);
    }
  }
  return speakAtRate(result.text, rate);
}

function speakAtRate(text: string, rate: SpeakingRate): boolean {
  if (!speechSupported()) return false;
  const utterance = new SpeechSynthesisUtterance(text);
  utterance.lang = "en-US";
  utterance.rate = Math.min(2, Math.max(0.5, 0.85 * rate));
  window.speechSynthesis.cancel();
  window.speechSynthesis.speak(utterance);
  return true;
}

export { stopSpeaking };
