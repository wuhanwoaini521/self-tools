import { chromium } from "playwright";

const BASE_URL = process.env.QA_BASE_URL ?? "http://127.0.0.1:1420";

const browser = await chromium.launch();
const context = await browser.newContext({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
const page = await context.newPage();
await page.goto(`${BASE_URL}/`, { waitUntil: "load" });
await page.waitForTimeout(900);
const info = await page.evaluate(() => {
  const doc = document.documentElement;
  const out = { clientWidth: doc.clientWidth, offenders: [] };
  document.querySelectorAll("*").forEach((el) => {
    const rect = el.getBoundingClientRect();
    if (rect.width > doc.clientWidth + 1) {
      const style = getComputedStyle(el);
      out.offenders.push({
        tag: el.tagName,
        cls: (el.className || "").toString().slice(0, 50),
        width: Math.round(rect.width),
        minWidth: style.minWidth,
        position: style.position,
      });
    }
  });
  out.offenders = out.offenders.slice(0, 12);
  return out;
});
console.log(JSON.stringify(info, null, 2));
await browser.close();
