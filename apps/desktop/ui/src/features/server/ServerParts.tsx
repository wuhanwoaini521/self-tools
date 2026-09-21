/**
 * Home Server Dashboard 组件（V7 §56/§78-§82）。
 *
 * 确认卡必须显示**目标 / 影响 / 风险 / 过期时间**（§56：不能只显示
 * "Confirm?"）；移动端全宽（§82）。
 */
import { useEffect, useState } from "react";
import { CaretRight, FileText, Plug, ShieldWarning } from "@phosphor-icons/react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  HEALTH_LABELS,
  OUTCOME_LABELS,
  type ActionRisk,
  type AppListItemDto,
  type AuditEntryDto,
  type ConfirmationDto,
  type HealthStatus,
  type ServiceListItemDto,
} from "./serverTypes";

const RISK_LABELS: Record<ActionRisk, string> = {
  read: "只读",
  safe_write: "安全写入",
  sensitive_write: "敏感写入",
  system: "系统修改",
};

export const ACTION_RISK_LABELS = RISK_LABELS;

export function HealthBadge({ status }: { status: HealthStatus }) {
  return (
    <span className={`server-badge health ${status}`}>
      {HEALTH_LABELS[status]}
    </span>
  );
}

export function MetricTile({
  icon,
  label,
  value,
  hint,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  hint?: string;
}) {
  return (
    <div className="server-tile">
      <span className="server-tile-label">
        {icon}
        {label}
      </span>
      <strong className="server-tile-value">{value}</strong>
      {hint ? <span className="server-tile-hint">{hint}</span> : null}
    </div>
  );
}

export function ServiceRow({
  service,
  pending,
  onRestart,
  onLogs,
}: {
  service: ServiceListItemDto;
  pending: boolean;
  onRestart: () => void;
  onLogs: () => void;
}) {
  const canRestart = service.allowed_actions.includes("restart");
  return (
    <li className="server-row">
      <div className="server-row-main">
        <span className="server-row-title">
          {service.display_name}
          <HealthBadge status={service.status} />
        </span>
        <span className="server-row-meta">{service.detail}</span>
      </div>
      <div className="server-row-actions">
        <button onClick={onLogs} title="查看有界日志">
          <FileText size={14} />
          日志
        </button>
        <button
          className="danger"
          disabled={pending || !canRestart}
          onClick={onRestart}
          title={
            canRestart
              ? "请求重启（需要你确认）"
              : "该服务未声明允许 restart"
          }
        >
          <Plug size={14} />
          重启
        </button>
      </div>
    </li>
  );
}

export function AppRow({
  app,
  onOpen,
}: {
  app: AppListItemDto;
  onOpen: () => void;
}) {
  return (
    <li className="server-row">
      <div className="server-row-main">
        <span className="server-row-title">
          {app.name}
          <HealthBadge status={app.status} />
        </span>
        <span className="server-row-meta">{app.url}</span>
      </div>
      <div className="server-row-actions">
        <button onClick={onOpen} title="用系统默认浏览器打开">
          <CaretRight size={14} />
          打开
        </button>
      </div>
    </li>
  );
}

export function AuditRow({ entry }: { entry: AuditEntryDto }) {
  return (
    <li className="server-audit-row">
      <span className={`server-audit-result ${entry.result}`}>
        {OUTCOME_LABELS[entry.result]}
      </span>
      <span className="server-audit-action">
        {entry.action_type} · {entry.target_id}
      </span>
      <span className="server-audit-meta">
        {RISK_LABELS[entry.risk]} · {entry.confirmed ? "已确认" : "未确认"} ·{" "}
        {entry.duration_ms}ms{entry.error_code ? ` · ${entry.error_code}` : ""}
      </span>
    </li>
  );
}

/**
 * SYSTEM 操作确认卡（§56/§82）。
 *
 * 显示目标 / 影响 / 风险 / 过期倒计时；**没有**「一键直接执行」路径。
 */
export function ConfirmActionCard({
  confirmation,
  onConfirm,
  onCancel,
}: {
  confirmation: ConfirmationDto;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  // 过期倒计时（Unix 秒）；过期后按钮禁用，必须重新请求确认（§59）。
  const secondsLeft = () =>
    Math.max(0, confirmation.expires_at - Math.floor(Date.now() / 1000));
  const [remaining, setRemaining] = useState(secondsLeft);

  useEffect(() => {
    const timer = window.setInterval(() => {
      setRemaining(secondsLeft());
    }, 1_000);
    return () => window.clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [confirmation.expires_at]);

  return (
    <section className="server-confirm" role="dialog" aria-label="确认系统操作">
      <header>
        <ShieldWarning size={16} />
        AI 请求执行系统操作
      </header>
      <p className="server-confirm-summary">{confirmation.summary}</p>
      <dl className="server-confirm-meta">
        <div>
          <dt>目标</dt>
          <dd>
            {confirmation.action_type} · {confirmation.target_id}
          </dd>
        </div>
        <div>
          <dt>风险</dt>
          <dd>
            <span className="server-badge risk system">
              {RISK_LABELS[confirmation.risk]}
            </span>
          </dd>
        </div>
        <div>
          <dt>影响</dt>
          <dd>服务将短暂停止后重新启动</dd>
        </div>
        <div>
          <dt>有效期</dt>
          <dd>{remaining > 0 ? `${remaining} 秒后失效` : "已过期"}</dd>
        </div>
      </dl>
      <div className="server-confirm-actions">
        <button className="danger" disabled={remaining <= 0} onClick={onConfirm}>
          确认执行
        </button>
        <button onClick={onCancel}>取消</button>
      </div>
    </section>
  );
}

export { openUrl };
