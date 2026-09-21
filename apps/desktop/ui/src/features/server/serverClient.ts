/**
 * Home Server（V7）前端命令客户端。
 *
 * 每个方法对应一个真实 Tauri 命令；页面只调用本 client，不直接接触 transport。
 * **写操作没有「直接执行」入口**：`requestRestart` 只拿确认票据，
 * `confirmAction` 才真正执行（V7 §68/§69）。
 */
import type { CommandTransport } from "../../transport";
import { tauriTransport } from "../../transport";
import type {
  ActionResultDto,
  AppListItemDto,
  AuditEntryDto,
  ConfirmationDto,
  LogsRequestDto,
  LogsResultDto,
  ServerStatusDto,
  ServiceListItemDto,
} from "./serverTypes";

/** 浏览器预览下的统一错误文案（页面据此展示空态）。 */
export const BROWSER_PREVIEW_MESSAGE = "浏览器预览不支持家庭服务器，请在桌面端使用。";

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
}

export function createServerClient(
  transport: CommandTransport = tauriTransport,
): ServerClient {
  const guard = () => {
    if (!transport.isTauriRuntime()) {
      throw new Error(BROWSER_PREVIEW_MESSAGE);
    }
  };

  return {
    async status() {
      guard();
      return transport.invoke<ServerStatusDto>("server_status");
    },
    async servicesList() {
      guard();
      return transport.invoke<ServiceListItemDto[]>("server_services_list");
    },
    async appsList() {
      guard();
      return transport.invoke<AppListItemDto[]>("server_apps_list");
    },
    async requestRestart(serviceId: string) {
      guard();
      return transport.invoke<ConfirmationDto>("services_restart", {
        serviceId,
      });
    },
    async confirmAction(confirmationId: string, serviceId: string) {
      guard();
      return transport.invoke<ActionResultDto>("confirm_action", {
        confirmationId,
        serviceId,
      });
    },
    async cancelAction(confirmationId: string) {
      guard();
      return transport.invoke<ActionResultDto>("cancel_action", {
        confirmationId,
      });
    },
    async actionsRecent(limit = 20) {
      guard();
      return transport.invoke<AuditEntryDto[]>("server_actions_recent", {
        limit,
      });
    },
    async logs(request: LogsRequestDto) {
      guard();
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
