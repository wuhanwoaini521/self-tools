/**
 * PWA 生命周期管理（V11 §76-§83）：
 * - register service worker（仅 HTTPS 或 localhost 的安全上下文）
 * - 更新提示（Update available [Reload]）
 * - 离线指示（navigator.onLine + online/offline 事件）
 * - 前后端版本兼容检查
 */

/** 前端版本（与 package.json 同步；构建时由 CI/脚本校验）。 */
export const APP_VERSION = "0.1.0";

/** 后端要求的最低前端版本（明显不兼容 → 升级提示）。 */
export const MIN_BACKEND_COMPAT_VERSION = "0.1.0";

export type PwaStatus = "unsupported" | "registered" | "update-available" | "offline";

export interface PwaState {
  status: PwaStatus;
  online: boolean;
  updateReady: boolean;
  /** 与后端不兼容时的提示（为空 = 兼容）。 */
  versionNotice: string | null;
}

/** 比较语义化版本（major.minor.patch）。返回 <0 / 0 / >0。 */
export function compareVersions(a: string, b: string): number {
  const parse = (value: string): number[] =>
    value
      .split(".")
      .map((part) => Number.parseInt(part, 10))
      .map((part) => (Number.isFinite(part) ? part : 0));
  const left = parse(a);
  const right = parse(b);
  for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
    const l = left[index] ?? 0;
    const r = right[index] ?? 0;
    if (l !== r) return l < r ? -1 : 1;
  }
  return 0;
}

/** 是否处于可注册 SW 的安全上下文（§77：手机/iPad 最终使用必须 HTTPS）。 */
export function supportsServiceWorker(): boolean {
  return (
    typeof navigator !== "undefined" &&
    "serviceWorker" in navigator &&
    (window.isSecureContext === true || location.hostname === "localhost" || location.hostname === "127.0.0.1")
  );
}

export interface PwaHandlers {
  onStateChange?: (state: PwaState) => void;
}

/**
 * 注册 service worker 并接好更新/离线事件。
 * 返回取消函数（测试 / HMR 用）。
 */
export function registerPwa(handlers: PwaHandlers = {}): () => void {
  if (typeof window === "undefined") return () => {};

  const state: PwaState = {
    status: supportsServiceWorker() ? "registered" : "unsupported",
    online: navigator.onLine,
    updateReady: false,
    versionNotice: null,
  };
  const emit = () => handlers.onStateChange?.({ ...state });

  const goOnline = () => {
    state.online = true;
    if (state.status === "offline") state.status = "registered";
    emit();
  };
  const goOffline = () => {
    state.online = false;
    state.status = "offline";
    emit();
  };
  window.addEventListener("online", goOnline);
  window.addEventListener("offline", goOffline);

  if (!supportsServiceWorker()) {
    emit();
    return () => {
      window.removeEventListener("online", goOnline);
      window.removeEventListener("offline", goOffline);
    };
  }

  let reloading = false;
  // §82：新 SW 接管 → 刷新，避免长期旧前端 + 新后端。
  navigator.serviceWorker.addEventListener("controllerchange", () => {
    if (reloading) return;
    reloading = true;
    window.location.reload();
  });

  void (async () => {
    try {
      const registration = await navigator.serviceWorker.register("/sw.js", { scope: "/" });
      const checkWaiting = () => {
        if (registration.waiting && navigator.serviceWorker.controller) {
          state.updateReady = true;
          state.status = "update-available";
          emit();
        }
      };
      checkWaiting();
      registration.addEventListener("updatefound", () => {
        const installing = registration.installing;
        if (!installing) return;
        installing.addEventListener("statechange", () => {
          if (installing.state === "installed" && navigator.serviceWorker.controller) {
            checkWaiting();
          }
        });
      });
      emit();
    } catch {
      // SW 注册失败不阻塞应用（降级为普通网页）。
      state.status = "unsupported";
      emit();
    }
  })();

  return () => {
    window.removeEventListener("online", goOnline);
    window.removeEventListener("offline", goOffline);
  };
}

/** 用户点击 [Reload]：让等待中的 SW 接管。 */
export function applyUpdate(): void {
  if (!("serviceWorker" in navigator)) {
    window.location.reload();
    return;
  }
  void (async () => {
    const registration = await navigator.serviceWorker.getRegistration();
    if (registration?.waiting) {
      registration.waiting.postMessage("SKIP_WAITING");
    } else {
      window.location.reload();
    }
  })();
}

/** 前后端版本兼容判断（§83）。 */
export function versionNotice(backendVersion: string | null | undefined): string | null {
  if (!backendVersion) return null;
  if (compareVersions(backendVersion, MIN_BACKEND_COMPAT_VERSION) < 0) {
    return `后端版本 ${backendVersion} 过旧，请升级 self-tools 服务端。`;
  }
  return null;
}
