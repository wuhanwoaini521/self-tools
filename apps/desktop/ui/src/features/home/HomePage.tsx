import React, { useEffect, useState } from "react";
import {
  ArrowRight,
  BookOpen,
  Brain,
  Cards,
  CheckCircle,
  Clock,
  FileText,
  Flame,
  FolderOpen,
  FolderSimple,
  Globe,
  HardDrives,
  Lightbulb,
  MapPin,
  Note,
  NotePencil,
  Rss,
  Sparkle,
  Translate,
  Wrench,
} from "@phosphor-icons/react";
import type { ReactNode } from "react";
import type {
  ArticleDto,
  GeographyHome,
  SemanticHistoryHome,
  TodayDashboardData,
} from "../../types";
import { fileName, formatRelativeTime, greetingByHour } from "../../utils";
import { stripRssHtml } from "../rss/rssContent";
import { learningClient } from "../learning/learningClient";

interface HomePageProps {
  recentFiles: string[];
  latestArticles: ArticleDto[];
  geographyHome: GeographyHome | null;
  historyHome: SemanticHistoryHome | null;
  /** 平台 Today 聚合（含各模块待复习数）。Language 不再自建第二套。 */
  platformToday: TodayDashboardData | null;
  rssRefreshing: boolean;
  onOpenNote: (path: string) => void;
  onOpenArticle: (article: ArticleDto) => void;
  onOpenGeography: (id?: string) => void;
  onOpenHistory: (id?: string) => void;
  onOpenLanguage: (id?: string) => void;
  onNewNote: () => void;
  onRefreshRss: () => void;
  /** V11-J：打开全局 Ask AI（Personal Hub 唯一 AI 入口）。 */
  onAskAi?: (prompt?: string) => void;
  /** V11-J：打开家庭服务器面板。 */
  onOpenServer?: () => void;
  /** V11-J：打开学习板。 */
  onOpenStudyBoard?: () => void;
  /** V11-J：打开知识层（Memory/Documents/Files）。 */
  onOpenKnowledge?: () => void;
  /** V11 Learning OS 导航 */
  onNavigate?: (route: string) => void;
}

const LANGUAGE_LABELS: Record<string, string> = {
  eng: "英语",
  jpn: "日语",
  cmn: "普通话",
  yue: "粤语",
};

const MODULE_ICONS: Record<string, React.ReactNode> = {
  history: <Clock size={16} color="#f59e0b" />,
  geography: <Globe size={16} color="#10b981" />,
  language: <Translate size={16} color="#3b82f6" />,
  study: <BookOpen size={16} color="#8b5cf6" />,
  news: <Lightbulb size={16} color="#f43f5e" />,
  documents: <FileText size={16} color="#06b6d4" />,
};

function dateLabel(article: ArticleDto | null) {
  if (!article?.published_at) return "今天";
  return new Date(article.published_at * 1000).toLocaleDateString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
  });
}

function HomeSectionHeading({
  eyebrow,
  title,
  action,
}: {
  eyebrow: string;
  title: string;
  action?: ReactNode;
}) {
  return (
    <header className="home-section-heading">
      <div>
        <span>{eyebrow}</span>
        <h2>{title}</h2>
      </div>
      {action}
    </header>
  );
}

export function HomePage({
  recentFiles,
  latestArticles,
  geographyHome,
  historyHome,
  platformToday,
  rssRefreshing,
  onOpenNote,
  onOpenArticle,
  onOpenGeography,
  onOpenHistory,
  onOpenLanguage,
  onNewNote,
  onRefreshRss,
  onAskAi,
  onOpenServer,
  onOpenStudyBoard,
  onOpenKnowledge,
  onNavigate,
}: HomePageProps) {
  const hour = new Date().getHours();
  const article = latestArticles[0] ?? null;
  const articleText = (
    (article?.title ?? "") +
    " " +
    (article?.summary ?? "")
  ).toLowerCase();

  const [todayData, setTodayData] = useState<TodayDashboardData | null>(null);

  useEffect(() => {
    learningClient
      .getToday()
      .then((data) => setTodayData(data))
      .catch((err) => console.error("Failed to load today dashboard data:", err));
  }, []);

  // 跨领域实体智能语义关联
  const allGeoEntities = [
    ...(geographyHome?.featured ?? []),
    ...(geographyHome?.recent ?? []),
  ];
  const matchedGeoEntity = articleText
    ? allGeoEntities.find(
        (item) =>
          (item.name && articleText.includes(item.name.toLowerCase())) ||
          (item.name_en && articleText.includes(item.name_en.toLowerCase()))
      )
    : null;

  const recommendation = geographyHome?.recommendation ?? null;
  const geoEntity =
    matchedGeoEntity ??
    allGeoEntities.find((item) => item.id === recommendation?.entity_id) ??
    allGeoEntities[0] ??
    null;
  const isGeoLinked = Boolean(matchedGeoEntity);

  const historyStories = historyHome?.stories ?? [];
  const matchedHistoryStory = articleText
    ? historyStories.find(
        (story) =>
          story.usable !== false &&
          ((story.title_zh_cn &&
            articleText.includes(story.title_zh_cn.toLowerCase())) ||
            (story.summary_zh_cn &&
              articleText.includes(story.summary_zh_cn.toLowerCase())))
      )
    : null;

  const historyStory =
    matchedHistoryStory ??
    historyStories.find((story) => story.usable !== false) ??
    historyStories[0] ??
    null;
  const isHistoryLinked = Boolean(matchedHistoryStory);

  // Language 在首页只展示「今天要复习多少」——那是平台 Today 的数字。
  const languageDueCount = platformToday?.pending_reviews_count ?? 0;
  const language = LANGUAGE_LABELS.jpn ?? "日语";
  const placeName =
    geoEntity?.name ?? recommendation?.title ?? "从一个地点开始";
  const placeEnglish = geoEntity?.name_en ?? "A place to explore";
  const placeSummary =
    geoEntity?.summary ??
    recommendation?.question ??
    "打开 Geography，沿着地点、地形与关系继续探索。";
  const historyTitle = historyStory?.title_zh_cn ?? "从一段历史推荐开始";
  const historySummary =
    historyStory?.summary_zh_cn ??
    "打开 History，沿着时间、人物与事件理解内容背后的来路。";
  const articleLead = article?.summary
    ? stripRssHtml(article.summary, article.url)
    : "首页会把最新订阅和地理、历史、语言学习线索放在一起，帮助你从阅读自然地走向理解。";

  const handleDeepLink = (deepLink: string) => {
    if (deepLink.startsWith("#")) {
      onNavigate?.(deepLink);
    }
  };

  return (
    <div className="page-scroll home-page home-story-page">
      {/* Top Header */}
      <header className="home-story-header">
        <div>
          <div className="home-story-kicker">
            <BookOpen size={18} weight="duotone" />
            <span>PERSONAL KNOWLEDGE & LEARNING OS</span>
          </div>
          <h1>{todayData?.greeting || "你好，开启今天的知识探索"}</h1>
          <p>
            {greetingByHour(hour)} · 跨历史、地理、语言、研习与新闻的统一学习工作台。
          </p>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          {todayData && (
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 12,
                background: "var(--surface-primary, #ffffff)",
                padding: "8px 16px",
                borderRadius: 12,
                border: "1px solid var(--border-color, #e5e7eb)",
              }}
            >
              <span
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 4,
                  fontSize: 13,
                  fontWeight: 700,
                  color: "#ea580c",
                }}
              >
                <Flame size={16} weight="fill" /> 连续 {todayData.recent_streak_days ?? 0} 天
              </span>
              <span style={{ color: "var(--border-subtle, #e5e7eb)" }}>|</span>
              <span style={{ fontSize: 13, color: "var(--text-secondary, #4b5563)", fontWeight: 500 }}>
                今日已学 {todayData.studied_topics_today ?? 0} 项
              </span>
              <span style={{ color: "var(--border-subtle, #e5e7eb)" }}>|</span>
              <span style={{ fontSize: 13, color: "#10b981", fontWeight: 700 }}>
                平均掌握度 {Math.round(todayData.average_mastery ?? 0)}%
              </span>
            </div>
          )}
        </div>
      </header>

      {/* Review Queue Notification Banner if items are due */}
      {todayData && (todayData.review_stats?.due_count ?? todayData.pending_reviews_count ?? 0) > 0 && (
        <div
          style={{
            margin: "0 0 20px",
            padding: "16px 20px",
            borderRadius: 12,
            background: "linear-gradient(135deg, rgba(37, 99, 235, 0.08), rgba(139, 92, 246, 0.08))",
            border: "1px solid rgba(37, 99, 235, 0.2)",
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <div
              style={{
                width: 36,
                height: 36,
                borderRadius: "50%",
                background: "#2563eb",
                color: "#ffffff",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <Cards size={20} />
            </div>
            <div>
              <div style={{ fontSize: 14, fontWeight: 700, color: "var(--text-primary, #111827)" }}>
                今日有 {todayData.review_stats?.due_count ?? todayData.pending_reviews_count ?? 0} 张卡片待复习
              </div>
              <div style={{ fontSize: 12, color: "var(--text-secondary, #6b7280)", marginTop: 2 }}>
                根据间隔重复记忆曲线，适时复习可最大化巩固记忆。
              </div>
            </div>
          </div>
          <button
            onClick={() => onNavigate?.("#review")}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 6,
              padding: "8px 16px",
              borderRadius: 8,
              border: "none",
              background: "#2563eb",
              color: "#ffffff",
              fontSize: 13,
              fontWeight: 600,
              cursor: "pointer",
            }}
          >
            进入复习中心 <ArrowRight size={14} />
          </button>
        </div>
      )}

      {/* Personal Hub: Ask AI & Quick Actions */}
      <section className="home-hub" aria-label="Personal Hub">
        <div className="home-hub-ask">
          <Sparkle size={18} weight="fill" />
          <button
            type="button"
            className="home-hub-ask-input"
            onClick={() => onAskAi?.()}
          >
            <span>问 AI：任何关于你知识、学习或家庭服务器的问题</span>
          </button>
          <button
            type="button"
            className="home-hub-ask-go"
            onClick={() => onAskAi?.()}
          >
            提问 <ArrowRight size={15} />
          </button>
        </div>

        {/* Continue Learning Cards */}
        {todayData && (todayData.continue_items?.length ?? 0) > 0 ? (
          <div style={{ marginTop: 20 }}>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
              <div style={{ fontSize: 14, fontWeight: 700, color: "var(--text-primary, #111827)" }}>
                继续学习 (Continue)
              </div>
              <button
                onClick={() => onNavigate?.("#graph")}
                style={{
                  fontSize: 12,
                  color: "#2563eb",
                  background: "none",
                  border: "none",
                  cursor: "pointer",
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 4,
                  fontWeight: 600,
                }}
              >
                <Brain size={14} /> 查看知识图谱
              </button>
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))", gap: 12 }}>
              {(todayData.continue_items ?? []).slice(0, 4).map((item, idx) => (
                <div
                  key={idx}
                  onClick={() => handleDeepLink(item.action_target)}
                  style={{
                    background: "var(--surface-primary, #ffffff)",
                    border: "1px solid var(--border-color, #e5e7eb)",
                    borderRadius: 10,
                    padding: "12px 14px",
                    cursor: "pointer",
                    transition: "transform 0.15s ease, box-shadow 0.15s ease",
                  }}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 6 }}>
                    {MODULE_ICONS[item.module] ?? <BookOpen size={14} />}
                    <span style={{ fontSize: 11, fontWeight: 700, textTransform: "uppercase", color: "#9ca3af" }}>
                      {item.module} · {item.entity_type}
                    </span>
                  </div>
                  <div style={{ fontSize: 14, fontWeight: 600, color: "var(--text-primary, #111827)", marginBottom: 8 }}>
                    {item.title}
                  </div>
                  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <div style={{ flex: 1, height: 4, background: "#f3f4f6", borderRadius: 2, overflow: "hidden" }}>
                      <div
                        style={{
                          height: "100%",
                          width: `${item.progress_percent ?? 0}%`,
                          background: (item.progress_percent ?? 0) >= 80 ? "#10b981" : "#3b82f6",
                        }}
                      />
                    </div>
                    <span style={{ fontSize: 11, color: "#6b7280", fontWeight: 600 }}>
                      {Math.round(item.progress_percent ?? 0)}%
                    </span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        ) : null}

        {/* Explore Recommendations */}
        {todayData && (todayData.explore_recommendations?.length ?? 0) > 0 ? (
          <div style={{ marginTop: 20 }}>
            <div style={{ fontSize: 14, fontWeight: 700, color: "var(--text-primary, #111827)", marginBottom: 12 }}>
              今日发现 (Explore)
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))", gap: 12 }}>
              {(todayData.explore_recommendations ?? []).slice(0, 3).map((exp) => (
                <div
                  key={exp.id}
                  onClick={() => {
                    if (exp.module === "history") {
                      if (exp.entity_type === "story") handleDeepLink(`#history?story=${exp.entity_id}`);
                      else if (exp.entity_type === "person") handleDeepLink(`#history?person=${exp.entity_id}`);
                      else handleDeepLink(`#history?event=${exp.entity_id}`);
                    } else if (exp.module === "geography") {
                      handleDeepLink(`#geography?id=${exp.entity_id}`);
                    } else if (exp.module === "language") {
                      handleDeepLink(`#language?id=${exp.entity_id}`);
                    } else {
                      handleDeepLink(`#${exp.module}?id=${exp.entity_id}`);
                    }
                  }}
                  style={{
                    background: "var(--surface-primary, #ffffff)",
                    border: "1px solid var(--border-color, #e5e7eb)",
                    borderRadius: 10,
                    padding: "14px",
                    cursor: "pointer",
                  }}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 6 }}>
                    {MODULE_ICONS[exp.module] ?? <Lightbulb size={14} />}
                    <span style={{ fontSize: 11, fontWeight: 700, color: "#2563eb" }}>
                      {exp.reason}
                    </span>
                  </div>
                  <div style={{ fontSize: 14, fontWeight: 600, color: "var(--text-primary, #111827)", marginBottom: 6 }}>
                    {exp.title}
                  </div>
                  {exp.summary && (
                    <p style={{ fontSize: 12, color: "var(--text-secondary, #6b7280)", margin: 0, lineClamp: 2 }}>
                      {exp.summary}
                    </p>
                  )}
                </div>
              ))}
            </div>
          </div>
        ) : null}

        {/* Hub Group Quick Shortcuts */}
        <div className="home-hub-groups" style={{ marginTop: 20 }}>
          <section className="home-hub-group">
            <h3>学习系统</h3>
            <ul>
              <li>
                <button type="button" onClick={() => onNavigate?.("#review")}>
                  <Cards size={15} />
                  <span>复习中心 (SRS)</span>
                </button>
              </li>
              <li>
                <button type="button" onClick={() => onNavigate?.("#graph")}>
                  <Brain size={15} />
                  <span>知识图谱</span>
                </button>
              </li>
              <li>
                <button type="button" onClick={() => onNavigate?.("#collections")}>
                  <FolderSimple size={15} />
                  <span>专题合集</span>
                </button>
              </li>
            </ul>
          </section>

          <section className="home-hub-group">
            <h3>探索领域</h3>
            <ul>
              <li>
                <button type="button" onClick={() => onOpenHistory()}>
                  <Clock size={15} />
                  <span>历史时空</span>
                </button>
              </li>
              <li>
                <button type="button" onClick={() => onOpenGeography()}>
                  <MapPin size={15} />
                  <span>地理百科</span>
                </button>
              </li>
              <li>
                <button type="button" onClick={() => onOpenLanguage()}>
                  <Translate size={15} />
                  <span>语言词典</span>
                </button>
              </li>
              {onOpenStudyBoard ? (
                <li>
                  <button type="button" onClick={onOpenStudyBoard}>
                    <NotePencil size={15} />
                    <span>专题研习</span>
                  </button>
                </li>
              ) : null}
            </ul>
          </section>

          <section className="home-hub-group">
            <h3>我的知识</h3>
            <ul>
              <li>
                <button type="button" onClick={() => onNavigate?.("#knowledge?tab=memory")}>
                  <Brain size={15} />
                  <span>Memory</span>
                </button>
              </li>
              <li>
                <button type="button" onClick={() => onNavigate?.("#knowledge?tab=documents")}>
                  <FileText size={15} />
                  <span>Documents</span>
                </button>
              </li>
              <li>
                <button type="button" onClick={() => onNavigate?.("#knowledge?tab=files")}>
                  <FolderOpen size={15} />
                  <span>Files</span>
                </button>
              </li>
            </ul>
          </section>

          <section className="home-hub-group">
            <h3>家庭系统</h3>
            <ul>
              <li>
                <button type="button" onClick={() => onOpenServer?.()}>
                  <HardDrives size={15} />
                  <span>Server</span>
                </button>
              </li>
              <li>
                <button type="button" onClick={() => onOpenServer?.()}>
                  <Wrench size={15} />
                  <span>Applications</span>
                </button>
              </li>
            </ul>
          </section>
        </div>
      </section>

      {/* Story & Context Section */}
      <main className="home-story-layout">
        <section className="home-story-column">
          <article className="home-article-panel">
            <header className="home-article-meta">
              <button
                type="button"
                className="home-back-link"
                onClick={onRefreshRss}
                disabled={rssRefreshing}
              >
                <Rss size={16} />
                <span>{rssRefreshing ? "正在刷新订阅" : "回到订阅源"}</span>
              </button>
              <span>
                {article
                  ? `${article.feed_title} · ${dateLabel(article)}`
                  : "RSS · 等待第一篇文章"}
              </span>
            </header>
            <div className="home-article-content">
              <div className="home-article-copy">
                <h2>
                  {article?.title ?? "从一篇文章，开始一次跨领域探索"}
                </h2>
                <p className="home-article-lead">{articleLead}</p>
                <p className="home-article-body">
                  {article
                    ? "先读懂这篇文章，再沿着页面提供的地点与历史入口继续展开；每一个入口都保留回到原文的路径。"
                    : "添加 RSS Feed 后，这里会展示最新文章，并自动提供可验证的继续探索入口。"}
                </p>
                <div className="home-article-actions">
                  <button
                    type="button"
                    className="home-primary-link"
                    onClick={() => article && onOpenArticle(article)}
                    disabled={!article}
                  >
                    阅读全文 <ArrowRight size={16} />
                  </button>
                  <span className="home-article-source">
                    {article
                      ? `来源：${article.feed_title} · ${formatRelativeTime(article.published_at)}`
                      : "来源：RSS Reader"}
                  </span>
                </div>
              </div>
              <section className="home-place-card">
                <div className="home-place-copy">
                  {isGeoLinked ? (
                    <span className="home-association-tag">
                      <MapPin size={12} weight="fill" /> 本文提及地点
                    </span>
                  ) : (
                    <span>继续探索地点</span>
                  )}
                  <strong>{placeName}</strong>
                  <small>{placeEnglish}</small>
                  <button
                    type="button"
                    onClick={() => onOpenGeography(geoEntity?.id ?? recommendation?.entity_id)}
                  >
                    {placeSummary} <ArrowRight size={15} />
                  </button>
                </div>
              </section>
            </div>
            <footer className="home-article-footer">
              <button
                type="button"
                onClick={() => article && onOpenArticle(article)}
                disabled={!article}
              >
                <Note size={17} />稍后读
              </button>
              <button
                type="button"
                onClick={() => onOpenHistory(historyStory?.id)}
              >
                <BookOpen size={17} />查看关联知识
              </button>
              <button
                type="button"
                className="home-note-action"
                onClick={onNewNote}
              >
                <NotePencil size={17} />写笔记
              </button>
            </footer>
          </article>

          <section className="home-history-context">
            <div className="home-context-icon">
              <Clock size={21} />
            </div>
            <div className="home-context-copy">
              <div>
                {isHistoryLinked ? (
                  <span className="home-association-tag">
                    <Clock size={12} weight="fill" /> 关联历史溯源
                  </span>
                ) : (
                  <span>历史推荐</span>
                )}
                <small>
                  {historyStory ? "来自本地语义资料库" : "等待历史资料"}
                </small>
              </div>
              <strong>{historyTitle}</strong>
              <p>{historySummary}</p>
            </div>
            <button
              type="button"
              onClick={() => onOpenHistory(historyStory?.id)}
            >
              了解更多历史 <ArrowRight size={16} />
            </button>
          </section>
        </section>

        <aside className="home-language-panel">
          <header>
            <div>
              <Translate size={18} />
              <h2>语言学习</h2>
            </div>
            <span>{language}</span>
          </header>
          <div className="home-language-inner">
            <span className="home-language-label">今天待复习</span>
            <strong>{languageDueCount}</strong>
            <small>
              {languageDueCount > 0
                ? "条到期 · 先把记住的再确认一遍"
                : "条 · 暂时没有到期的复习"}
            </small>
            <div className="home-language-divider" />
            <div className="home-language-example">
              <span>怎么开始</span>
              <p>
                {languageDueCount > 0
                  ? "打开 Language，从今天到期的复习开始；答错的会自动进入错题本。"
                  : "打开 Language 学一课新内容，到复习时间它会自动出现在这里。"}
              </p>
            </div>
            <button
              type="button"
              className="home-language-practice"
              onClick={() => onOpenLanguage()}
            >
              开始学习 <ArrowRight size={17} />
            </button>
          </div>
          <button
            type="button"
            className="home-language-more"
            onClick={() => onOpenLanguage()}
          >
            查看更多学习内容 <ArrowRight size={15} />
          </button>
        </aside>
      </main>

      <section className="home-activity-grid">
        <section className="home-activity-section">
          <HomeSectionHeading
            eyebrow="WORKSPACE"
            title="最近笔记"
            action={
              <button type="button" onClick={onNewNote}>
                新建 <ArrowRight size={14} />
              </button>
            }
          />
          {recentFiles.length === 0 ? (
            <p className="home-empty">
              还没有编辑记录，从一篇文章开始写下你的理解。
            </p>
          ) : (
            <ul>
              {recentFiles.slice(0, 4).map((filePath) => (
                <li key={filePath}>
                  <button type="button" onClick={() => onOpenNote(filePath)}>
                    <FileText size={16} />
                    <span>{fileName(filePath)}</span>
                    <small>{filePath}</small>
                    <ArrowRight size={14} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
        <section className="home-activity-section">
          <HomeSectionHeading
            eyebrow="CONTINUE EXPLORING"
            title="最近探索"
          />
          <ul className="home-exploration-list">
            {geoEntity ? (
              <li>
                <button
                  type="button"
                  onClick={() => onOpenGeography(geoEntity.id)}
                >
                  <MapPin size={16} />
                  <span>{geoEntity.name}</span>
                  <small>Geography · {geoEntity.entity_type}</small>
                  <ArrowRight size={14} />
                </button>
              </li>
            ) : null}
            {historyStory ? (
              <li>
                <button
                  type="button"
                  onClick={() => onOpenHistory(historyStory.id)}
                >
                  <Clock size={16} />
                  <span>{historyStory.title_zh_cn}</span>
                  <small>History · Story</small>
                  <ArrowRight size={14} />
                </button>
              </li>
            ) : null}
            {!geoEntity && !historyStory ? (
              <li>
                <button
                  type="button"
                  onClick={() => onOpenGeography()}
                >
                  <MapPin size={16} />
                  <span>打开 Geography 开始探索</span>
                  <small>沿着地点与关系继续</small>
                  <ArrowRight size={14} />
                </button>
              </li>
            ) : null}
          </ul>
        </section>
      </section>

      <footer className="home-offline-note">
        <CheckCircle size={16} />
        跨模块内容优先使用本地数据；学习进度实时同步至本地 learning.db。
      </footer>
    </div>
  );
}
