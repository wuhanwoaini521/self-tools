/**
 * Home Server Dashboard（V7 §77-§82）。
 *
 * 「一眼看懂」而不是 Grafana clone（§79）：Overall / CPU / Memory / Storage +
 * Services + Applications + Recent Actions。敏感操作必须走确认卡
 * （§82：不能单击一个 tiny restart icon 直接执行）。
 */
import { useCallback, useEffect, useState } from "react";
import { ArrowClockwise, Cpu, HardDrives, Memory as MemoryIcon, Warning } from "@phosphor-icons/react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { errorMessage, isTauriRuntime } from "../../utils";
import {
  AppRow,
  AuditRow,
  ConfirmActionCard,
  HealthBadge,
  MetricTile,
  ServiceRow,
} from "./ServerParts";
import { serverClient } from "./serverClient";
import {
  AUDIT_SOURCE_LABELS,
  PLATFORM_LABELS,
  type AppListItemDto,
  type AuditEntryDto,
  type AuditSource,
  type ConfirmationDto,
  type LogsResultDto,
  type McpStatusDto,
  type ServerStatusDto,
  type ServiceListItemDto,
} from "./serverTypes";

interface ServerPageProps {
  active: boolean;
  setNotice: (message: string) => void;
}

const EMPTY_STATUS: ServerStatusDto | null = null;

export function ServerPage({ active, setNotice }: ServerPageProps) {
  const [status, setStatus] = useState<ServerStatusDto | null>(EMPTY_STATUS);
  const [services, setServices] = useState<ServiceListItemDto[]>([]);
  const [apps, setApps] = useState<AppListItemDto[]>([]);
  const [audit, setAudit] = useState<AuditEntryDto[]>([]);
  const [confirmation, setConfirmation] = useState<ConfirmationDto | null>(null);
  const [logs, setLogs] = useState<LogsResultDto | null>(null);
  const [mcp, setMcp] = useState<McpStatusDto | null>(null);
  const [auditFilter, setAuditFilter] = useState<AuditSource | "all">("all");
  const [loading, setLoading] = useState(false);
  const [pendingService, setPendingService] = useState<string | null>(null);

  const reload = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setLoading(true);
    try {
      const [nextStatus, nextServices, nextApps, nextAudit, nextMcp] =
        await Promise.all([
          serverClient.status(),
          serverClient.servicesList(),
          serverClient.appsList(),
          serverClient.actionsRecent(20),
          serverClient.mcpStatus().catch(() => null),
        ]);
      setStatus(nextStatus);
      setServices(nextServices);
      setApps(nextApps);
      setAudit(nextAudit);
      setMcp(nextMcp);
    } catch (error) {
      setNotice(`服务器状态读取失败：${errorMessage(error)}`);
    } finally {
      setLoading(false);
    }
  }, [setNotice]);

  useEffect(() => {
    if (active) void reload();
  }, [active, reload]);

  const requestRestart = async (serviceId: string) => {
    setPendingService(serviceId);
    try {
      const ticket = await serverClient.requestRestart(serviceId);
      setConfirmation(ticket);
    } catch (error) {
      setNotice(`重启请求被拒绝：${errorMessage(error)}`);
    } finally {
      setPendingService(null);
    }
  };

  const confirmRestart = async () => {
    if (!confirmation) return;
    try {
      const result = await serverClient.confirmAction(
        confirmation.confirmation_id,
        confirmation.target_id,
      );
      setNotice(
        result.outcome === "success"
          ? `已重启 ${confirmation.target_id}`
          : `操作结果：${result.outcome}`,
      );
      setConfirmation(null);
      await reload();
    } catch (error) {
      setNotice(`执行失败：${errorMessage(error)}`);
    }
  };

  const cancelRestart = async () => {
    if (!confirmation) return;
    try {
      await serverClient.cancelAction(confirmation.confirmation_id);
      setNotice("已取消");
    } catch (error) {
      setNotice(`取消失败：${errorMessage(error)}`);
    } finally {
      setConfirmation(null);
    }
  };

  const openLogs = async (serviceId: string) => {
    try {
      const result = await serverClient.logs({ serviceId, maxLines: 60 });
      setLogs(result);
    } catch (error) {
      setNotice(`日志读取失败：${errorMessage(error)}`);
    }
  };

  const openApp = async (url: string) => {
    try {
      await openUrl(url);
    } catch (error) {
      setNotice(`打开失败：${errorMessage(error)}`);
    }
  };

  const filteredAudit =
    auditFilter === "all"
      ? audit
      : audit.filter((entry) => entry.source === auditFilter);

  if (!isTauriRuntime()) {
    return (
      <div className="server-page">
        <p className="knowledge-empty">
          浏览器预览不支持家庭服务器，请在桌面端使用。
        </p>
      </div>
    );
  }

  const health = status?.health.overall ?? "unknown";

  return (
    <div className="server-page">
      <header className="server-head">
        <div>
          <h2>
            Home Server
            <HealthBadge status={health} />
          </h2>
          <p className="server-subtitle">
            {status
              ? `${status.hostname || "未知主机"} · ${PLATFORM_LABELS[status.platform]} ${status.os_version} · 已运行 ${formatUptime(status.uptime_secs)}`
              : "读取中…"}
          </p>
        </div>
        <button
          className="server-refresh"
          onClick={() => void reload()}
          disabled={loading}
        >
          <ArrowClockwise size={14} />
          刷新
        </button>
      </header>

      {health !== "healthy" && status && status.health.reasons.length > 0 ? (
        <ul className="server-reasons">
          {status.health.reasons.slice(0, 4).map((reason) => (
            <li key={`${reason.code}-${reason.detail}`}>
              <Warning size={13} />
              <span>
                <code>{reason.code}</code> {reason.detail}
              </span>
            </li>
          ))}
        </ul>
      ) : null}

      <section className="server-metrics">
        <MetricTile
          icon={<Cpu size={15} />}
          label="CPU"
          value={ratioText(status?.cpu_usage_ratio ?? null)}
          hint={status ? `${status.cpu_cores} 核` : ""}
        />
        <MetricTile
          icon={<MemoryIcon size={15} />}
          label="Memory"
          value={ratioText(status?.memory_usage_ratio ?? null)}
          hint={status ? formatBytes(status.memory_total_bytes) : ""}
        />
        <MetricTile
          icon={<HardDrives size={15} />}
          label="Storage"
          value={
            status?.tightest_volume
              ? `${ratioText(status.tightest_volume_ratio)} · ${status.tightest_volume}`
              : "—"
          }
          hint="最紧张的卷"
        />
      </section>

      {mcp ? (
        <section className="server-mcp">
          <div className="server-mcp-head">
            <h3>MCP</h3>
            <span
              className={
                "server-badge " +
                (mcp.mcp.enabled ? "healthy" : "unhealthy")
              }
            >
              {mcp.mcp.enabled ? "已启用" : "已停用"}
            </span>
          </div>
          <p className="knowledge-muted">
            STDIO {mcp.mcp.stdio_enabled ? "开" : "关"} · HTTP{" "}
            {mcp.mcp.http_enabled ? `开（${mcp.mcp.bind}:${mcp.mcp.port}）` : "关"} ·
            远程 {mcp.mcp.remote_enabled ? "开" : "关"}
          </p>
          <p className="knowledge-muted">{mcp.auth_status}</p>
        </section>
      ) : null}

      <section className="server-block">
        <h3>Services</h3>
        {services.length === 0 ? (
          <p className="knowledge-muted">
            尚未注册任何服务（在 settings.json 的 server.services 中配置）。
          </p>
        ) : (
          <ul className="server-list">
            {services.map((service) => (
              <ServiceRow
                key={service.service_id}
                service={service}
                pending={pendingService === service.service_id}
                onRestart={() => void requestRestart(service.service_id)}
                onLogs={() => void openLogs(service.service_id)}
              />
            ))}
          </ul>
        )}
      </section>

      <section className="server-block">
        <h3>Applications</h3>
        {apps.length === 0 ? (
          <p className="knowledge-muted">
            尚未注册任何应用（在 settings.json 的 server.applications 中配置）。
          </p>
        ) : (
          <ul className="server-list">
            {apps.map((app) => (
              <AppRow
                key={app.app_id}
                app={app}
                onOpen={() => void openApp(app.url)}
              />
            ))}
          </ul>
        )}
      </section>

      {logs ? (
        <section className="server-block">
          <h3>
            日志 · {logs.service_id}
            <span className="server-untrusted">不可信数据</span>
          </h3>
          <p className="knowledge-muted">
            {logs.lines} 行 · 脱敏 {logs.redactions} 处
            {logs.truncated ? " · 已截断" : ""}
          </p>
          <pre className="server-log">{logs.text}</pre>
        </section>
      ) : null}

      {confirmation ? (
        <ConfirmActionCard
          confirmation={confirmation}
          onConfirm={() => void confirmRestart()}
          onCancel={() => void cancelRestart()}
        />
      ) : null}

      <section className="server-block">
        <div className="server-block-head">
          <h3>Recent Actions</h3>
          <div className="server-audit-filter">
            {(["all", "desktop", "mcp"] as const).map((option) => (
              <button
                key={option}
                className={auditFilter === option ? "active" : ""}
                onClick={() => setAuditFilter(option)}
              >
                {option === "all" ? "全部" : AUDIT_SOURCE_LABELS[option]}
              </button>
            ))}
          </div>
        </div>
        {filteredAudit.length === 0 ? (
          <p className="knowledge-muted">还没有执行过任何操作。</p>
        ) : (
          <ul className="server-audit">
            {filteredAudit.map((entry) => (
              <AuditRow key={entry.id} entry={entry} />
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function ratioText(ratio: number | null): string {
  if (ratio === null || Number.isNaN(ratio)) return "—";
  return `${Math.round(ratio * 100)}%`;
}

function formatUptime(seconds: number): string {
  if (seconds <= 0) return "未知";
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3_600);
  if (days > 0) return `${days} 天 ${hours} 小时`;
  const minutes = Math.floor((seconds % 3_600) / 60);
  return `${hours} 小时 ${minutes} 分`;
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "—";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${units[unit]}`;
}
