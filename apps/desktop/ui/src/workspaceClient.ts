/**
 * Workspace（工作区文件系统）系统能力客户端（Gate 6）。
 *
 * `list_workspace` 是外壳级能力（当前唯一消费者是 Markdown 页的文件树），
 * 不归属任何单个 Feature 页面，因此单独成 client。
 */
import type { CommandTransport } from "./transport";
import { tauriTransport } from "./transport";
import type { WorkspaceFile } from "./types";

export interface WorkspaceClient {
  list(path: string): Promise<WorkspaceFile[]>;
}

export function createWorkspaceClient(
  transport: CommandTransport = tauriTransport,
): WorkspaceClient {
  return {
    list: (path) =>
      transport.invoke<WorkspaceFile[]>("list_workspace", { path }),
  };
}

export const workspaceClient = createWorkspaceClient();