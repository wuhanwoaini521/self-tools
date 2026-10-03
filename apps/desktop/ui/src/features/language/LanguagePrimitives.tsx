/**
 * Language 模块的呈现原语：面板容器、骨架屏、内联错误、空态。
 *
 * 全部复用 `styles.css` 里已有的 `lang-*` 类名（.lang-card / .lang-empty /
 * .lang-muted …），只新增本模块独有的少量类（骨架行、焦点模式等）。
 */
import type { ReactNode } from "react";
import { ArrowClockwise } from "@phosphor-icons/react";
import { cx } from "./languageUi";

/** 一块内容面板。 */
export function Panel({
  title,
  hint,
  actions,
  children,
  className,
}: {
  title: string;
  hint?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cx("lang-card", className)}>
      <header className="lang-card-head">
        <h3>
          {title}
          {hint ? <small>{hint}</small> : null}
        </h3>
        {actions ? <div className="lang-row-actions">{actions}</div> : null}
      </header>
      {children}
    </section>
  );
}

/**
 * 面板内骨架屏。用真实的 DOM 节点而不是一个居中的 spinner——
 * 骨架保留了最终布局的高度，切换时不会跳。
 */
export function Skeleton({ rows = 3 }: { rows?: number }) {
  return (
    <div className="lang-skeleton" role="status" aria-live="polite">
      <span className="lang-skeleton-line" />
      {Array.from({ length: rows }, (_, index) => (
        <span key={index} className="lang-skeleton-line" />
      ))}
      <span className="lang-skeleton-line short" />
    </div>
  );
}

/** 面板级加载态：有旧数据时不骨架化，避免刷新时闪烁。 */
export function PanelBody({
  loading,
  error,
  reload,
  empty,
  skeletonRows = 3,
  children,
}: {
  loading: boolean;
  error: string | null;
  reload: () => void;
  empty: ReactNode | null;
  skeletonRows?: number;
  children: ReactNode;
}) {
  if (error) {
    return (
      <div className="lang-inline-error">
        <p>{error}</p>
        <button type="button" className="lang-link" onClick={reload}>
          <ArrowClockwise size={13} /> 重试
        </button>
      </div>
    );
  }
  if (loading) return <Skeleton rows={skeletonRows} />;
  if (empty) return <p className="lang-empty">{empty}</p>;
  return <>{children}</>;
}

/** 空态：一句话解释「为什么是空的」以及接下来可以做什么。 */
export function EmptyState({ children }: { children: ReactNode }) {
  return <p className="lang-empty">{children}</p>;
}

/** 状态/难度小徽标。 */
export function Chip({
  children,
  tone = "plain",
}: {
  children: ReactNode;
  tone?: "plain" | "accent" | "warn" | "danger";
}) {
  return <span className={cx("lang-chip", `is-${tone}`)}>{children}</span>;
}

/** 0..100 的掌握度条。数值必须来自后端，绝不在这里估算。 */
export function MasteryBar({ score }: { score: number }) {
  const clamped = Math.max(0, Math.min(100, score));
  return (
    <span
      className="lang-mastery"
      role="img"
      aria-label={`掌握度 ${Math.round(clamped)}%`}
    >
      <i style={{ width: `${clamped}%` }} />
      <b>{Math.round(clamped)}%</b>
    </span>
  );
}

/** 进度条（课程步骤 / 段落）。 */
export function ProgressTrack({
  value,
  total,
}: {
  value: number;
  total: number;
}) {
  const safeTotal = Math.max(1, total);
  const clamped = Math.max(0, Math.min(safeTotal, value));
  return (
    <div
      className="lang-track"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={safeTotal}
      aria-valuenow={clamped}
    >
      <i style={{ width: `${(clamped / safeTotal) * 100}%` }} />
    </div>
  );
}

/** 主/次按钮。 */
export function Action({
  children,
  onClick,
  disabled,
  variant = "primary",
  title,
}: {
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  variant?: "primary" | "danger" | "ghost";
  title?: string;
}) {
  return (
    <button
      type="button"
      className={cx(
        variant === "primary" && "lang-primary",
        variant === "danger" && "lang-danger",
        variant === "ghost" && "lang-link",
      )}
      onClick={onClick}
      disabled={disabled ?? false}
      title={title}
    >
      {children}
    </button>
  );
}