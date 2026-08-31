/**
 * TTS 助手（#36/#37）：以 Web Speech API（speechSynthesis）作为 V1 实际 Provider。
 * 离线可用性取决于 OS 语音；不可用时静默降级（返回 false）。
 * 未来可替换为 Local TTS / Cloud TTS Provider（Data Pack / Settings 插槽预留）。
 */

const VOICE_LANG: Record<string, string> = {
  eng: "en-US",
  jpn: "ja-JP",
  cmn: "zh-CN",
  yue: "zh-HK",
};

export function speechSupported(): boolean {
  return typeof window !== "undefined" && "speechSynthesis" in window && "SpeechSynthesisUtterance" in window;
}

/** 播放一段文本；返回是否成功发起。 */
export function speak(text: string, language: string): boolean {
  if (!speechSupported()) return false;
  const utterance = new SpeechSynthesisUtterance(text);
  utterance.lang = VOICE_LANG[language] ?? "en-US";
  utterance.rate = 0.85;
  window.speechSynthesis.cancel();
  window.speechSynthesis.speak(utterance);
  return true;
}

export function stopSpeaking(): void {
  if (speechSupported()) window.speechSynthesis.cancel();
}

/** 估算朗读时长（用于口语 Fluency 参考时长；粗略字符估算）。 */
export function estimateSpeechMs(text: string, language: string): number {
  const cjk = language === "jpn" || language === "cmn" || language === "yue";
  const rate = cjk ? 5.5 : 4.2;
  return Math.round((text.length / rate) * 1000) + 600;
}