/**
 * Pencil Sketch Component Library
 *
 * 全站唯一的 UI 原语。页面不得再自行实现卡片 / 按钮 / 输入 / 弹层，
 * 一律引用这里，保证「同一个产品」的一致性。
 *
 * 几何与颜色全部来自 pencil.css 的 `.sketch-*` 类与 token。
 */
import {
  forwardRef,
  useEffect,
  useId,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from "react";
import { SketchIllustration, type SketchIllustrationName } from "./SketchIllustrations";

type Tone = "default" | "blue" | "green" | "orange" | "red" | "purple" | "yellow";

/* ------------------------------------------------------------------ Paper */
export function SketchPaper({
  children,
  className = "",
  tone,
}: {
  children: ReactNode;
  className?: string;
  tone?: Tone;
}) {
  return (
    <div className={`sketch-paper ${tone ? `sketch-paper-${tone}` : ""} ${className}`}>
      {children}
    </div>
  );
}

/* ------------------------------------------------------------------- Card */
export interface SketchCardProps {
  children: ReactNode;
  className?: string;
  interactive?: boolean;
  rotation?: "a" | "b" | "c" | "none";
  onClick?: () => void;
  ariaLabel?: string;
}

export function SketchCard({
  children,
  className = "",
  interactive,
  rotation = "none",
  onClick,
  ariaLabel,
}: SketchCardProps) {
  const rot = rotation !== "none" ? ` sketch-rot-${rotation}` : "";
  const isClickable = interactive || Boolean(onClick);
  const Tag = onClick ? "button" : "div";
  return (
    <Tag
      type={onClick ? "button" : undefined}
      aria-label={ariaLabel}
      onClick={onClick}
      className={`sketch-card${isClickable ? " is-interactive" : ""}${rot} ${className}`}
      style={onClick ? { textAlign: "left", font: "inherit", color: "inherit" } : undefined}
    >
      {children}
    </Tag>
  );
}

export function SketchCardHead({
  title,
  description,
  action,
}: {
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="sketch-card-head">
      <div>
        <div className="sketch-card-title">{title}</div>
        {description ? <p className="sketch-card-desc">{description}</p> : null}
      </div>
      {action}
    </div>
  );
}

/** 面板：用于侧栏分区 / 工具区容器（无 hover 位移）。 */
export function SketchPanel({
  children,
  className = "",
  title,
  action,
}: {
  children: ReactNode;
  className?: string;
  title?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <section className={`sketch-card ${className}`}>
      {title ? (
        <SketchCardHead title={title} action={action} />
      ) : null}
      {children}
    </section>
  );
}

/* ----------------------------------------------------------------- Button */
export interface SketchButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "default" | "primary" | "ghost";
  size?: "md" | "sm";
  icon?: ReactNode;
}

export const SketchButton = forwardRef<HTMLButtonElement, SketchButtonProps>(
  function SketchButton(
    { variant = "default", size = "md", icon, className = "", children, ...rest },
    ref,
  ) {
    return (
      <button
        ref={ref}
        className={`sketch-btn sketch-btn-${variant}${size === "sm" ? " sketch-btn-sm" : ""} ${className}`}
        {...rest}
      >
        {icon}
        {children}
      </button>
    );
  },
);

export const SketchIconButton = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & { label: string; children: ReactNode }
>(function SketchIconButton({ label, children, className = "", ...rest }, ref) {
  return (
    <button
      ref={ref}
      type="button"
      aria-label={label}
      title={label}
      className={`sketch-icon-btn ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
});

/* ------------------------------------------------------------------ Input */
export function SketchField({
  label,
  hint,
  children,
}: {
  label: ReactNode;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <label className="sketch-field">
      <span className="sketch-label">{label}</span>
      {children}
      {hint ? <span className="sketch-hint">{hint}</span> : null}
    </label>
  );
}

export const SketchInput = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  function SketchInput({ className = "", ...rest }, ref) {
    return <input ref={ref} className={`sketch-input ${className}`} {...rest} />;
  },
);

export const SketchTextarea = forwardRef<
  HTMLTextAreaElement,
  TextareaHTMLAttributes<HTMLTextAreaElement>
>(function SketchTextarea({ className = "", ...rest }, ref) {
  return <textarea ref={ref} className={`sketch-textarea ${className}`} {...rest} />;
});

export const SketchSelect = forwardRef<
  HTMLSelectElement,
  SelectHTMLAttributes<HTMLSelectElement>
>(function SketchSelect({ className = "", children, ...rest }, ref) {
  return (
    <select ref={ref} className={`sketch-select ${className}`} {...rest}>
      {children}
    </select>
  );
});

/* ------------------------------------------------------------------- Tabs */
export function SketchTabs<T extends string>({
  tabs,
  value,
  onChange,
  ariaLabel,
}: {
  tabs: Array<{ id: T; label: ReactNode }>;
  value: T;
  onChange: (id: T) => void;
  ariaLabel?: string;
}) {
  return (
    <div className="sketch-tabs" role="tablist" aria-label={ariaLabel}>
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          role="tab"
          aria-selected={tab.id === value}
          className={"sketch-tab" + (tab.id === value ? " active" : "")}
          onClick={() => onChange(tab.id)}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}

/* ------------------------------------------------------------------ Badge */
export function SketchBadge({
  children,
  tone = "default",
  className = "",
}: {
  children: ReactNode;
  tone?: Tone;
  className?: string;
}) {
  return (
    <span className={`sketch-badge${tone !== "default" ? ` sketch-badge-${tone}` : ""} ${className}`}>
      {children}
    </span>
  );
}

/* --------------------------------------------------------------- Progress */
export function SketchProgress({
  value,
  tone = "blue",
  label,
}: {
  value: number;
  tone?: "blue" | "green" | "orange";
  label?: string;
}) {
  const clamped = Math.max(0, Math.min(100, value));
  return (
    <div
      className={`sketch-progress${tone !== "blue" ? ` is-${tone}` : ""}`}
      role="progressbar"
      aria-valuenow={Math.round(clamped)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={label}
    >
      <span style={{ width: `${clamped}%` }} />
    </div>
  );
}

/* ---------------------------------------------------------------- Tooltip */
export function SketchTooltip({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <span className="sketch-tooltip-wrap" aria-describedby={id}>
      {children}
      <span role="tooltip" id={id} className="sketch-tooltip">
        {label}
      </span>
    </span>
  );
}

/* ---------------------------------------------------------------- Popover */
export function SketchPopover({
  trigger,
  children,
  align = "start",
}: {
  trigger: (props: { open: boolean; toggle: () => void }) => ReactNode;
  children: ReactNode;
  align?: "start" | "end";
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLSpanElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onPointer = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <span className="sketch-popover-wrap" ref={rootRef}>
      {trigger({ open, toggle: () => setOpen((v) => !v) })}
      {open ? (
        <div className={`sketch-popover sketch-popover-${align}`} role="dialog">
          {children}
        </div>
      ) : null}
    </span>
  );
}

/* --------------------------------------------------------- Dialog/Drawer */
function useFocusTrap(active: boolean, ref: React.RefObject<HTMLElement | null>) {
  useEffect(() => {
    if (!active || !ref.current) return;
    const root = ref.current;
    const previous = document.activeElement as HTMLElement | null;
    const focusables = () =>
      Array.from(
        root.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((el) => !el.hasAttribute("disabled"));
    focusables()[0]?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Tab") return;
      const items = focusables();
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      previous?.focus?.();
    };
  }, [active, ref]);
}

export function SketchDialog({
  open,
  onClose,
  title,
  children,
  footer,
  labelledBy,
}: {
  open: boolean;
  onClose: () => void;
  title?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  labelledBy?: string;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  useFocusTrap(open, ref);
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);
  if (!open) return null;
  return (
    <div className="sketch-dialog-backdrop" onMouseDown={onClose}>
      <div
        ref={ref}
        className="sketch-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        onMouseDown={(e) => e.stopPropagation()}
      >
        {title ? (
          <header className="sketch-dialog-head">
            <h2 id={labelledBy} className="sketch-card-title">
              {title}
            </h2>
            <SketchIconButton label="关闭" onClick={onClose}>
              ✕
            </SketchIconButton>
          </header>
        ) : null}
        <div className="sketch-dialog-body">{children}</div>
        {footer ? <footer className="sketch-dialog-foot">{footer}</footer> : null}
      </div>
    </div>
  );
}

export function SketchDrawer({
  open,
  onClose,
  title,
  side = "right",
  children,
}: {
  open: boolean;
  onClose: () => void;
  title?: ReactNode;
  side?: "right" | "left" | "bottom";
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  useFocusTrap(open, ref);
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);
  if (!open) return null;
  return (
    <div className="sketch-dialog-backdrop" onMouseDown={onClose}>
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        className={`sketch-drawer sketch-drawer-${side}`}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="sketch-dialog-head">
          {title ? <h2 className="sketch-card-title">{title}</h2> : <span />}
          <SketchIconButton label="关闭" onClick={onClose}>
            ✕
          </SketchIconButton>
        </header>
        <div className="sketch-dialog-body">{children}</div>
      </div>
    </div>
  );
}

/* ----------------------------------------------------------------- Table */
export function SketchTable({
  headers,
  rows,
  caption,
  empty,
}: {
  headers: ReactNode[];
  rows: ReactNode[][];
  caption?: string;
  empty?: ReactNode;
}) {
  if (rows.length === 0 && empty) {
    return <SketchEmptyState title="暂无数据">{empty}</SketchEmptyState>;
  }
  return (
    <div className="sketch-table-wrap">
      <table className="sketch-table">
        {caption ? <caption>{caption}</caption> : null}
        <thead>
          <tr>
            {headers.map((header, index) => (
              <th key={index} scope="col">
                {header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, rowIndex) => (
            <tr key={rowIndex}>
              {row.map((cell, cellIndex) => (
                <td key={cellIndex}>{cell}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/* ------------------------------------------------------------ EmptyState */
export function SketchEmptyState({
  title,
  children,
  illustration,
  action,
}: {
  title: string;
  children?: ReactNode;
  illustration?: SketchIllustrationName;
  action?: ReactNode;
}) {
  return (
    <div className="sketch-empty">
      {illustration ? (
        <SketchIllustration name={illustration} size={56} />
      ) : (
        <SketchIllustration name="search" size={56} />
      )}
      <span className="sketch-empty-title">{title}</span>
      {children ? <p>{children}</p> : null}
      {action}
    </div>
  );
}

/* ------------------------------------------------------- Section / Header */
export function SketchSectionHeader({
  title,
  en,
  hint,
  icon,
  action,
}: {
  title: ReactNode;
  en?: string;
  hint?: ReactNode;
  icon?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <header className="sketch-section-head">
      <h2>
        {icon}
        {title}
        {en ? <span className="sketch-section-en">{en}</span> : null}
        {hint ? <small>{hint}</small> : null}
      </h2>
      {action ? <div>{action}</div> : null}
    </header>
  );
}

export function SketchBreadcrumb({
  items,
  onNavigate,
}: {
  items: Array<{ label: string; onClick?: () => void }>;
  onNavigate?: (index: number) => void;
}) {
  return (
    <nav className="sketch-breadcrumb" aria-label="Breadcrumb">
      {items.map((item, index) => (
        <span key={index}>
          {index > 0 ? <span className="sketch-breadcrumb-sep">/</span> : null}
          {item.onClick ? (
            <button type="button" onClick={item.onClick}>
              {item.label}
            </button>
          ) : (
            <span aria-current="page">{item.label}</span>
          )}
        </span>
      ))}
    </nav>
  );
}

export function SketchPageHeader({
  eyebrow,
  title,
  description,
  actions,
  illustration,
}: {
  eyebrow?: string;
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  illustration?: SketchIllustrationName;
}) {
  return (
    <header className="sketch-page-header">
      <div className="sketch-page-header-copy">
        {eyebrow ? <span className="sketch-page-eyebrow">{eyebrow}</span> : null}
        <h1>{title}</h1>
        {description ? <p>{description}</p> : null}
      </div>
      <div className="sketch-page-header-side">
        {actions ? <div className="sketch-page-header-actions">{actions}</div> : null}
        {illustration ? (
          <SketchIllustration name={illustration} size={64} className="sketch-page-illustration" />
        ) : null}
      </div>
    </header>
  );
}

/* --------------------------------------------------------------- StatCard */
export function SketchStatCard({
  icon,
  value,
  label,
  tone = "default",
}: {
  icon: ReactNode;
  value: ReactNode;
  label: ReactNode;
  tone?: Tone;
}) {
  return (
    <div className={`sketch-stat sketch-stat-${tone}`}>
      <span className="sketch-stat-icon">{icon}</span>
      <span>
        <span className="sketch-stat-value">{value}</span>
        <span className="sketch-stat-label">{label}</span>
      </span>
    </div>
  );
}

/* ----------------------------------------------------------- Sticky/Divider */
export function SketchStickyNote({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return <div className={`sketch-sticky ${className}`}>{children}</div>;
}

export function SketchDivider({ label }: { label?: ReactNode }) {
  if (label) {
    return (
      <div className="sketch-divider-label">
        <span>{label}</span>
      </div>
    );
  }
  return <hr className="sketch-divider" />;
}

/* ---------------------------------------------------------- Module / Nav */
export interface SketchModuleLink {
  label: string;
  icon?: ReactNode;
  onClick: () => void;
}

export function SketchModuleCard({
  title,
  links,
  illustration,
  rotation = "none",
  footer,
}: {
  title: ReactNode;
  links: SketchModuleLink[];
  illustration: SketchIllustrationName;
  rotation?: "a" | "b" | "c" | "none";
  footer?: ReactNode;
}) {
  return (
    <article className={`sketch-module${rotation !== "none" ? ` sketch-rot-${rotation}` : ""}`}>
      <div className="sketch-module-illustration">
        <SketchIllustration name={illustration} size={72} />
      </div>
      <h3 className="sketch-module-title">{title}</h3>
      <div className="sketch-module-links">
        {links.map((link) => (
          <button key={link.label} type="button" className="sketch-module-link" onClick={link.onClick}>
            {link.icon}
            <span>{link.label}</span>
            <span className="sketch-module-arrow" aria-hidden>
              →
            </span>
          </button>
        ))}
      </div>
      {footer ? <div className="sketch-module-foot">{footer}</div> : null}
    </article>
  );
}

export function SketchNavigationItem({
  icon,
  label,
  active,
  badge,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  active?: boolean;
  badge?: ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={"sketch-nav-item" + (active ? " active" : "")}
      aria-current={active ? "page" : undefined}
      onClick={onClick}
    >
      {icon}
      <span>{label}</span>
      {badge ? <span className="sketch-nav-badge">{badge}</span> : null}
    </button>
  );
}
