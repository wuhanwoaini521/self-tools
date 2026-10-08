/**
 * AI Bubble —— AI 不是普通搜索框，而是随时在旁的助手气泡。
 *
 * - `AIBubbleHero`：Home / Knowledge 等页面的主入口。小型气泡点击后
 *   `scale + fade` 展开成可输入的问句框；未展开时展示 AI 助手卡片。
 * - `AIBubbleLauncher`：全站右下角常驻气泡，任何页面都能唤起 AI。
 *
 * 只负责「外观 + 收集意图」，真正的对话仍交给 AIPanel（业务逻辑不重复）。
 */
import {
  Brain,
  FilePlus,
  Globe,
  Lightbulb,
  PaperPlaneTilt,
  Sparkle,
  X,
} from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";
import { SketchIllustration } from "../../components/sketch/SketchIllustrations";

function useAutoGrow(value: string) {
  const ref = useRef<HTMLTextAreaElement | null>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
  }, [value]);
  return ref;
}

const AI_SUGGESTIONS = [
  { icon: <Lightbulb size={16} />, label: "智能问答", desc: "快速获取答案" },
  { icon: <Brain size={16} />, label: "学习规划", desc: "制定个性化计划" },
  { icon: <Sparkle size={16} />, label: "知识关联", desc: "跨模块串联内容" },
  { icon: <Globe size={16} />, label: "工具调用", desc: "操作你的资源" },
];

export interface AIBubbleHeroProps {
  onAsk: (prompt: string) => void;
  onOpenFiles?: () => void;
  /** 页面语境提示，例如 "History · 毛泽东"。 */
  contextLabel?: string | null;
}

export function AIBubbleHero({ onAsk, onOpenFiles, contextLabel }: AIBubbleHeroProps) {
  const [expanded, setExpanded] = useState(false);
  const [value, setValue] = useState("");
  const [webSearch, setWebSearch] = useState(false);
  const [think, setThink] = useState(false);
  const textareaRef = useAutoGrow(value);

  const submit = () => {
    const trimmed = value.trim();
    if (!trimmed) return;
    const prefix = [webSearch ? "[联网搜索]" : "", think ? "[深度思考]" : ""]
      .filter(Boolean)
      .join(" ");
    onAsk(prefix ? `${prefix} ${trimmed}` : trimmed);
    setValue("");
    setExpanded(false);
  };

  return (
    <section className={"ai-hero" + (expanded ? " is-expanded" : "")} aria-label="AI 助手">
      <div className="ai-hero-robot" aria-hidden>
        <SketchIllustration name="ai" size={72} />
        <span className="ai-hero-bubble-hint">
          Hi! 我可以帮你解答问题
        </span>
      </div>

      <div className="ai-hero-main">
        {!expanded ? (
          <button
            type="button"
            className="ai-hero-collapsed"
            onClick={() => setExpanded(true)}
          >
            <Sparkle size={18} weight="fill" />
            <span>问 AI：任何关于你知识、学习或家庭服务器的问题…</span>
            <span className="ai-hero-send" aria-hidden>
              <PaperPlaneTilt size={18} weight="fill" />
            </span>
          </button>
        ) : (
          <div className="ai-hero-expanded">
            <textarea
              ref={textareaRef}
              className="ai-hero-textarea"
              aria-label="向 AI 提问"
              placeholder="问 AI：任何关于你知识、学习或家庭服务器的问题…"
              value={value}
              autoFocus
              onChange={(event) => setValue(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault();
                  submit();
                }
                if (event.key === "Escape") setExpanded(false);
              }}
            />
            <div className="ai-hero-toolbar">
              <button type="button" className="ai-hero-tool" onClick={onOpenFiles}>
                <FilePlus size={15} /> 知识库文件
              </button>
              <button
                type="button"
                className={"ai-hero-tool" + (webSearch ? " active" : "")}
                aria-pressed={webSearch}
                onClick={() => setWebSearch((v) => !v)}
              >
                <Globe size={15} /> 联网搜索
              </button>
              <button
                type="button"
                className={"ai-hero-tool" + (think ? " active" : "")}
                aria-pressed={think}
                onClick={() => setThink((v) => !v)}
              >
                <Brain size={15} /> 深度思考
              </button>
              <span className="ai-hero-context">
                {contextLabel ? `上下文：${contextLabel}` : "上下文：General"}
              </span>
              <button
                type="button"
                className="ai-hero-submit"
                onClick={submit}
                disabled={!value.trim()}
                aria-label="发送"
              >
                <PaperPlaneTilt size={18} weight="fill" />
              </button>
            </div>
          </div>
        )}
      </div>

      <aside className="ai-hero-assist" aria-label="AI 助手建议">
        <span className="ai-hero-assist-head">
          AI Assistant
          <button
            type="button"
            className="ai-hero-assist-close"
            aria-label="收起建议"
            onClick={() => setExpanded(false)}
          >
            <X size={14} />
          </button>
        </span>
        {AI_SUGGESTIONS.map((item) => (
          <button
            key={item.label}
            type="button"
            className="ai-hero-assist-item"
            onClick={() => {
              setValue(item.label + "：");
              setExpanded(true);
            }}
          >
            <span className="ai-hero-assist-icon">{item.icon}</span>
            <span>
              <strong>{item.label}</strong>
              <small>{item.desc}</small>
            </span>
          </button>
        ))}
      </aside>
    </section>
  );
}

export function AIBubbleLauncher({ onOpen }: { onOpen: () => void }) {
  return (
    <button
      type="button"
      className="ai-bubble-launch"
      onClick={onOpen}
      aria-label="Ask AI"
      title="Ask AI (⌘/)"
    >
      <Sparkle size={18} weight="fill" />
      <span>Ask AI</span>
    </button>
  );
}
