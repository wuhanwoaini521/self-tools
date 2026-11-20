import type { ScenarioGoal, SpeakingSessionStatus } from "../speakingPracticeTypes";

/** 对话页顶部状态。录音和等待外教优先于会话状态。 */
export function speakingStatusLabel(
  status: SpeakingSessionStatus | null,
  recording: boolean,
  busy: boolean,
): string {
  if (recording) return "正在听你说";
  if (busy && status !== "completed") return "外教在想";
  switch (status) {
    case "listening":
      return "轮到你说";
    case "thinking":
      return "外教在想";
    case "speaking":
      return "外教在说";
    case "transcribing":
      return "正在识别";
    case "paused":
      return "已暂停";
    case "interrupted":
      return "朗读已停下";
    case "failed":
      return "这句没发出去，可以再试";
    case "completed":
      return "对话结束";
    default:
      return "准备开始";
  }
}

/** 下一句提示只用任务说明，不把完成关键词摊开。 */
export function nextGoalHint(
  goals: ScenarioGoal[],
  remaining: string[],
): { en: string; zh: string } | null {
  const id = remaining[0];
  if (!id) return null;
  const goal = goals.find((item) => item.id === id);
  if (!goal) return null;
  return { en: goal.description, zh: goal.descriptionZh };
}

export function correctionLabel(category: string): string {
  if (category === "grammar") return "语法";
  if (category === "unnatural") return "更自然";
  return "说法";
}
