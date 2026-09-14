/**
 * Markdown / Documents 模块的前端命令客户端（Gate 6）。
 *
 * 每个方法对应一个真实 Tauri 命令（命令名 / 参数 / 返回类型与后端契约冻结一致）。
 * 工作区文件树扫描（`list_workspace`）属 workspace 能力，见 `../../workspaceClient`。
 */
import type { CommandTransport } from "../../transport";
import { tauriTransport } from "../../transport";
import type { DocumentDto } from "../../types";

export interface MarkdownClient {
  read(path: string): Promise<DocumentDto>;
  write(path: string, text: string): Promise<void>;
  cycleTaskLines(lines: string[], step: number): Promise<string[]>;
}

export function createMarkdownClient(
  transport: CommandTransport = tauriTransport,
): MarkdownClient {
  return {
    read: (path) => transport.invoke<DocumentDto>("read_document", { path }),
    write: (path, text) =>
      transport.invoke<void>("write_document", { path, text }),
    cycleTaskLines: (lines, step) =>
      transport.invoke<string[]>("cycle_task_lines", { lines, step }),
  };
}

export const markdownClient = createMarkdownClient();