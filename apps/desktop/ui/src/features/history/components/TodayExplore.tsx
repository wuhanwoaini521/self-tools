import { ArrowRight, Clock } from "@phosphor-icons/react";
import type { HistoryStory } from "../types/history";

/**
 * Today Explore（今日探索）：从已收录的学习路线中挑一条，作为轻量入口。
 * 不强占视线；点击进入 Story View。
 */
export function TodayExplore({
  story,
  onStart,
}: {
  story: HistoryStory | null;
  onStart: (story: HistoryStory) => void;
}) {
  if (!story) return null;
  return (
    <aside className="history-today" aria-label="今日探索">
      <span className="history-today-eyebrow">今日探索</span>
      <div className="history-today-body">
        <h3>{story.title}</h3>
        <p>{story.description}</p>
      </div>
      <div className="history-today-actions">
        <span className="history-today-minutes">
          <Clock size={14} weight="duotone" />约 {story.minutes} 分钟
        </span>
        <button type="button" onClick={() => onStart(story)}>
          开始探索 <ArrowRight size={14} />
        </button>
      </div>
    </aside>
  );
}