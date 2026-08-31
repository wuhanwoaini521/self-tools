import { Check, Circle, CircleNotch } from "@phosphor-icons/react";
import type { ResearchPhase, StepStatus, TravelResearchEvent } from "../../types";

/** 需求 #十六：Research Progress —— 步骤化展示研究进度（✓ / ● / ○）。 */

const PHASE_ORDER: { phase: ResearchPhase; label: string }[] = [
  { phase: "identify_city", label: "识别城市" },
  { phase: "plan_queries", label: "规划搜索任务" },
  { phase: "search", label: "搜索信息" },
  { phase: "fetch_documents", label: "抓取网页" },
  { phase: "extract_facts", label: "提取事实" },
  { phase: "data_sources", label: "结构化数据源" },
  { phase: "rank_sources", label: "来源排序" },
  { phase: "validate_facts", label: "信息验证" },
  { phase: "generate_guide", label: "生成攻略" },
  { phase: "save_guide", label: "保存攻略" },
];

/** 每个阶段的最新消息。 */
function phaseMessage(phase: ResearchPhase, events: TravelResearchEvent[]): string {
  const latest = [...events].reverse().find((event) => event.phase === phase);
  return latest ? latest.message : "";
}

function phaseStatus(phase: ResearchPhase, events: TravelResearchEvent[]): StepStatus {
  const ofPhase = events.filter((event) => event.phase === phase);
  if (ofPhase.some((event) => event.status === "done")) return "done";
  if (ofPhase.some((event) => event.status === "in_progress")) return "in_progress";
  if (ofPhase.some((event) => event.status === "failed")) return "failed";
  if (ofPhase.some((event) => event.status === "skipped")) return "skipped";
  return "pending";
}

interface TravelProgressProps {
  events: TravelResearchEvent[];
}

export function TravelProgress({ events }: TravelProgressProps) {
  return <ol className="travel-progress">
    {PHASE_ORDER.map(({ phase, label }) => {
      const status = phaseStatus(phase, events);
      const message = phaseMessage(phase, events);
      return <li key={phase} className={"travel-step " + status}>
        <span className="travel-step-icon">
          {status === "done" ? <Check size={15} weight="bold" />
            : status === "in_progress" ? <CircleNotch size={15} className="spin" />
              : status === "failed" ? <Circle size={15} />
                : <Circle size={15} weight="duotone" />}
        </span>
        <span className="travel-step-body">
          <b>{label}</b>
          {message ? <small>{message}</small> : null}
        </span>
      </li>;
    })}
  </ol>;
}