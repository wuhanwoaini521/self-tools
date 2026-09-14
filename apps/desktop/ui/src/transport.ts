/**
 * 前端命令传输层（Gate 3 Boundary）。
 *
 * 唯一职责：把「命令名 + 参数 → Promise<T>」的调用与具体运行时隔离开。
 * 当前唯一实现是 TauriTransport；未来 Home Server / Web 场景增加 HTTP 实现时，
 * feature Client 不需要改动。
 *
 * 注意：不要在这里引入 RPC 框架 / 事件总线 / 中间件 —— 保持薄接口。
 */
import { invoke as tauriInvoke, type InvokeArgs } from "@tauri-apps/api/core";

export interface CommandTransport {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  /** 当前是否运行在 Tauri 桌面运行时（浏览器预览时返回 false）。 */
  isTauriRuntime(): boolean;
}

function inTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

/** Tauri 实现：唯一职责是把调用转发给 `@tauri-apps/api/core` 的 invoke。 */
export const tauriTransport: CommandTransport = {
  invoke: (command, args) =>
    // 边界转换：tauri 的 InvokeArgs 是尚未索引签名的递归类型，
    // `Record<string, unknown>` 与它结构兼容，仅在此处做一次显式断言。
    tauriInvoke(command, args as InvokeArgs | undefined),
  isTauriRuntime: inTauriRuntime,
};