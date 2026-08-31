import { Check, Circle, CircleNotch, Clock, WarningCircle } from "@phosphor-icons/react";
import type { CSSProperties } from "react";
import { useLayoutEffect, useRef, useState } from "react";
import type { ResearchPhase, StepStatus, TravelResearchEvent } from "../../types";

/** 研究进度：左侧阶段导航 + 右侧当前任务工作区。 */

const PHASE_ORDER: { phase: ResearchPhase; label: string; description: string }[] = [
  { phase: "identify_city", label: "识别城市", description: "确认目的地与研究范围" },
  { phase: "plan_queries", label: "规划搜索任务", description: "拆分景点、交通、美食等搜索主题" },
  { phase: "search", label: "搜索信息", description: "从已配置的搜索服务收集候选来源" },
  { phase: "fetch_documents", label: "抓取网页", description: "读取来源正文并保留可追溯链接" },
  { phase: "extract_facts", label: "提取事实", description: "从资料中提炼可用于攻略的事实" },
  { phase: "data_sources", label: "结构化数据源", description: "补充地图、天气等结构化信息" },
  { phase: "rank_sources", label: "来源排序", description: "按权威度与相关性筛选来源" },
  { phase: "validate_facts", label: "信息验证", description: "合并冲突信息并标记可信度" },
  { phase: "generate_guide", label: "生成攻略", description: "将已验证信息整理成行程建议" },
  { phase: "save_guide", label: "保存攻略", description: "写入本地，方便稍后再次查看" },
];

function phaseStatus(phase: ResearchPhase, events: TravelResearchEvent[]): StepStatus {
  const latest = events.filter((event) => event.phase === phase).reduce<TravelResearchEvent | null>(
    (current, event) => !current || event.seq > current.seq ? event : current,
    null,
  );
  return latest?.status ?? "pending";
}

function StepIcon({ status }: { status: StepStatus }) {
  if (status === "done") return <Check size={16} weight="bold" />;
  if (status === "in_progress") return <CircleNotch size={16} className="spin" weight="bold" />;
  if (status === "failed") return <WarningCircle size={16} weight="fill" />;
  if (status === "skipped") return <Clock size={15} />;
  return <Circle size={15} weight="duotone" />;
}

function statusLabel(status: StepStatus) {
  if (status === "in_progress") return "进行中";
  if (status === "done") return "已完成";
  if (status === "failed") return "需要处理";
  if (status === "skipped") return "已跳过";
  return "未开始";
}

interface TravelProgressProps {
  events: TravelResearchEvent[];
}

export function TravelProgress({ events }: TravelProgressProps) {
  const liveLogRef = useRef<HTMLDivElement>(null);
  const shouldFollowLog = useRef(true);
  const lastPhase = useRef<ResearchPhase | null>(null);
  const [reviewPhase, setReviewPhase] = useState<ResearchPhase | null>(null);
  const steps = PHASE_ORDER.map((item) => ({ ...item, status: phaseStatus(item.phase, events) }));
  const runningIndex = steps.findIndex((step) => step.status === "in_progress");
  const currentIndex = runningIndex >= 0 ? runningIndex : Math.max(steps.findIndex((step) => step.status === "pending"), 0);
  const reviewIndex = reviewPhase === null ? -1 : steps.findIndex((step) => step.phase === reviewPhase);
  const activeIndex = reviewIndex >= 0 ? reviewIndex : currentIndex;
  const activeStep = steps[activeIndex];
  const activeEvents = events.filter((event) => event.phase === activeStep.phase);
  const completedCount = steps.filter((step) => step.status === "done" || step.status === "skipped").length;

  useLayoutEffect(() => {
    const log = liveLogRef.current;
    if (!log) return;
    if (lastPhase.current !== activeStep.phase) {
      lastPhase.current = activeStep.phase;
      shouldFollowLog.current = true;
    }
    if (shouldFollowLog.current) log.scrollTop = log.scrollHeight;
  }, [activeEvents.length, activeStep.phase]);

  return <div className="travel-progress" style={{ "--travel-active-y": `${activeIndex * 52 + 24}px` } as CSSProperties}>
    <ol className="travel-progress-steps" aria-label="攻略研究步骤">
      {steps.map((step, index) => <li key={step.phase} className={`travel-step ${step.status}${index === currentIndex ? " active" : ""}${index === reviewIndex ? " selected" : ""}`}>
        <button onClick={() => setReviewPhase((current) => current === step.phase ? null : step.phase)} aria-current={index === currentIndex ? "step" : undefined} aria-pressed={index === reviewIndex}>
          <span className="travel-step-icon"><StepIcon status={step.status} /></span>
          <span className="travel-step-body"><b>{step.label}</b><small>{index === reviewIndex ? "回看中" : statusLabel(step.status)}</small></span>
        </button>
      </li>)}
    </ol>

    <div className="travel-progress-fog" aria-hidden="true"><span /></div>

    <section className={`travel-progress-workspace ${activeStep.status}`} aria-live="polite">
      <header>
        <div><span className="travel-progress-status"><StepIcon status={activeStep.status} />{statusLabel(activeStep.status)}</span><h2>{activeStep.label}</h2></div>
        <span className="travel-progress-count">{completedCount} / {steps.length}</span>
      </header>
      <p className="travel-progress-description">{activeStep.description}</p>
      <div className="travel-progress-meter" aria-label={`已完成 ${completedCount} / ${steps.length} 个步骤`}><span style={{ width: `${(completedCount / steps.length) * 100}%` }} /></div>
      <div className="travel-progress-live" ref={liveLogRef} onScroll={(event) => {
        const target = event.currentTarget;
        shouldFollowLog.current = target.scrollTop + target.clientHeight >= target.scrollHeight - 24;
      }}>
        {activeEvents.length > 0
          ? <ol>{activeEvents.map((event) => <li className={event.status} key={event.seq}><span>{statusLabel(event.status)}</span><p>{event.message}</p></li>)}</ol>
          : <p className="travel-progress-waiting">正在准备此步骤所需的信息…</p>}
      </div>
      <footer>{reviewPhase === null ? "当前工作区会随研究进度自动切换" : "正在回看此步骤已记录的分析过程；再次点击可回到当前任务"}</footer>
    </section>
  </div>;
}
