/**
 * 统一页面骨架（PageShell）。
 *
 * ## 为什么需要它
 *
 * 改造前 14 个页面有 **9 种标题方案**：H1 20px、H2 22px/700、SPAN eyebrow、
 * 5 个页面干脆没有标题。用户切换页面时看到的是"换了一张皮"，
 * 而不是"在同一个应用里换了内容"——这正是「像一堆工具拼在一起」的观感来源。
 *
 * ## 契约
 *
 * 每个页面只需要提供「这是哪一页 + 这一页在做什么」，骨架负责：
 * - 位置感：小写 eyebrow（模块名）+ 大标题（页面内容）+ 一句话说明；
 * - 统一的右侧操作位（`actions`）与下方内容区；
 * - 滚动容器与留白，保证各页内容起始位置一致。
 *
 * 视觉语言沿用既有 token（`--text-*` / `--line-soft` / `--radius-lg`），
 * 不引入新的配色，主题切换自动跟随。
 */
import type { ReactNode } from "react";

export interface PageShellProps {
  /** 模块名，小写 eyebrow，例如 "history" / "travel"。 */
  eyebrow: string;
  /** 页面主标题（一句话，不加句号）。 */
  title: string;
  /** 一句话说明这页做什么；没有就不渲染，避免占位式废话。 */
  description?: string;
  /** 右上角操作区（刷新、筛选、按钮…）。 */
  actions?: ReactNode;
  /** 正文。 */
  children: ReactNode;
  /** 内容区是否自行滚动（默认 true，适合长页面）。 */
  scroll?: boolean;
  /** 附加 className（页面特有的布局修饰）。 */
  className?: string;
}

export function PageShell({
  eyebrow,
  title,
  description,
  actions,
  children,
  scroll = true,
  className,
}: PageShellProps) {
  return (
    <div className={scroll ? "page-scroll page-shell" : "page-shell"}>
      <header className="page-shell-head">
        <div className="page-shell-title">
          <span className="page-shell-eyebrow">{eyebrow}</span>
          <h1>{title}</h1>
          {description ? <p className="page-shell-desc">{description}</p> : null}
        </div>
        {actions ? <div className="page-shell-actions">{actions}</div> : null}
      </header>
      <div className="page-shell-body">{children}</div>
    </div>
  );
}

/** 页面内的次级分组标题（区块而非页面）。 */
export function SectionTitle({
  children,
  hint,
  actions,
}: {
  children: ReactNode;
  hint?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="page-section-head">
      <h2>
        {children}
        {hint ? <small>{hint}</small> : null}
      </h2>
      {actions ? <div className="page-section-actions">{actions}</div> : null}
    </header>
  );
}