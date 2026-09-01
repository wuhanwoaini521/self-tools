import { ArrowRight, Clock, MapPin } from "@phosphor-icons/react";
import { useState } from "react";
import { historyStories } from "../data/historyStories";
import { historyEras } from "../historyEras";

/**
 * Story View（故事 / 学习路线）：不是随机百科，
 * 而是围绕「为什么」的叙事主线；节点对应真实资料库条目时可点击深入。
 */
export function StoryView({
  onOpenNodeId,
  onLocateEra,
}: {
  onOpenNodeId: (nodeId: string, title: string) => void;
  onLocateEra: (periodId: string) => void;
}) {
  const [activeId, setActiveId] = useState<string | null>(
    historyStories[0]?.id ?? null,
  );
  const story = historyStories.find((item) => item.id === activeId) ?? null;
  const era = story
    ? historyEras.find((item) => item.id === story.periodId) ?? null
    : null;

  return (
    <section className="history-story-view">
      <header className="history-section-head">
        <div>
          <span>STORY</span>
          <h2>历史故事 · 学习路线</h2>
        </div>
        <p>从「为什么」出发，沿真实史料主线逐步深入</p>
      </header>

      <div className="history-story-list">
        {historyStories.map((item) => (
          <button
            type="button"
            key={item.id}
            className={item.id === activeId ? "is-active" : ""}
            onClick={() => setActiveId(item.id)}
          >
            <h3>{item.title}</h3>
            <p>{item.description}</p>
            <span>
              <Clock size={13} />约 {item.minutes} 分钟 ·{" "}
              {historyEras.find((e) => e.id === item.periodId)?.name ?? "历史"}
            </span>
          </button>
        ))}
      </div>

      {story && era ? (
        <div className="history-story-detail">
          <header>
            <div>
              <span>STORYLINE</span>
              <h3>{story.title}</h3>
            </div>
            <button
              type="button"
              className="history-story-locate"
              onClick={() => onLocateEra(story.periodId)}
            >
              <MapPin size={14} /> 回到{era.name}时代
            </button>
          </header>
          <p className="history-story-description">{story.description}</p>
          <ol className="history-story-path">
            {story.nodes.map((node, index) => {
              const clickable = Boolean(node.nodeId);
              return (
                <li key={node.id} className={clickable ? "is-clickable" : ""}>
                  <i aria-hidden="true">{String(index + 1).padStart(2, "0")}</i>
                  <div>
                    <b>{node.title}</b>
                    <p>{node.note}</p>
                  </div>
                  {clickable && node.nodeId ? (
                    <button
                      type="button"
                      onClick={() => onOpenNodeId(node.nodeId!, node.title)}
                    >
                      进入详情
                    </button>
                  ) : (
                    <span className="history-story-pending">资料整理中</span>
                  )}
                </li>
              );
            })}
          </ol>
          <footer className="history-story-end">
            <ArrowRight size={16} />
            路线结束 · 可以回到时间轴继续纵向探索
          </footer>
        </div>
      ) : (
        <p className="history-view-empty">暂无故事数据。</p>
      )}
    </section>
  );
}