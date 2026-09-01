import { invoke } from "@tauri-apps/api/core";
import { Compass } from "@phosphor-icons/react";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { HistoryHome, HistoryNode } from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import {
  clampEraIndex,
  DEFAULT_ERA_INDEX,
  historyEras,
} from "./historyEras";
import { historyStories } from "./data/historyStories";
import { useHistoryNavigation } from "./hooks/useHistoryNavigation";
import { useHistorySearch } from "./hooks/useHistorySearch";
import { HistoryHeader } from "./components/HistoryHeader";
import { HistoryViewSwitcher } from "./components/HistoryViewSwitcher";
import { HistorySearch } from "./components/HistorySearch";
import { DetailRouter } from "./details/DetailRouter";
import { TimelineView } from "./views/TimelineView";
import { MapView } from "./views/MapView";
import { PeopleView } from "./views/PeopleView";
import { GraphView } from "./views/GraphView";
import { StoryView } from "./views/StoryView";
import type {
  HistoryGeoNavigationRequest,
  HistoryViewMode,
} from "./types/history";

interface HistoryPageProps {
  active: boolean;
  setNotice: (message: string) => void;
  intent?: { id: string; nonce: number } | null;
  onNavigateToGeography?: (request: HistoryGeoNavigationRequest) => void;
}

/** 今日故事：按天轮换（数据稳定，不随机）。 */
function todayStoryId(): string {
  const day = Math.floor(Date.now() / 86_400_000);
  const stories = historyStories;
  if (!stories.length) return "";
  return stories[day % stories.length].id;
}

export function HistoryPage({
  active,
  setNotice,
  intent,
  onNavigateToGeography,
}: HistoryPageProps) {
  const [home, setHome] = useState<HistoryHome | null>(null);
  const [view, setView] = useState<HistoryViewMode>("timeline");
  const [currentEraIndex, setCurrentEraIndex] = useState(DEFAULT_ERA_INDEX);

  const currentEra = historyEras[currentEraIndex] ?? historyEras[0];

  const todayStory = useMemo(
    () => historyStories.find((story) => story.id === todayStoryId()) ?? null,
    [],
  );

  const loadHome = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      setHome(await invoke<HistoryHome>("history_home", { cursor: 0 }));
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [setNotice]);

  useEffect(() => {
    if (active && !home) void loadHome();
  }, [active, home, loadHome]);

  const navigation = useHistoryNavigation({
    favoriteIds: home?.favorite_ids ?? [],
    setNotice,
    onFavoritesChange: useCallback(
      (next: string[]) =>
        setHome((current) =>
          current ? { ...current, favorite_ids: next } : current,
        ),
      [],
    ),
    onNavigateToGeography,
  });

  const { detail, periodNodes, periodLoading, openDetail, openEraDetail } =
    navigation;

  const search = useHistorySearch({ setNotice });

  const handleOpenNode = useCallback(
    (node: HistoryNode) => {
      void openDetail(node.id);
      search.clear();
    },
    [openDetail, search.clear],
  );

  const handleOpenNodeId = useCallback(
    (nodeId: string) => {
      void openDetail(nodeId);
      search.clear();
    },
    [openDetail, search.clear],
  );

  const locateEra = useCallback((periodId: string) => {
    const index = historyEras.findIndex((era) => era.id === periodId);
    if (index >= 0) {
      setCurrentEraIndex(index);
      setView("timeline");
    }
  }, []);

  const handleSelectEraIndex = useCallback((index: number) => {
    setCurrentEraIndex(clampEraIndex(index));
  }, []);

  const handleLocateEraByIndex = useCallback((index: number) => {
    setCurrentEraIndex(clampEraIndex(index));
    setView("timeline");
  }, []);

  // intent：从 Home / 其他模块跳转到指定资料。
  useEffect(() => {
    if (active && intent?.id) void openDetail(intent.id);
  }, [active, intent, openDetail]);

  // 时代切换 → 预取该时代的节点（认识这个时代 / 人物 / 关系共用）。
  useEffect(() => {
    void navigation.loadPeriodNodes(currentEra.id);
  }, [currentEra.id, navigation.loadPeriodNodes]);

  if (!isTauriRuntime()) {
    return (
      <div className="history-page history-empty">
        <Compass size={28} />
        <h1>History Explorer</h1>
        <p>历史探索需要通过桌面应用启动，以访问内置的离线历史资料库。</p>
      </div>
    );
  }

  if (detail) {
    return (
      <div className="history-page">
        <div className="history-masthead-row">
          <HistoryHeader />
          <HistorySearch
            search={search}
            onOpenNode={handleOpenNode}
            onLocateEra={handleLocateEraByIndex}
          />
        </div>
        <DetailRouter
          view={detail}
          favorite={Boolean(home?.favorite_ids.includes(detail.document.node.id))}
          onBack={navigation.closeDetail}
          onToggleFavorite={() => void navigation.toggleFavorite()}
          onOpenNode={handleOpenNode}
          onOpenEra={(era) => void openEraDetail(era)}
          onNavigateToGeography={onNavigateToGeography}
        />
      </div>
    );
  }

  const searching = search.query.trim() !== "";

  return (
    <div className="history-page history-explorer-page">
      <div className="history-masthead-row">
        <HistoryHeader />
        <HistorySearch
          search={search}
          onOpenNode={handleOpenNode}
          onLocateEra={handleLocateEraByIndex}
        />
      </div>

      <HistoryViewSwitcher view={view} onChange={setView} />

      {searching ? (
        <SearchStatus search={search} />
      ) : (
        <ViewContent
          view={view}
          currentEra={currentEra}
          currentEraIndex={currentEraIndex}
          onSelectEra={handleSelectEraIndex}
          persons={periodNodes}
          periodLoading={periodLoading}
          todayStory={todayStory}
          onOpenEra={(era) => void openEraDetail(era)}
          onOpenNode={handleOpenNode}
          onOpenNodeId={handleOpenNodeId}
          onStoryInfo={setNotice}
          onStartStory={() => setView("story")}
          onSwitchView={setView}
          onLocateEra={locateEra}
        />
      )}

      {!searching ? (
        <p className="history-chronology-hint">
          浏览中国五千年历史 · 选择时代后从人物、事件、地点与制度继续展开
        </p>
      ) : null}
    </div>
  );
}

/** 按顶层 View 分发内容。 */
function ViewContent({
  view,
  currentEra,
  currentEraIndex,
  onSelectEra,
  persons,
  periodLoading,
  todayStory,
  onOpenEra,
  onOpenNode,
  onOpenNodeId,
  onStoryInfo,
  onStartStory,
  onSwitchView,
  onLocateEra,
}: {
  view: HistoryViewMode;
  currentEra: (typeof historyEras)[number];
  currentEraIndex: number;
  onSelectEra: (index: number) => void;
  persons: HistoryNode[];
  periodLoading: boolean;
  todayStory: (typeof historyStories)[number] | null;
  onOpenEra: (era: (typeof historyEras)[number]) => void;
  onOpenNode: (node: HistoryNode) => void;
  onOpenNodeId: (nodeId: string, title: string) => void;
  onStoryInfo: (message: string) => void;
  onStartStory: () => void;
  onSwitchView: (view: HistoryViewMode) => void;
  onLocateEra: (periodId: string) => void;
}) {
  switch (view) {
    case "timeline":
      return (
        <TimelineView
          era={currentEra}
          currentIndex={currentEraIndex}
          onSelectEra={onSelectEra}
          persons={persons}
          periodLoading={periodLoading}
          onOpenEra={onOpenEra}
          onOpenNode={onOpenNode}
          onOpenNodeId={onOpenNodeId}
          onStoryInfo={onStoryInfo}
          todayStory={todayStory}
          onStartStory={onStartStory}
        />
      );
    case "map":
      return <MapView />;
    case "people":
      return (
        <PeopleView
          era={currentEra}
          persons={persons}
          onOpenNode={onOpenNode}
          onSwitchView={onSwitchView}
        />
      );
    case "graph":
      return (
        <GraphView
          era={currentEra}
          nodes={persons}
          onOpenNode={onOpenNode}
        />
      );
    case "story":
      return (
        <StoryView
          onOpenNodeId={onOpenNodeId}
          onLocateEra={onLocateEra}
        />
      );
  }
}

function SearchStatus({
  search,
}: {
  search: ReturnType<typeof useHistorySearch>;
}) {
  const hasQuery = search.query.trim() !== "";
  if (!hasQuery) return null;
  if (search.searching) {
    return <p className="history-search-status">正在搜索「{search.query}」…</p>;
  }
  if (search.hasResults) {
    const total = search.groups.reduce((sum, group) => sum + group.items.length, 0);
    return (
      <p className="history-search-status">
        找到 {total} 条结果 · ↑↓ 选择，Enter 打开，Esc 清除
      </p>
    );
  }
  if (!search.submitted) {
    return (
      <p className="history-search-status">按回车开始搜索「{search.query}」</p>
    );
  }
  return <p className="history-search-status">没有匹配资料，换个关键词试试。</p>;
}