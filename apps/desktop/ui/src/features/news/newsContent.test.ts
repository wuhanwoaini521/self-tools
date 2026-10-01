import { describe, expect, it } from "vitest";
import { deduplicateRepeatedText, prepareRssContent, stripRssHtml } from "./newsContent";

describe("newsContent deduplication", () => {
  it("deduplicates exact repeated photo captions in text", () => {
    const repeated =
      "10月1日，内蒙古呼和浩特市，民众在商场黄金专柜挑选金饰。中新社记者 刘文华 摄 10月1日，内蒙古呼和浩特市，民众在商场黄金专柜挑选金饰。中新社记者 刘文华 摄 10月1日，内蒙古呼和浩特市，民众在商场黄金专柜挑选金饰。中新社记者 刘文华 摄 10月1日，内蒙古呼和浩特市，民众在商场黄金专柜挑选金饰。中新社记者 刘文华 摄 10月1日，内蒙古呼和浩特市，民众在商场黄金专柜挑选金饰。中新社记者 刘文华 摄";

    const result = deduplicateRepeatedText(repeated);
    expect(result).toBe("10月1日，内蒙古呼和浩特市，民众在商场黄金专柜挑选金饰。中新社记者 刘文华 摄");
  });

  it("deduplicates repeated text in stripRssHtml", () => {
    const raw =
      "<p>10月1日，内蒙古呼和浩特市，民众在商场黄金专柜挑选金饰。中新社记者 刘文华 摄</p> <p>10月1日，内蒙古呼和浩特市，民众在商场黄金专柜挑选金饰。中新社记者 刘文华 摄</p>";

    const cleaned = stripRssHtml(raw);
    expect(cleaned).toBe("10月1日，内蒙古呼和浩特市，民众在商场黄金专柜挑选金饰。中新社记者 刘文华 摄");
  });

  it("preserves non-repeated normal articles", () => {
    const normal = "今天天气晴朗，气温适宜。广大市民纷纷出门游玩，各大景区秩序井然。";
    expect(deduplicateRepeatedText(normal)).toBe(normal);
  });
});
