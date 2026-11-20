import { describe, expect, it } from "vitest";
import type { ScenarioGoal } from "../speakingPracticeTypes";
import { nextGoalHint, speakingStatusLabel } from "./speakingConversation";

const goals: ScenarioGoal[] = [
  {
    id: "greet",
    description: "Greet the barista",
    descriptionZh: "向店员问好",
    keywords: ["hello", "hi"],
  },
  {
    id: "order",
    description: "Order a drink",
    descriptionZh: "点一杯饮料",
    keywords: ["latte"],
  },
];

describe("speakingStatusLabel", () => {
  it("puts listening and thinking ahead of the saved status", () => {
    expect(speakingStatusLabel("listening", true, false)).toBe("正在听你说");
    expect(speakingStatusLabel("listening", false, true)).toBe("外教在想");
    expect(speakingStatusLabel("listening", false, false)).toBe("轮到你说");
    expect(speakingStatusLabel("completed", false, true)).toBe("对话结束");
  });
});

describe("nextGoalHint", () => {
  it("shows the next task without its completion keywords", () => {
    const hint = nextGoalHint(goals, ["order"]);
    expect(hint).toEqual({ en: "Order a drink", zh: "点一杯饮料" });
    expect(JSON.stringify(hint)).not.toContain("latte");
    expect(nextGoalHint(goals, [])).toBeNull();
  });
});
