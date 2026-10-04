/**
 * System Readiness（V11 §123-§127）：明早验收不需要先读 README。
 *
 * 13 项固定检查 id（与 `crates/core/src/readiness/model.rs` 冻结一致）：
 * backend / database / ai_provider / vision / decision / jev / search /
 * mcp_local / mcp_remote / home_server / backup / pwa_secure_context / device_session
 *
 * 只显示「已配置 / 未配置 / 就绪」类状态，**永不**显示 secret 值。
 */

import { useCallback, useEffect, useState } from "react";
import { errorMessage } from "../../utils";
import { readinessClient } from "./readinessClient";
import {
  CircleNotch,
  Play,
  ShieldCheck,
  Warning,
  XCircle,
} from "@phosphor-icons/react";

export type ReadinessStatus = "ready" | "degraded" | "not_configured" | "failed";

export interface ReadinessCheckDto {
  id: string;
  label: string;
  status: ReadinessStatus;
  detail: string;
  blocking: boolean;
}

export interface ReadinessReportDto {
  overall: ReadinessStatus;
  checks: ReadinessCheckDto[];
  generated_at: number;
}

/** 与后端 `devtoolbox_core::readiness::DiagnosticCheck` 字段一一对应。 */
export interface DiagnosticCheckDto {
  id: string;
  label: string;
  status: ReadinessStatus;
  detail: string;
  blocking: boolean;
}

const STATUS_LABELS: Record<ReadinessStatus, string> = {
  ready: "就绪",
  degraded: "降级",
  not_configured: "未配置",
  failed: "失败",
};

const STATUS_ICON: Record<ReadinessStatus, typeof ShieldCheck> = {
  ready: ShieldCheck,
  degraded: Warning,
  not_configured: Warning,
  failed: XCircle,
};

const STATUS_CLASS: Record<ReadinessStatus, string> = {
  ready: "ok",
  degraded: "warn",
  not_configured: "warn",
  failed: "bad",
};

/** 本地推导的 readiness（无后端时的兜底）：浏览器侧可判定的项直接给结论。 */
function localChecks(): ReadinessCheckDto[] {
  const secure = typeof window !== "undefined" && window.isSecureContext === true;
  const sw = typeof navigator !== "undefined" && "serviceWorker" in navigator;
  const online = typeof navigator !== "undefined" ? navigator.onLine : false;
  return [
    {
      id: "backend",
      label: "Backend",
      status: "ready",
      detail: "本地界面已加载",
      blocking: true,
    },
    {
      id: "pwa_secure_context",
      label: "PWA / Secure Context",
      status: secure && sw ? "ready" : "degraded",
      detail: secure
        ? sw
          ? "安全上下文 + Service Worker 可用"
          : "安全上下文可用，Service Worker 不可用"
        : "非安全上下文（手机/iPad 需 HTTPS 才能装 PWA）",
      blocking: false,
    },
    {
      id: "device_session",
      label: "Device Session",
      status: online ? "ready" : "degraded",
      detail: online ? "设备已连接" : "当前离线（PWA 外壳仍可用）",
      blocking: false,
    },
    {
      id: "mcp_remote",
      label: "MCP Remote",
      status: "not_configured",
      detail: "默认关闭（fail-closed）",
      blocking: false,
    },
  ];
}

/** 本地兜底报告（后端不可达时使用）；只判定浏览器能判的项，其余如实缺失。 */
function localReport(): ReadinessReportDto {
  return {
    overall: "degraded",
    checks: localChecks(),
    generated_at: Math.floor(Date.now() / 1000),
  };
}

/**
 * 把「由前端判定」的两项用浏览器事实覆盖后端占位。
 * 后端无法得知安全上下文 / 在线状态，这两项只有前端能给出真结论。
 */
function mergeFrontendChecks(report: ReadinessReportDto): ReadinessReportDto {
  const secure = window.isSecureContext === true;
  const sw = "serviceWorker" in navigator;
  const online = navigator.onLine;
  const overrides: Record<string, ReadinessCheckDto> = {
    pwa_secure_context: {
      id: "pwa_secure_context",
      label: "PWA / Secure Context",
      status: secure && sw ? "ready" : "degraded",
      detail: secure
        ? sw
          ? "安全上下文 + Service Worker 可用"
          : "安全上下文可用，Service Worker 不可用"
        : "非安全上下文（手机/iPad 需 HTTPS 才能装 PWA）",
      blocking: false,
    },
    device_session: {
      id: "device_session",
      label: "Device Session",
      status: online ? "ready" : "degraded",
      detail: online ? "设备已连接" : "当前离线（PWA 外壳仍可用）",
      blocking: false,
    },
  };
  const checks = report.checks.map((check) => overrides[check.id] ?? check);
  for (const [id, check] of Object.entries(overrides)) {
    if (!checks.some((item) => item.id === id)) checks.push(check);
  }
  const overall: ReadinessStatus = checks.some((c) => c.status === "failed")
    ? "degraded"
    : checks.some((c) => c.status === "degraded")
      ? "degraded"
      : checks.every((c) => c.status === "ready")
        ? "ready"
        : "degraded";
  return { ...report, checks, overall };
}

/** 浏览器侧安全 READ 检查（仅在后端诊断不可用时作为降级）。 */
async function localDiagnostics(): Promise<DiagnosticCheckDto[]> {
  const started = Date.now();
  const secure = window.isSecureContext === true;
  const sw = "serviceWorker" in navigator;
  const online = navigator.onLine;
  const registration = sw ? await navigator.serviceWorker.getRegistration() : undefined;
  void started;
  const mk = (id: string, label: string, ok: boolean, detail: string): DiagnosticCheckDto => ({
    id,
    label,
    status: ok ? "ready" : "degraded",
    detail,
    blocking: false,
  });
  return [
    mk("pwa_secure_context", "安全上下文", secure, secure ? "HTTPS / localhost" : "非安全上下文"),
    mk(
      "pwa_service_worker",
      "Service Worker",
      Boolean(registration),
      registration ? "已注册" : "未注册",
    ),
    mk("network", "网络", online, online ? "在线" : "离线"),
  ];
}

const REQUIRED_IDS = [
  "backend",
  "database",
  "ai_provider",
  "vision",
  "decision",
  "jev",
  "search",
  "mcp_local",
  "mcp_remote",
  "home_server",
  "backup",
  "pwa_secure_context",
  "device_session",
];

export interface SystemReadinessPageProps {
  active: boolean;
}

export function SystemReadinessPage({ active }: SystemReadinessPageProps) {
  const [report, setReport] = useState<ReadinessReportDto | null>(null);
  const [diagnostics, setDiagnostics] = useState<DiagnosticCheckDto[] | null>(null);
  const [running, setRunning] = useState(false);
  const [notice, setNotice] = useState("");
  const [backendError, setBackendError] = useState<string | null>(null);

  /**
   * 优先使用后端真实探测（readiness_report）。
   *
   * 此前这里从不请求后端，直接用 4 项本地兜底渲染 13 项，缺失项落到
   * 「未配置 / 由后端 ReadinessService 提供（尚未装配）」——于是页面会谎报
   * `database 未配置`，而数据库实际正常读写。误导性报告比没有报告更糟。
   */
  const loadReport = useCallback(async () => {
    try {
      const remote = await readinessClient.report();
      setReport(mergeFrontendChecks(remote));
      setBackendError(null);
    } catch (error) {
      setBackendError(errorMessage(error));
      setReport(localReport());
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    void loadReport();
  }, [active, loadReport]);

  // 无后端命令可用时（纯浏览器 dev），用本地推导的兜底报告。
  useEffect(() => {
    if (report) return;
    setReport(localReport());
  }, [report]);

  const runDiagnostics = useCallback(async () => {
    setRunning(true);
    setNotice("");
    try {
      // 后端逐项诊断（与报告同源，不重复昂贵探测）。
      const remote = await readinessClient.diagnostics();
      setDiagnostics(remote);
      setNotice(`诊断完成（${remote.length} 项）`);
    } catch (error) {
      // 后端不可用时退回浏览器侧安全 READ 检查，并**明确说明**这不是后端结论。
      setDiagnostics(await localDiagnostics());
      setNotice(`后端诊断不可用（${errorMessage(error)}），以下仅为浏览器侧检查`);
    } finally {
      setRunning(false);
    }
  }, []);

  if (!report) return null;

  const overallClass = STATUS_CLASS[report.overall];

  return (
    <div className="page-scroll readiness-page page-shell">
      <header className="page-shell-head">
        <div className="page-shell-title">
          <span className="page-shell-eyebrow">system</span>
          <h1>System Readiness</h1>
          <p className="page-shell-desc">系统状态一屏可见；只报告配置状态，不显示任何密钥。</p>
        </div>
        <div className="page-shell-actions">
          <span className={"readiness-overall " + overallClass}>
            {STATUS_LABELS[report.overall]}
          </span>
        </div>
      </header>

      <section className="readiness-actions">
        <button type="button" onClick={() => void runDiagnostics()} disabled={running}>
          {running ? <CircleNotch size={16} /> : <Play size={16} />}
          {running ? "诊断运行中" : "运行诊断"}
        </button>
        {notice ? <span className="readiness-notice">{notice}</span> : null}
      </section>

      <ul className="readiness-list" aria-label="系统检查">
        {REQUIRED_IDS.map((id) => {
          const check = report.checks.find((item) => item.id === id);
          const status: ReadinessStatus = check?.status ?? "not_configured";
          const Icon = STATUS_ICON[status];
          return (
            <li key={id} className={"readiness-row " + STATUS_CLASS[status]}>
              <Icon size={16} />
              <span className="readiness-label">{check?.label ?? id}</span>
              <span className="readiness-status">{STATUS_LABELS[status]}</span>
              <span className="readiness-detail">
                {check?.detail ?? "由后端 ReadinessService 提供（尚未装配）"}
              </span>
              {check?.blocking ? <b className="readiness-blocking">关键</b> : null}
            </li>
          );
        })}
      </ul>

      {diagnostics ? (
        <section className="readiness-diagnostics">
          <h2>诊断结果</h2>
          <ul>
            {diagnostics.map((item) => (
              <li key={item.id} className={item.status === "ready" ? "ok" : "warn"}>
                <span>{item.label}</span>
                <small>{item.detail}</small>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
