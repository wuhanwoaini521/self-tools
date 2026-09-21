/**
 * History「AI 解读」区（V5 §27/§33/§34）。
 *
 * - Canonical 立即渲染（这里只读富化状态，不阻塞页面）。
 * - READY/REVIEWED：展示内容 + 来源 (N) 可展开 + 生成时间 + 重新整理/审定。
 * - STALE：旧内容立即显示（stale-while-revalidate）+ 提醒 + 重新整理。
 * - MISSING/FAILED：生成按钮（触发后端单飞管线）。
 * - GENERATING：轮询状态直至结束。
 * - 搜索/模型未配置 → FAILED 状态展示「不可用」，Canonical 一切照常。
 */
import {
  ArrowsClockwise,
  CaretDown,
  CaretRight,
  CheckCircle,
  Sparkle,
  Warning,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { errorMessage, isTauriRuntime } from "../../utils";
import type {
  EnrichmentPayloadDto,
  EnrichmentSectionInfo,
  EnrichmentState,
  EnrichmentViewDto,
} from "../ai/aiTypes";
import { enrichmentClient } from "./enrichmentClient";

const SECTIONS: { key: "overview" | "background" | "impact"; label: string }[] =
  [
    { key: "overview", label: "概述" },
    { key: "background", label: "背景" },
    { key: "impact", label: "影响" },
  ];

const STATE_LABEL: Record<EnrichmentState, string> = {
  MISSING: "未生成",
  GENERATING: "生成中",
  READY: "已生成",
  STALE: "可能过期",
  FAILED: "生成失败",
  REVIEWED: "已审定",
};

export function EnrichmentPanel({
  eventId,
  locale = "zh-CN",
}: {
  eventId: string;
  locale?: string;
}) {
  const [sections, setSections] = useState<EnrichmentSectionInfo[]>([]);
  const [busy, setBusy] = useState<"overview" | "background" | "impact" | null>(
    null,
  );
  const [notice, setNotice] = useState("");
  const timerRef = useRef<number | null>(null);

  const reload = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      setSections(await enrichmentClient.state("event", eventId, locale));
    } catch (cause) {
      setNotice(errorMessage(cause));
    }
  }, [eventId, locale]);

  useEffect(() => {
    void reload();
  }, [reload]);

  // GENERATING 轮询（§27 on-demand flow；§30 单飞在后端）。
  useEffect(() => {
    if (!sections.some((section) => section.state === "GENERATING")) {
      if (timerRef.current !== null) {
        window.clearInterval(timerRef.current);
        timerRef.current = null;
      }
      return;
    }
    if (timerRef.current !== null) return;
    timerRef.current = window.setInterval(() => {
      void reload();
    }, 3000);
    return () => {
      if (timerRef.current !== null) {
        window.clearInterval(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [sections, reload]);

  const run = useCallback(
    async (
      section: "overview" | "background" | "impact",
      mode: "ensure" | "refresh",
    ) => {
      setNotice("");
      setBusy(section);
      try {
        if (mode === "ensure") {
          await enrichmentClient.ensure("event", eventId, section, locale);
        } else {
          await enrichmentClient.refresh("event", eventId, section, locale);
        }
        await reload();
      } catch (cause) {
        setNotice(errorMessage(cause));
      } finally {
        setBusy(null);
      }
    },
    [eventId, locale, reload],
  );

  const review = useCallback(
    async (section: "overview" | "background" | "impact") => {
      setNotice("");
      try {
        await enrichmentClient.review("event", eventId, section, locale);
        await reload();
      } catch (cause) {
        setNotice(errorMessage(cause));
      }
    },
    [eventId, locale, reload],
  );

  return (
    <section className="history-enrichment">
      <header className="history-enrichment-head">
        <Sparkle size={14} weight="fill" />
        <strong>AI 解读</strong>
        <span className="history-enrichment-note">
          按需联网检索生成 · 仅作补充，不替代史料核对
        </span>
      </header>
      {notice ? <p className="history-enrichment-notice">{notice}</p> : null}
      <div className="history-enrichment-sections">
        {SECTIONS.map(({ key, label }) => {
          const info = sections.find((section) => section.section === key);
          const state = info?.state ?? "MISSING";
          return (
            <EnrichmentRow
              key={key}
              section={key}
              label={label}
              eventId={eventId}
              locale={locale}
              state={state}
              busy={busy === key}
              onGenerate={() => void run(key, "ensure")}
              onRefresh={() => void run(key, "refresh")}
              onReview={() => void review(key)}
            />
          );
        })}
      </div>
    </section>
  );
}

function EnrichmentRow({
  section,
  label,
  eventId,
  locale,
  state,
  busy,
  onGenerate,
  onRefresh,
  onReview,
}: {
  section: "overview" | "background" | "impact";
  label: string;
  eventId: string;
  locale: string;
  state: EnrichmentState;
  busy: boolean;
  onGenerate: () => void;
  onRefresh: () => void;
  onReview: () => void;
}) {
  const [view, setView] = useState<EnrichmentViewDto | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [expandedSources, setExpandedSources] = useState(false);

  useEffect(() => {
    if (state === "MISSING") {
      setView(null);
      setLoading(false);
      return;
    }
    if (!isTauriRuntime()) return;
    let cancelled = false;
    setLoading(true);
    void enrichmentClient
      .view("event", eventId, section, locale)
      .then((result) => {
        if (!cancelled) setView(result);
      })
      .catch(() => {
        if (!cancelled) setView(null);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [state, section, eventId, locale]);

  if (state === "MISSING" || (state === "FAILED" && !view)) {
    return (
      <div className="history-enrichment-row">
        <span className="history-enrichment-label">{label}</span>
        <span className={`history-enrichment-state ${state.toLowerCase()}`}>
          {STATE_LABEL[state]}
        </span>
        <button
          type="button"
          className="history-enrichment-action"
          disabled={busy || !isTauriRuntime()}
          onClick={onGenerate}
        >
          {busy ? (
            <ArrowsClockwise size={13} className="spin" />
          ) : (
            <Sparkle size={13} />
          )}
          {busy ? "生成中…" : "生成 AI 解读"}
        </button>
      </div>
    );
  }
  if (loading || !view) {
    return (
      <div className="history-enrichment-row">
        <span className="history-enrichment-label">{label}</span>
        <span className="history-enrichment-state generating">读取中…</span>
      </div>
    );
  }
  const payload: EnrichmentPayloadDto | null = view.payload;
  const sourceCount = payload
    ? payload.claims.reduce(
        (total, claim) => total + claim.source_ids.length,
        0,
      )
    : 0;
  return (
    <div
      className={`history-enrichment-row filled ${view.state.toLowerCase()}`}
    >
      <div className="history-enrichment-row-head">
        <span className="history-enrichment-label">{label}</span>
        <span
          className={`history-enrichment-state ${view.state.toLowerCase()}`}
        >
          {STATE_LABEL[view.state]}
        </span>
        <span className="history-enrichment-actions">
          {view.state === "REVIEWED" ? (
            <span className="history-enrichment-reviewed">
              <CheckCircle size={12} weight="fill" /> 已审定
            </span>
          ) : null}
          <button
            type="button"
            title="重新整理"
            onClick={onRefresh}
            disabled={busy}
          >
            <ArrowsClockwise size={13} className={busy ? "spin" : ""} />
          </button>
          <button
            type="button"
            title="标记为已审定（此后不再自动刷新）"
            onClick={onReview}
            disabled={view.state === "REVIEWED"}
          >
            <CheckCircle size={13} />
          </button>
          <button
            type="button"
            title={expanded ? "收起" : "展开"}
            onClick={() => setExpanded((value) => !value)}
          >
            {expanded ? <CaretDown size={13} /> : <CaretRight size={13} />}
          </button>
        </span>
      </div>
      {view.state === "STALE" ? (
        <p className="history-enrichment-stale-hint">
          <Warning size={12} /> 内容可能已过期（Canonical
          或缓存配置变化）——旧内容保持可读，可点击重新整理更新。
        </p>
      ) : null}
      {expanded ? (
        <div className="history-enrichment-body">
          {payload?.content ? (
            <p className="history-enrichment-content">{payload.content}</p>
          ) : null}
          {payload && payload.uncertainties.length > 0 ? (
            <p className="history-enrichment-limit">
              资料局限：{payload.uncertainties.join("；")}
            </p>
          ) : null}
          {payload && payload.controversies.length > 0 ? (
            <p className="history-enrichment-limit">
              争议：{payload.controversies.join("；")}
            </p>
          ) : null}
          {sourceCount > 0 ? (
            <div className="history-enrichment-sources">
              <button
                type="button"
                className="history-enrichment-sources-toggle"
                onClick={() => setExpandedSources((value) => !value)}
              >
                来源 ({sourceCount})
                {expandedSources ? (
                  <CaretDown size={12} />
                ) : (
                  <CaretRight size={12} />
                )}
              </button>
              {expandedSources ? (
                <ul>
                  {uniqueUrls(payload).map((url) => (
                    <li key={url}>
                      <a href={url} target="_blank" rel="noreferrer">
                        {prettyUrl(url)}
                      </a>
                    </li>
                  ))}
                </ul>
              ) : null}
            </div>
          ) : null}
          {view.metadata ? (
            <p className="history-enrichment-meta">
              生成于 {formatTs(view.metadata.generated_at)}
              {view.metadata.provider ? ` · ${view.metadata.provider}` : ""}
              {view.metadata.model ? ` / ${view.metadata.model}` : ""}
            </p>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

// 便于子行读取当前 eventId（已改为 props 传递，不再需要全局引用）。

function uniqueUrls(payload: EnrichmentPayloadDto | null): string[] {
  if (!payload) return [];
  const seen = new Set<string>();
  const urls: string[] = [];
  for (const claim of payload.claims) {
    for (const url of claim.source_ids) {
      if (!seen.has(url)) {
        seen.add(url);
        urls.push(url);
      }
    }
  }
  return urls;
}

function prettyUrl(url: string): string {
  try {
    const parsed = new URL(url);
    return `${parsed.hostname}${parsed.pathname.slice(0, 40)}`;
  } catch {
    return url.slice(0, 60);
  }
}

function formatTs(seconds: number): string {
  const date = new Date(seconds * 1000);
  return date.toLocaleString("zh-CN", {
    dateStyle: "short",
    timeStyle: "short",
  });
}
