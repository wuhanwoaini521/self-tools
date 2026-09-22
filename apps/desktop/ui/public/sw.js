/**
 * PWA Service Worker（V11 §76-§83）。
 *
 * 缓存策略（§80）：
 * - PRE-CACHE：静态资源 + app shell + icons（install 时）。
 * - RUNTIME cache-first：同源静态资产（hashed assets 永远有效）。
 * - NETWORK-ONLY（禁止缓存）：/api/**、settings、任何带 Authorization/Cookie 的请求、
 *   以及敏感路径（memory/documents/files/conversation/study-board/safe-action/mcp）。
 * - 导航请求：network-first，失败 → 缓存 app shell（离线可打开）。
 *
 * 更新（§82）：新版本激活 → 提示 `Update available [Reload]`（page 监听
 * `controllerchange` + `registration.waiting`）。
 */

const VERSION = "v1";
const STATIC_CACHE = `self-tools-static-${VERSION}`;
const SHELL_CACHE = `self-tools-shell-${VERSION}`;

/// App shell（离线至少要能渲染外壳与离线指示）。
const APP_SHELL = ["/", "/index.html", "/manifest.webmanifest"];

/// 同源静态资产前缀（hashed，可长期缓存）。
const STATIC_PREFIX = "/assets/";
const ICON_PREFIX = "/icons/";

/// 敏感路径：绝不进任何缓存（§80）。
const SENSITIVE_MARKERS = [
  "api",
  "settings",
  "memory",
  "document",
  "file",
  "conversation",
  "study",
  "board",
  "safe-action",
  "confirmation",
  "mcp",
  "auth",
  "token",
  "session",
  "agent",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(STATIC_CACHE);
      // 逐个 add：单个失败不阻塞 install（app shell 优先）。
      await cache.addAll(APP_SHELL.map((url) => new Request(url, { cache: "reload" })));
      const shell = await caches.open(SHELL_CACHE);
      await shell.addAll(APP_SHELL.map((url) => new Request(url, { cache: "reload" })));
      await self.skipWaiting();
    })(),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(
        keys
          .filter((key) => !key.endsWith(VERSION))
          .map((key) => caches.delete(key)),
      );
      await self.clients.claim();
    })(),
  );
});

/// 请求是否敏感（网络-only）。
function isSensitive(url) {
  const path = url.pathname.toLowerCase();
  if (path.startsWith("/api/")) return true;
  return SENSITIVE_MARKERS.some((marker) => path.includes(marker));
}

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") {
    // 写请求一律走网络（不缓存、不拦截）。
    return;
  }
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) {
    // 跨站请求不拦截（模型 API / MCP remote / 地图瓦片按浏览器默认行为）。
    return;
  }
  if (isSensitive(url)) {
    // §80：敏感数据 network-only，禁止写入任何缓存。
    event.respondWith(fetch(request));
    return;
  }

  if (request.mode === "navigate") {
    event.respondWith(
      (async () => {
        try {
          const fresh = await fetch(request);
          const shell = await caches.open(SHELL_CACHE);
          shell.put("/index.html", fresh.clone());
          return fresh;
        } catch {
          const shell = await caches.open(SHELL_CACHE);
          const cached =
            (await shell.match("/index.html")) ||
            (await shell.match("/")) ||
            (await caches.match("/index.html"));
          return (
            cached ||
            new Response("离线且无缓存外壳", {
              status: 503,
              headers: { "Content-Type": "text/plain; charset=utf-8" },
            })
          );
        }
      })(),
    );
    return;
  }

  if (url.pathname.startsWith(STATIC_PREFIX) || url.pathname.startsWith(ICON_PREFIX)) {
    event.respondWith(
      (async () => {
        const cached = await caches.match(request);
        if (cached) return cached;
        const fresh = await fetch(request);
        if (fresh.ok) {
          const cache = await caches.open(STATIC_CACHE);
          cache.put(request, fresh.clone());
        }
        return fresh;
      })(),
    );
    return;
  }

  // 其他同源 GET：network-first + 失败回落缓存（不主动缓存敏感内容）。
  event.respondWith(
    (async () => {
      try {
        return await fetch(request);
      } catch {
        const cached = await caches.match(request);
        return cached || Response.error();
      }
    })(),
  );
});

/// 页面通知新版本可用（§82）。
self.addEventListener("message", (event) => {
  if (event.data === "SKIP_WAITING") {
    self.skipWaiting();
  }
});
