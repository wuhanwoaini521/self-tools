/**
 * 平台能力适配（Phase 1.5 的最小收口）。
 *
 * 组件不应该自己判断 `isTauriRuntime()` 再挑 API：能力是否可用的判断和
 * 具体实现放在这里，调用方只处理成功/失败。阶段 2.3 会把这里扩成完整的
 * capability guard；现在先统一「打开外链」与「打开本机文件」两个动作。
 */
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { isTauriRuntime } from "./utils";

/** 外链在两端都能打开（桌面交给系统浏览器，网页新标签）。 */
export function canOpenExternalUrl(): boolean {
  return true;
}

/** 唤起系统程序打开本机文件只有桌面端能做。 */
export function canOpenLocalPath(): boolean {
  return isTauriRuntime();
}

/**
 * 公开设置与凭据写入两端都有：桌面走 Tauri 命令，网页走网关 admin 端点
 * （`PUT /api/v1/settings/public` / `POST /api/v1/settings/secrets`）。
 * 权限是否足够由后端判定，这里只表达「客户端具备该能力」。
 */
export function canWriteSettings(): boolean {
  return true;
}

export const LOCAL_PATH_UNAVAILABLE = "网页端不能打开本机文件，请在桌面应用里操作。";

export async function openExternalUrl(url: string): Promise<void> {
  if (isTauriRuntime()) {
    await openUrl(url);
    return;
  }
  const opened = window.open(url, "_blank", "noopener,noreferrer");
  if (!opened) {
    throw new Error("浏览器阻止了新标签页，请允许弹窗后重试。");
  }
}

export async function revealLocalPath(path: string): Promise<void> {
  if (!isTauriRuntime()) {
    throw new Error(LOCAL_PATH_UNAVAILABLE);
  }
  await openPath(path);
}
