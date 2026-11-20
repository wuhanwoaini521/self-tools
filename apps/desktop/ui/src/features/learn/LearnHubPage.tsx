import { ArrowRight, Cards, MapTrifold, Notebook, Scroll, Translate } from "@phosphor-icons/react";
import { useCallback, useEffect, useState } from "react";
import { sourceRoute } from "../shared/sourceNavigation";
import { learningClient } from "../learning/learningClient";
import { errorMessage } from "../../utils";
import type { ContinueItem } from "../../types";
import "../../shell/product-shell.css";

interface LearnHubPageProps {
  active: boolean;
  onOpen: (page: "history" | "geography" | "language" | "study-board" | "review") => void;
  onNavigate: (hash: string) => void;
}

const MODULES = [
  {
    id: "history" as const,
    title: "历史",
    blurb: "年表、人物与事件。可从地理地点跳进相关史实。",
    icon: Scroll,
  },
  {
    id: "geography" as const,
    title: "地理",
    blurb: "地球与地貌。地点可加入行程，也可跳到历史。",
    icon: MapTrifold,
  },
  {
    id: "language" as const,
    title: "语言",
    blurb: "课程、复习与继续上次的课文。",
    icon: Translate,
  },
  {
    id: "study-board" as const,
    title: "画板",
    blurb: "草稿与快照保存在学习板里，可随时接着画。",
    icon: Notebook,
  },
];

function continueRoute(item: ContinueItem): string {
  return (
    sourceRoute(item.module, item.entity_type, item.entity_id) ??
    `#${item.module}?id=${encodeURIComponent(item.entity_id)}`
  );
}

export function LearnHubPage({ active, onOpen, onNavigate }: LearnHubPageProps) {
  const [due, setDue] = useState<number | null>(null);
  const [continues, setContinues] = useState<ContinueItem[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const today = await learningClient.getToday();
      setDue(today.pending_reviews_count);
      setContinues(today.continue_items.slice(0, 4));
      setLoadError(null);
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    void load();
  }, [active, load]);

  return (
    <div className="learn-hub" aria-hidden={!active}>
      <p className="learn-hub-kicker">学习</p>
      <h1>从一门课接着往下</h1>
      <p className="learn-hub-lead">
        历史、地理、语言和画板仍用原来的工作流。进度在各模块里，这里只负责入口。
      </p>

      {loadError ? (
        <p className="learn-hub-note">{loadError}</p>
      ) : due && due > 0 ? (
        <button type="button" className="learn-hub-continue" onClick={() => onOpen("review")}>
          <Cards size={18} />
          <span>
            <strong>{due} 张卡片待复习</strong>
            <small>复习中心仍走原来的队列，不改计分规则</small>
          </span>
          <ArrowRight size={16} />
        </button>
      ) : null}

      {continues.length > 0 ? (
        <div className="learn-hub-resume">
          <h2>继续学习</h2>
          <ul>
            {continues.map((item) => (
              <li key={`${item.module}:${item.entity_type}:${item.entity_id}`}>
                <button type="button" onClick={() => onNavigate(continueRoute(item))}>
                  <strong>{item.title}</strong>
                  <span>
                    {item.module}
                    {item.progress_percent == null ? "" : ` · ${Math.round(item.progress_percent)}%`}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <div className="learn-hub-grid">
        {MODULES.map((module) => (
          <button
            key={module.id}
            type="button"
            className="learn-hub-card"
            onClick={() => onOpen(module.id)}
          >
            <module.icon size={22} />
            <strong>{module.title}</strong>
            <span>{module.blurb}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
