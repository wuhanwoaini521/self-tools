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

export interface DiagnosticCheckDto {
  id: string;
  label: string;
  ok: boolean;
  detail: string;
  duration_ms: number;
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

  // 无后端命令可用时（纯浏览器 dev），用本地推导的兜底报告。
  useEffect(() => {
    if (report) return;
    setReport({
      overall: "degraded",
      checks: localChecks(),
      generated_at: Math.floor(Date.now() / 1000),
    });
  }, [report]);

  const runDiagnostics = useCallback(async () => {
    setRunning(true);
    setNotice("");
    try {
      // 后端 ReadinessService 就绪后由组合根经 Tauri 命令暴露；
      // 目前只运行浏览器侧可做的安全 READ 检查（不碰任何用户数据）。
      const started = Date.now();
      const secure = window.isSecureContext === true;
      const sw = "serviceWorker" in navigator;
      const online = navigator.onLine;
      const registration = sw ? await navigator.serviceWorker.getRegistration() : undefined;
      setDiagnostics([
        {
          id: "pwa_secure_context",
          label: "安全上下文",
          ok: secure,
          detail: secure ? "HTTPS / localhost" : "非安全上下文",
          duration_ms: 0,
        },
        {
          id: "pwa_service_worker",
          label: "Service Worker",
          ok: Boolean(registration),
          detail: registration ? "已注册" : "未注册",
          duration_ms: Date.now() - started,
        },
        {
          id: "network",
          label: "网络",
          ok: online,
          detail: online ? "在线" : "离线",
          duration_ms: 0,
        },
      ]);
      setNotice("诊断完成（仅浏览器侧安全 READ 检查）");
    } catch (error) {
      setNotice(`诊断失败：${String(error)}`);
    } finally {
      setRunning(false);
    }
  }, []);

  if (!report) return null;

  const overallClass = STATUS_CLASS[report.overall];

  return (
    <div className="page-scroll readiness-page">
      <header className="readiness-head">
        <div>
          <h1>System Readiness</h1>
          <p>系统状态一屏可见；只报告配置状态，不显示任何密钥。</p>
        </div>
        <span className={"readiness-overall " + overallClass}>
          {STATUS_LABELS[report.overall]}
        </span>
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
              <li key={item.id} className={item.ok ? "ok" : "warn"}>
                <span>{item.label}</span>
                <small>
                  {item.detail} · {item.duration_ms}ms
                </small>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
