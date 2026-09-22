import { chromium } from "playwright";

const BASE_URL = process.env.QA_BASE_URL ?? "http://127.0.0.1:1420";

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const messages = [];
page.on("console", (message) => messages.push(`${message.type()}: ${message.text()}`));
page.on("pageerror", (error) => messages.push(`pageerror: ${error.message}`));
await page.goto(`${BASE_URL}/`, { waitUntil: "load" });
await page.waitForTimeout(2500);
const text = await page.evaluate(() => {
  const pre = Array.from(document.querySelectorAll("pre"));
  return pre.map((node) => (node.textContent ?? "").slice(0, 600));
});
console.log("=== PRE ===");
for (const entry of text) console.log(entry, "\n---");
console.log("=== CONSOLE (first 3) ===");
for (const entry of messages.slice(0, 3)) console.log(entry.slice(0, 400), "\n---");
await browser.close();
