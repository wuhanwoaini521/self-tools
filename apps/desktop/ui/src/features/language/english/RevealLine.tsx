/**
 * 主动回忆用的「先看英文，再看译文」行。
 *
 * 学习研究发现，先尝试回忆再核对答案（retrieval practice）比同时看到
 * 中英文更能加深记忆；因此默认隐藏译文，点击英文或提示才显示。
 * 这里只做前端展示状态：不写学习记录、不改后端契约。
 */
import { useState } from "react";
import { Eye, EyeSlash, SpeakerHigh } from "@phosphor-icons/react";

export interface RevealLineProps {
  english: string;
  chinese?: string;
  onSpeak?: () => void;
  defaultRevealed?: boolean;
  hint?: string;
  className?: string;
}

export function RevealLine({
  english,
  chinese,
  onSpeak,
  defaultRevealed = false,
  hint = "点击显示译文",
  className,
}: RevealLineProps) {
  const [revealed, setRevealed] = useState(defaultRevealed);
  const hasChinese = Boolean(chinese);

  return (
    <div
      className={`en-reveal${revealed ? " is-revealed" : ""}${
        className ? ` ${className}` : ""
      }`}
    >
      <div className="en-reveal-main">
        <button
          type="button"
          className="en-reveal-en"
          aria-expanded={hasChinese ? revealed : undefined}
          onClick={() => {
            if (hasChinese) setRevealed((value) => !value);
          }}
          title={hasChinese ? (revealed ? "隐藏译文" : "显示译文") : undefined}
        >
          <strong>{english}</strong>
        </button>
        {hasChinese ? (
          revealed ? (
            <button
              type="button"
              className="en-reveal-zh"
              onClick={() => setRevealed(false)}
              aria-label="隐藏译文"
            >
              <span>{chinese}</span>
              <EyeSlash size={13} aria-hidden="true" />
            </button>
          ) : (
            <button
              type="button"
              className="en-reveal-hint"
              onClick={() => setRevealed(true)}
            >
              <Eye size={13} aria-hidden="true" />
              {hint}
            </button>
          )
        ) : null}
      </div>
      {onSpeak ? (
        <button
          type="button"
          className="en-icon-btn en-reveal-speak"
          aria-label={`朗读：${english}`}
          onClick={onSpeak}
        >
          <SpeakerHigh size={16} />
        </button>
      ) : null}
    </div>
  );
}
