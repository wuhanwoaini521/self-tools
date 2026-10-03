/**
 * 系统就绪度客户端：走统一 transport（桌面 = Tauri IPC，网页 = HTTP）。
 *
 * 与后端 `readiness_report` / `readiness_diagnostics` 命令一一对应。
 */
import { defaultTransport } from "../../transport";
import type { CommandTransport } from "../../transport";
import type { DiagnosticCheckDto, ReadinessReportDto } from "./SystemReadinessPage";

export interface ReadinessClient {
  report(): Promise<ReadinessReportDto>;
  diagnostics(): Promise<DiagnosticCheckDto[]>;
}

export function createReadinessClient(
  transport: CommandTransport = defaultTransport,
): ReadinessClient {
  return {
    report: () => transport.invoke<ReadinessReportDto>("readiness_report"),
    diagnostics: () =>
      transport.invoke<DiagnosticCheckDto[]>("readiness_diagnostics"),
  };
}

export const readinessClient = createReadinessClient();
