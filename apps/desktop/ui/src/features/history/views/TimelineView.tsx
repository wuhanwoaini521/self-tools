import type { HistoryNode } from "../../../types";
import type { HistoryEra } from "../historyEras";
import { HistoryChronologyNavigator } from "../components/HistoryChronologyNavigator";
import { EraHero } from "../components/EraHero";
import { EraTimeline } from "../components/EraTimeline";
import { EraExploreGrid } from "../components/EraExploreGrid";
import { TodayExplore } from "../components/TodayExplore";
import type { HistoryStory } from "../types/history";

/**
 * Timeline View（默认）：顶部分期导航 + Era Hero + 时代事件时间轴 + 认识这个时代。
 * 时代切换时，内容区整体以轻微 fade + translateY 过渡。
 */
export function TimelineView({
  era,
  currentIndex,
  onSelectEra,
  persons,
  periodLoading,
  onOpenEra,
  onOpenNode,
  onOpenNodeId,
  onStoryInfo,
  todayStory,
  onStartStory,
}: {
  era: HistoryEra;
  currentIndex: number;
  onSelectEra: (index: number) => void;
  persons: HistoryNode[];
  periodLoading: boolean;
  onOpenEra: (era: HistoryEra) => void;
  onOpenNode: (node: HistoryNode) => void;
  onOpenNodeId: (nodeId: string, title: string) => void;
  onStoryInfo: (message: string) => void;
  todayStory: HistoryStory | null;
  onStartStory: (story: HistoryStory) => void;
}) {
  return (
    <div className="history-timeline-view">
      <HistoryChronologyNavigator
        currentIndex={currentIndex}
        onSelect={onSelectEra}
      />

      <div className="history-era-stage" key={era.id}>
        <EraHero
          era={era}
          persons={periodLoading ? [] : persons}
          onOpenEra={onOpenEra}
          onOpenNode={onOpenNode}
          onStartStory={todayStory ? () => onStartStory(todayStory) : undefined}
        />

        <EraTimeline era={era} onOpenNodeId={onOpenNodeId} onInfo={onStoryInfo} />

        <EraExploreGrid
          nodes={periodLoading ? [] : persons}
          onOpenNode={onOpenNode}
        />
      </div>

      <TodayExplore story={todayStory} onStart={onStartStory} />
    </div>
  );
}