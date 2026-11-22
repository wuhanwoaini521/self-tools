/**
 * Home Server（V7）前端命令客户端。
 *
 * 每个方法对应一个真实 Tauri 命令；页面只调用本 client，不直接接触 transport。
 * **写操作没有「直接执行」入口**：`requestRestart` 只拿确认票据，
 * `confirmAction` 才真正执行（V7 §68/§69）。
 */
import type { CommandTransport } from "../../transport";
import { defaultTransport } from "../../transport";
import type {
  ActionResultDto,
  AppListItemDto,
  AuditEntryDto,
  ConfirmationDto,
  LogsRequestDto,
  LogsResultDto,
  McpStatusDto,
  ServerStatusDto,
  ServiceListItemDto,
} from "./serverTypes";

export interface ServerClient {
  status(): Promise<ServerStatusDto>;
  servicesList(): Promise<ServiceListItemDto[]>;
  appsList(): Promise<AppListItemDto[]>;
  /** 请求重启：只返回确认票据，不执行（§68）。 */
  requestRestart(serviceId: string): Promise<ConfirmationDto>;
  /** 用户确认后执行（§69：immutable confirmed request）。 */
  confirmAction(confirmationId: string, serviceId: string): Promise<ActionResultDto>;
  cancelAction(confirmationId: string): Promise<ActionResultDto>;
  actionsRecent(limit?: number): Promise<AuditEntryDto[]>;
  logs(request: LogsRequestDto): Promise<LogsResultDto>;
  mcpStatus(): Promise<McpStatusDto>;
}

export function createServerClient(
  transport: CommandTransport = defaultTransport,
): ServerClient {
  return {
    async status() {
      return transport.invoke<ServerStatusDto>("server_status");
    },
    async servicesList() {
      return transport.invoke<ServiceListItemDto[]>("server_services_list");
    },
    async appsList() {
      return transport.invoke<AppListItemDto[]>("server_apps_list");
    },
    async requestRestart(serviceId: string) {
      return transport.invoke<ConfirmationDto>("services_restart", {
        serviceId,
      });
    },
    async confirmAction(confirmationId: string, serviceId: string) {
      return transport.invoke<ActionResultDto>("confirm_action", {
        confirmationId,
        serviceId,
      });
    },
    async cancelAction(confirmationId: string) {
      return transport.invoke<ActionResultDto>("cancel_action", {
        confirmationId,
      });
    },
    async actionsRecent(limit = 20) {
      return transport.invoke<AuditEntryDto[]>("server_actions_recent", {
        limit,
      });
    },
    async mcpStatus() {
      return transport.invoke<McpStatusDto>("mcp_status");
    },
    async logs(request: LogsRequestDto) {
      return transport.invoke<LogsResultDto>("services_get_logs", {
        serviceId: request.serviceId,
        logSourceId: request.logSourceId,
        maxLines: request.maxLines,
        maxBytes: request.maxBytes,
        maxAgeSecs: request.maxAgeSecs,
      });
    },
  };
}

export const serverClient = createServerClient();
