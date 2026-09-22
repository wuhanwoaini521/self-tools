/**
 * PWA 状态条（V11 §81/§82）：
 * - 离线：显示明确指示 + 说明哪些功能不可用（不伪装在线）
 * - 更新：Update available [Reload]
 * - 版本不兼容：升级提示（§83）
 */

import { useEffect, useState } from "react";
import { ArrowClockwise, PlugsConnected, Warning } from "@phosphor-icons/react";
import { APP_VERSION, applyUpdate, registerPwa, versionNotice, type PwaState } from "./pwa";

const INITIAL: PwaState = {
  status: "unsupported",
  online: true,
  updateReady: false,
  versionNotice: null,
};

export function PwaBanner() {
  const [state, setState] = useState<PwaState>(INITIAL);
  const [backendVersion, setBackendVersion] = useState<string | null>(null);

  useEffect(() => {
    const unregister = registerPwa({ onStateChange: setState });
    return unregister;
  }, []);

  // 后端版本：从 /api/health 之类拿（失败不影响本条）；只做兼容判断。
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const response = await fetch("/api/health", { headers: { Accept: "application/json" } });
        if (!response.ok) return;
        const body = (await response.json()) as { version?: string };
        if (!cancelled) setBackendVersion(body.version ?? null);
      } catch {
        // 离线 / 无后端：保留 null（不做兼容判断）。
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const notice = versionNotice(backendVersion);
  if (state.status === "unsupported" && !notice) return null;

  return (
    <div className="pwa-banners">
      {notice ? (
        <div className="pwa-banner pwa-banner-warn" role="alert">
          <Warning size={14} />
          <span>{notice}</span>
        </div>
      ) : null}
      {state.status === "update-available" ? (
        <div className="pwa-banner pwa-banner-update" role="status">
          <span>新版本可用（当前 {APP_VERSION}）</span>
          <button type="button" onClick={() => void applyUpdate()}>
            <ArrowClockwise size={14} />
            重新加载
          </button>
        </div>
      ) : null}
      {!state.online ? (
        <div className="pwa-banner pwa-banner-offline" role="status">
          <PlugsConnected size={14} />
          <span>
            离线模式：AI、联网搜索、远程 MCP 与服务器操作不可用；本地界面仍可使用。
          </span>
        </div>
      ) : null}
    </div>
  );
}
