/**
 * Settings（应用设置读写）系统能力 —— 前端命令客户端（Gate 6）。
 *
 * 包装普通设置的 `get_settings` / `put_settings` 与独立凭据写入命令；
 * 设置表单校验、主题逻辑、provider 选择等 UI 状态不属于本 client。
 */
import type { CommandTransport } from "./transport";
import { tauriTransport } from "./transport";
import type {
  AppSettings,
  SecretSettingsStatus,
  SecretSettingsUpdate,
} from "./types";

export interface SettingsClient {
  get(): Promise<AppSettings>;
  put(next: AppSettings): Promise<void>;
  putSecrets(update: SecretSettingsUpdate): Promise<SecretSettingsStatus>;
}

export function createSettingsClient(
  transport: CommandTransport = tauriTransport,
): SettingsClient {
  return {
    get: () => transport.invoke<AppSettings>("get_settings"),
    put: (next) => transport.invoke<void>("put_settings", { settings: next }),
    putSecrets: (update) =>
      transport.invoke<SecretSettingsStatus>("put_secret_settings", { update }),
  };
}

export const settingsClient = createSettingsClient();