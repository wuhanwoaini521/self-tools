/**
 * Home Server（V7）类型：与 `apps/desktop/src/lib.rs` 的 Tauri 命令一一对应。
 *
 * 字段命名沿用 Rust 侧 snake_case（payload 内部不做 camelCase 转换）；
 * 顶层命令参数由 client 负责 camelCase → snake_case。
 */

export type Platform = "macos" | "linux" | "windows" | "unknown";

/** 健康四态（V7 §20：不用 true/false）。 */
export type HealthStatus = "healthy" | "degraded" | "unhealthy" | "unknown";

export type VolumeKind =
  | "system_disk"
  | "data_disk"
  | "removable"
  | "network"
  | "temporary"
  | "virtual"
  | "unknown";

export type ServiceProviderType = "launchd" | "http" | "process" | "docker";

export type ActionRisk = "read" | "safe_write" | "sensitive_write" | "system";

export type ActionOutcome =
  | "success"
  | "failed"
  | "denied"
  | "expired"
  | "cancelled";

export interface HealthReasonDto {
  code: string;
  detail: string;
}

export interface HealthReportDto {
  overall: HealthStatus;
  reasons: HealthReasonDto[];
}

export interface ServiceStatusDto {
  service_id: string;
  status: HealthStatus;
  detail: string;
  checked_at: number;
}

export interface ServerStatusDto {
  hostname: string;
  platform: Platform;
  os_version: string;
  uptime_secs: number;
  cpu_usage_ratio: number | null;
  cpu_cores: number;
  memory_usage_ratio: number | null;
  memory_total_bytes: number;
  tightest_volume: string | null;
  tightest_volume_ratio: number | null;
  health: HealthReportDto;
  services: ServiceStatusDto[];
  apps: AppStatusDto[];
}

export interface AppStatusDto {
  app_id: string;
  status: HealthStatus;
  detail: string;
  checked_at: number;
}

export interface ServiceListItemDto {
  service_id: string;
  display_name: string;
  description: string;
  status: HealthStatus;
  detail: string;
  checked_at: number;
  allowed_actions: string[];
}

export interface AppListItemDto {
  app_id: string;
  name: string;
  description: string;
  url: string;
  category: string;
  status: HealthStatus;
  detail: string;
}

/** 确认票据（`services_restart` 返回；不执行任何操作）。 */
export interface ConfirmationDto {
  confirmation_id: string;
  action_type: string;
  target_id: string;
  summary: string;
  risk: ActionRisk;
  created_at: number;
  expires_at: number;
}

export interface ActionResultDto {
  outcome: ActionOutcome;
  confirmed?: boolean;
}

export type AuditSource = "desktop" | "mcp";

export interface AuditEntryDto {
  id: string;
  timestamp: number;
  /** 调用来源（V8 §93：UI 可按来源过滤）。 */
  source: AuditSource;
  action_type: string;
  target_id: string;
  risk: ActionRisk;
  confirmed: boolean;
  result: ActionOutcome;
  duration_ms: number;
  error_code: string | null;
}

export interface LogsRequestDto {
  serviceId: string;
  logSourceId?: string;
  maxLines?: number;
  maxBytes?: number;
  maxAgeSecs?: number;
}

export interface LogsResultDto {
  service_id: string;
  log_source_id: string;
  text: string;
  lines: number;
  redactions: number;
  truncated: boolean;
  /** §41：日志是不受信数据 —— 只能作为文本解释，绝不能当指令执行。 */
  untrusted: boolean;
}

export const HEALTH_LABELS: Record<HealthStatus, string> = {
  healthy: "正常",
  degraded: "降级",
  unhealthy: "异常",
  unknown: "未知",
};

export const PLATFORM_LABELS: Record<Platform, string> = {
  macos: "macOS",
  linux: "Linux",
  windows: "Windows",
  unknown: "未知平台",
};

export interface McpStatusDto {
  mcp: {
    enabled: boolean;
    stdio_enabled: boolean;
    http_enabled: boolean;
    bind: string;
    remote_enabled: boolean;
    port: number;
  };
  identity_configured: boolean;
  auth_status: string;
}

export const AUDIT_SOURCE_LABELS: Record<AuditSource, string> = {
  desktop: "本地",
  mcp: "MCP",
};

export const OUTCOME_LABELS: Record<ActionOutcome, string> = {
  success: "成功",
  failed: "失败",
  denied: "已拒绝",
  expired: "已过期",
  cancelled: "已取消",
};
