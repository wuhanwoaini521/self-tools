import { MapTrifold, Scroll, ShareNetwork, UsersThree, Users } from "@phosphor-icons/react";
import type { HistoryViewMode } from "../types/history";

const VIEWS: { id: HistoryViewMode; label: string; icon: typeof Scroll }[] = [
  { id: "timeline", label: "时间", icon: Scroll },
  { id: "map", label: "地图", icon: MapTrifold },
  { id: "people", label: "人物", icon: Users },
  { id: "graph", label: "关系", icon: ShareNetwork },
  { id: "story", label: "故事", icon: UsersThree },
];

/**
 * History 顶层 View Switcher：时间 / 地图 / 人物 / 关系 / 故事。
 */
export function HistoryViewSwitcher({
  view,
  onChange,
}: {
  view: HistoryViewMode;
  onChange: (view: HistoryViewMode) => void;
}) {
  return (
    <nav className="history-view-switcher" aria-label="历史浏览视角">
      {VIEWS.map((item) => {
        const Icon = item.icon;
        const selected = item.id === view;
        return (
          <button
            key={item.id}
            type="button"
            className={selected ? "is-active" : ""}
            aria-current={selected ? "page" : undefined}
            onClick={() => onChange(item.id)}
          >
            <Icon size={16} weight={selected ? "fill" : "regular"} />
            {item.label}
          </button>
        );
      })}
    </nav>
  );
}