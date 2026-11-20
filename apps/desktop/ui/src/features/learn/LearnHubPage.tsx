import { MapTrifold, Notebook, Scroll, Translate } from "@phosphor-icons/react";
import "../../shell/product-shell.css";

interface LearnHubPageProps {
  active: boolean;
  onOpen: (page: "history" | "geography" | "language" | "study-board") => void;
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

export function LearnHubPage({ active, onOpen }: LearnHubPageProps) {
  return (
    <div className="learn-hub" aria-hidden={!active}>
      <p className="learn-hub-kicker">学习</p>
      <h1>从一门课接着往下</h1>
      <p className="learn-hub-lead">
        历史、地理、语言和画板仍用原来的工作流。这里只是入口；进度与数据还在各模块里。
      </p>
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
