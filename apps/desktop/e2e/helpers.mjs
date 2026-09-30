import { expect } from "@wdio/globals";

export async function waitForApp() {
  // Pin the intended Tauri window once; automatic title matching differs between
  // the native window title and the Vite document title on Windows.
  await browser.tauri.switchWindow("main");
  const shell = await $(".app-shell");
  await shell.waitForDisplayed({ timeout: 60_000 });
  await expect(shell).toBeDisplayed();
}

export async function openPage(label, id) {
  const navButton = await $(`//nav[@aria-label='功能导航']//button[normalize-space(.)='${label}']`);
  await navButton.click();
  await browser.waitUntil(
    async () => (await navButton.getAttribute("class"))?.includes("active") === true,
    { timeout: 5_000, timeoutMsg: `导航未切换到 ${label}` },
  );
  await expect($(".page-pane:not(.page-hidden)")).toBeDisplayed();
  void id;
}
