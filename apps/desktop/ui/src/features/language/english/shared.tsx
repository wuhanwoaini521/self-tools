/**
 * 英语学习子界面的共享小组件与格式化。
 *
 * 保持克制：只放**多处复用**的东西，避免每个文件各写一份百分比/时长格式化。
 */
import type { LessonStage, LessonStatus } from "../../../types";
import { cx } from "../languageUi";

/** 秒 → 「12 min」/「1h 20m」。 */
export function formatDurationShort(seconds: number): string {
  const value = Math.max(0, Math.round(seconds));
  if (value < 60) return `${value}s`;
  const minutes = Math.floor(value / 60);
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${hours}h` : `${hours}h ${rest}m`;
}

/** 毫秒 → 「00:12.3」（逐句时间轴显示）。 */
export function formatTimestamp(ms: number): string {
  const total = Math.max(0, ms);
  const minutes = Math.floor(total / 60_000);
  const seconds = Math.floor((total % 60_000) / 1000);
  const tenths = Math.floor((total % 1000) / 100);
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}.${tenths}`;
}

export const STAGE_LABELS: Record<LessonStage, string> = {
  vocabulary: "Vocabulary",
  listen: "Listening",
  read: "Reading",
  sentence: "Sentence",
  shadow: "Shadow",
  quiz: "Quiz",
  done: "完成",
};

/** 课时状态点（列表 / Continue 卡片用）。 */
export function StageDots({ status }: { status: LessonStatus }) {
  return (
    <span className={cx("en-lesson-badge", `is-${status}`)} aria-label={statusLabel(status)}>
      {statusLabel(status)}
    </span>
  );
}

export function statusLabel(status: LessonStatus): string {
  switch (status) {
    case "completed":
      return "已完成";
    case "learning":
      return "学习中";
    case "review":
      return "待复习";
    default:
      return "未开始";
  }
}

/** 掌握状态标签（阅读模式单词下划线的可访问名）。 */
export function stateLabel(state: "new" | "learning" | "known"): string {
  switch (state) {
    case "known":
      return "已掌握";
    case "learning":
      return "学习中";
    default:
      return "新词";
  }
}

/** 工作台顶部阶段导航（横向 pill）。 */
export function StageNav({
  stage,
  onChange,
  completed,
}: {
  stage: LessonStage;
  onChange: (stage: LessonStage) => void;
  completed?: Partial<Record<LessonStage, boolean>>;
}) {
  const order: LessonStage[] = ["vocabulary", "listen", "read", "sentence", "shadow", "quiz"];
  return (
    <nav className="en-stage-nav" aria-label="学习阶段">
      {order.map((item) => (
        <button
          key={item}
          type="button"
          className={cx(
            "en-stage-tab",
            stage === item && "is-active",
            completed?.[item] && "is-done",
          )}
          onClick={() => onChange(item)}
          aria-current={stage === item ? "step" : undefined}
        >
          {completed?.[item] ? <span className="en-stage-check">✓</span> : null}
          {STAGE_LABELS[item]}
        </button>
      ))}
    </nav>
  );
}

/** 阶段顺序是否在前（用于「上一步/下一步」）。 */
export function stageOrder(stage: LessonStage): number {
  const index = STAGE_SEQUENCE.indexOf(stage);
  return index === -1 ? 0 : index;
}

/** 完整阶段序列（按学习顺序，含 done）。 */
export const STAGE_SEQUENCE: LessonStage[] = [
  "vocabulary",
  "listen",
  "read",
  "sentence",
  "shadow",
  "quiz",
  "done",
];

export function nextStage(stage: LessonStage): LessonStage | null {
  const index = stageOrder(stage);
  return STAGE_SEQUENCE[index + 1] ?? null;
}

export function prevStage(stage: LessonStage): LessonStage | null {
  const index = stageOrder(stage);
  return index > 0 ? STAGE_SEQUENCE[index - 1] : null;
}
