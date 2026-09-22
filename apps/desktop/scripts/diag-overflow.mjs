import { chromium } from "playwright";

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 768, height: 1024 } });
await page.goto("http://127.0.0.1:1420/", { waitUntil: "networkidle" });
await page.waitForTimeout(500);
const info = await page.evaluate(() => {
  const doc = document.documentElement;
  const out = {
    scrollWidth: doc.scrollWidth,
    clientWidth: doc.clientWidth,
    device: doc.dataset.device,
    offenders: [],
  };
  document.querySelectorAll("*").forEach((el) => {
    const rect = el.getBoundingClientRect();
    if (rect.right > doc.clientWidth + 1 || rect.left < -1) {
      out.offenders.push({
        tag: el.tagName,
        cls: (el.className || "").toString().slice(0, 60),
        left: Math.round(rect.left),
        right: Math.round(rect.right),
        width: Math.round(rect.width),
      });
    }
  });
  out.offenders = out.offenders.slice(0, 15);
  return out;
});
console.log(JSON.stringify(info, null, 2));
await browser.close();
