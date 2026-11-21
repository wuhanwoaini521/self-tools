import { expect } from "@wdio/globals";

export async function waitForApp() {
  // Pin the intended Tauri window once; automatic title matching differs between
  // the native window title and the Vite document title on Windows.
  await browser.tauri.switchWindow("main");
  const shell = await $(".app-shell");
  await shell.waitForDisplayed({ timeout: 60_000 });
  await expect(shell).toBeDisplayed();
}

/**
 * 通过 hash 深链打开页面。
 *
 * 产品壳已从「一个扁平导航列表」改成「一级入口 + 分区子导航 + 更多页面」，
 * 旧选择器（`nav[aria-label='功能导航']`、`.app-nav-item`）已不存在。
 * 但 `parseHashPath` 仍把每个 PageId 当作稳定契约，所以 E2E 统一用 hash
 * 打开页面；导航控件本身由 `navigation.e2e.mjs` 单独点击验证。
 *
 * 兼容旧签名 `openPage(label, id)`：第二个参数优先。
 */
export async function openPage(labelOrId, maybeId) {
  const pageId = maybeId ?? labelOrId;
  await browser.execute((id) => {
    window.location.hash = `#${id}`;
  }, pageId);
  // 上一个页面在 React 重渲染前仍然可见；先等 hash 生效，再等新页面挂载。
  await browser.waitUntil(
    async () =>
      await browser.execute((id) => window.location.hash === `#${id}`, pageId),
    { timeout: 5_000, timeoutMsg: `hash 未切换：${pageId}` },
  );
  await browser.pause(150);
  await browser.waitUntil(
    async () => (await $(".page-pane:not(.page-hidden)")).isDisplayed(),
    { timeout: 10_000, timeoutMsg: `页面未打开：${pageId}` },
  );
  await expect($(".page-pane:not(.page-hidden)")).toBeDisplayed();
}
