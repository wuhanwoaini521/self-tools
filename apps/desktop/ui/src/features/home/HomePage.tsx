import {
  ArrowRight,
  Brain,
  Cards,
  CaretRight,
  FileText,
  Flame,
  FolderOpen,
  FolderSimple,
  Globe,
  HardDrives,
  MapPin,
  Notebook,
  NotePencil,
  Rss,
  Translate,
  TrendUp,
  Wrench,
} from "@phosphor-icons/react";
import { useEffect, useState, type ReactNode } from "react";
import type {
  ArticleDto,
  GeographyHome,
  SemanticHistoryHome,
  TodayDashboardData,
} from "../../types";
import { fileName, formatRelativeTime, greetingByHour } from "../../utils";
import { stripRssHtml } from "../rss/rssContent";
import { learningClient } from "../learning/learningClient";
import { AIBubbleHero } from "../ai/AIBubble";
import {
  SketchBadge,
  SketchButton,
  SketchCard,
  SketchProgress,
  SketchSectionHeader,
  SketchStatCard,
} from "../../components/sketch/SketchKit";
import {
  SketchIllustration,
  type SketchIllustrationName,
} from "../../components/sketch/SketchIllustrations";

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
  onOpenKnowledge?: (tab?: "memory" | "documents" | "files") => void;
  /** V11 Learning OS 导航 */
  onNavigate?: (route: string) => void;
}

const LANGUAGE_LABELS: Record<string, string> = {
  eng: "英语",
  jpn: "日语",
  cmn: "普通话",
  yue: "粤语",
};

const MODULE_ILLUSTRATION: Record<string, SketchIllustrationName> = {
  history: "history",
  geography: "geography",
  language: "language",
  study: "study",
  news: "news",
  documents: "markdown",
  knowledge: "knowledge",
  review: "review",
};

const MODULE_TONE: Record<string, "blue" | "green" | "orange" | "red" | "purple" | "default"> = {
  history: "orange",
  geography: "green",
  language: "blue",
  study: "purple",
  news: "red",
};

function dateLabel(article: ArticleDto | null) {
  if (!article?.published_at) return "今天";
  return new Date(article.published_at * 1000).toLocaleDateString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
  });
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

  const today = todayData ?? platformToday;

  // 跨领域实体智能语义关联
  const allGeoEntities = [
    ...(geographyHome?.featured ?? []),
    ...(geographyHome?.recent ?? []),
  ];
  const matchedGeoEntity = articleText
    ? allGeoEntities.find(
        (item) =>
          (item.name && articleText.includes(item.name.toLowerCase())) ||
          (item.name_en && articleText.includes(item.name_en.toLowerCase())),
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
              articleText.includes(story.summary_zh_cn.toLowerCase()))),
      )
    : null;

  const historyStory =
    matchedHistoryStory ??
    historyStories.find((story) => story.usable !== false) ??
    historyStories[0] ??
    null;
  const isHistoryLinked = Boolean(matchedHistoryStory);

  const languageDueCount = today?.pending_reviews_count ?? 0;
  const language = LANGUAGE_LABELS.jpn ?? "日语";
  const placeName = geoEntity?.name ?? recommendation?.title ?? "从一个地点开始";
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

  const continueItems = (today?.continue_items ?? []).slice(0, 4);
  const exploreItems = (today?.explore_recommendations ?? []).slice(0, 4);

  const handleDeepLink = (deepLink: string) => {
    if (deepLink.startsWith("#")) {
      onNavigate?.(deepLink);
    }
  };

  const openExplore = (module: string, entityType: string, entityId: string) => {
    if (module === "history") {
      if (entityType === "story") handleDeepLink(`#history?story=${entityId}`);
      else if (entityType === "person") handleDeepLink(`#history?person=${entityId}`);
      else handleDeepLink(`#history?event=${entityId}`);
    } else if (module === "geography") {
      handleDeepLink(`#geography?id=${entityId}`);
    } else if (module === "language") {
      handleDeepLink(`#language?id=${entityId}`);
    } else {
      handleDeepLink(`#${module}?id=${entityId}`);
    }
  };

  return (
    <div className="page-scroll home-page home-story-page">
      {/* Greeting + Stats */}
      <header className="home-hero">
        <div className="home-hero-copy">
          <span className="home-hero-kicker">PERSONAL KNOWLEDGE &amp; LEARNING OS</span>
          <h1>
            {hour < 12 ? "早上好" : hour < 18 ? "下午好" : "晚上好"}，整理今天学到的知识并完成复习。
          </h1>
          <p>{greetingByHour(hour)} · 跨历史、地理、语言、研习与新闻的统一学习工作台。</p>
        </div>
        <div className="home-hero-stats">
          <SketchStatCard
            tone="orange"
            icon={<Flame size={18} />}
            value={`连续 ${today?.recent_streak_days ?? 0} 天`}
            label="保持学习"
          />
          <SketchStatCard
            tone="green"
            icon={<TrendUp size={18} />}
            value={`今日已学 ${today?.studied_topics_today ?? 0} 项`}
            label="比昨天多一点"
          />
          <SketchStatCard
            tone="blue"
            icon={<Brain size={18} />}
            value={`平均掌握度 ${Math.round(today?.average_mastery ?? 0)}%`}
            label="持续进步中"
          />
        </div>
      </header>

      {/* AI Bubble */}
      <AIBubbleHero
        onAsk={(prompt) => onAskAi?.(prompt)}
        onOpenFiles={() => onNavigate?.("#knowledge?tab=files")}
        contextLabel={null}
      />

      {/* Continue Learning */}
      <section className="home-block" aria-label="继续学习">
        <SketchSectionHeader
          title="继续学习"
          en="Continue Learning"
          icon={<Notebook size={20} />}
          action={
            <button
              type="button"
              className="home-link-btn"
              onClick={() => onNavigate?.("#review")}
            >
              <Cards size={15} /> 复习中心
            </button>
          }
        />
        {continueItems.length > 0 ? (
          <div className="home-continue-grid">
            {continueItems.map((item, index) => {
              const tone = MODULE_TONE[item.module] ?? "default";
              const progress = Math.round(item.progress_percent ?? 0);
              return (
                <SketchCard
                  key={`${item.module}-${item.entity_id ?? index}`}
                  interactive
                  rotation={index % 3 === 0 ? "a" : index % 3 === 1 ? "b" : "c"}
                  onClick={() => handleDeepLink(item.action_target)}
                  className="home-continue-card"
                >
                  <div className="home-continue-head">
                    <SketchBadge tone={tone}>
                      {item.module} · {item.entity_type}
                    </SketchBadge>
                    <span className="home-continue-illustration" aria-hidden>
                      <SketchIllustration
                        name={MODULE_ILLUSTRATION[item.module] ?? "study"}
                        size={30}
                      />
                    </span>
                  </div>
                  <div className="home-continue-title">{item.title}</div>
                  <div className="home-continue-progress">
                    <SketchProgress
                      value={progress}
                      tone={progress >= 80 ? "green" : tone === "green" ? "green" : "blue"}
                      label={`${item.title} 进度`}
                    />
                    <span>{progress}%</span>
                    <ArrowRight size={14} />
                  </div>
                </SketchCard>
              );
            })}
          </div>
        ) : (
          <SketchCard className="home-empty-card">
            <p className="home-empty">
              还没有进行中的学习项。打开 Language 或 History，开始今天的第一段内容。
            </p>
            <div className="home-empty-actions">
              <SketchButton size="sm" onClick={() => onOpenLanguage()}>
                <Translate size={15} /> 学语言
              </SketchButton>
              <SketchButton size="sm" onClick={() => onOpenHistory()}>
                <Notebook size={15} /> 看历史
              </SketchButton>
            </div>
          </SketchCard>
        )}
      </section>

      {/* Explore */}
      <section className="home-block" aria-label="今日发现">
        <SketchSectionHeader
          title="今日发现"
          en="Explore"
          icon={<Globe size={20} />}
          action={
            <button type="button" className="home-link-btn" onClick={onRefreshRss} disabled={rssRefreshing}>
              <Rss size={15} /> {rssRefreshing ? "刷新中…" : "刷新订阅"}
            </button>
          }
        />
        {exploreItems.length > 0 ? (
          <div className="home-explore-grid">
            {exploreItems.map((exp, index) => (
              <SketchCard
                key={exp.id}
                interactive
                rotation={index % 2 === 0 ? "c" : "b"}
                onClick={() => openExplore(exp.module, exp.entity_type, exp.entity_id)}
                className="home-explore-card"
              >
                <span className="home-explore-illustration" aria-hidden>
                  <SketchIllustration
                    name={MODULE_ILLUSTRATION[exp.module] ?? "knowledge"}
                    size={44}
                  />
                </span>
                <div className="home-explore-copy">
                  <SketchBadge tone={MODULE_TONE[exp.module] ?? "default"}>{exp.reason}</SketchBadge>
                  <strong>{exp.title}</strong>
                  {exp.summary ? <p>{exp.summary}</p> : null}
                </div>
              </SketchCard>
            ))}
          </div>
        ) : (
          <div className="home-explore-grid">
            <SketchCard
              interactive
              rotation="c"
              onClick={() => onOpenHistory(historyStory?.id)}
              className="home-explore-card"
            >
              <span className="home-explore-illustration" aria-hidden>
                <SketchIllustration name="history" size={44} />
              </span>
              <div className="home-explore-copy">
                <SketchBadge tone="orange">历史精选</SketchBadge>
                <strong>{historyTitle}</strong>
                <p>{historySummary}</p>
              </div>
            </SketchCard>
            <SketchCard
              interactive
              rotation="b"
              onClick={() => onOpenGeography(geoEntity?.id)}
              className="home-explore-card"
            >
              <span className="home-explore-illustration" aria-hidden>
                <SketchIllustration name="geography" size={44} />
              </span>
              <div className="home-explore-copy">
                <SketchBadge tone="green">地理百科</SketchBadge>
                <strong>{placeName}</strong>
                <p>{placeSummary}</p>
              </div>
            </SketchCard>
          </div>
        )}
      </section>

      {/* Workspace modules */}
      <section className="home-block" aria-label="工作区">
        <SketchSectionHeader title="Workspace" en="All in one place" icon={<FolderSimple size={20} />} />
        <div className="home-module-grid">
          <ModuleCard
            title="学习系统"
            illustration="review"
            rotation="a"
            links={[
              { label: "复习中心 (SRS)", icon: <Cards size={15} />, onClick: () => onNavigate?.("#review") },
              { label: "知识图谱", icon: <Brain size={15} />, onClick: () => onNavigate?.("#graph") },
              { label: "专题合集", icon: <FolderSimple size={15} />, onClick: () => onNavigate?.("#collections") },
            ]}
          />
          <ModuleCard
            title="探索领域"
            illustration="geography"
            rotation="b"
            links={[
              { label: "历史时空", icon: <Notebook size={15} />, onClick: () => onOpenHistory() },
              { label: "地理百科", icon: <MapPin size={15} />, onClick: () => onOpenGeography() },
              { label: "语言词典", icon: <Translate size={15} />, onClick: () => onOpenLanguage() },
              ...(onOpenStudyBoard
                ? [{ label: "专题研习", icon: <NotePencil size={15} />, onClick: onOpenStudyBoard }]
                : []),
            ]}
          />
          <ModuleCard
            title="我的知识"
            illustration="knowledge"
            rotation="c"
            links={[
              {
                label: "Memory",
                icon: <Brain size={15} />,
                onClick: () => onNavigate?.("#knowledge?tab=memory"),
              },
              {
                label: "Documents",
                icon: <FileText size={15} />,
                onClick: () => onNavigate?.("#knowledge?tab=documents"),
              },
              {
                label: "Files",
                icon: <FolderOpen size={15} />,
                onClick: () => {
                  if (onOpenKnowledge) onOpenKnowledge("files");
                  else onNavigate?.("#knowledge?tab=files");
                },
              },
            ]}
          />
          <ModuleCard
            title="家庭系统"
            illustration="server"
            rotation="b"
            links={[
              { label: "Server", icon: <HardDrives size={15} />, onClick: () => onOpenServer?.() },
              { label: "Applications", icon: <Wrench size={15} />, onClick: () => onOpenServer?.() },
            ]}
          />
        </div>
      </section>

      {/* Reading & cross-domain */}
      <section className="home-block home-cross-grid" aria-label="阅读与跨领域">
        <SketchCard className="home-article-panel">
          <header className="home-article-meta">
            <SketchBadge tone="red">
              <Rss size={12} /> {article?.feed_title ?? "RSS"}
            </SketchBadge>
            <span>{article ? dateLabel(article) : "等待第一篇文章"}</span>
          </header>
          <h2 className="home-article-title">
            {article?.title ?? "从一篇文章，开始一次跨领域探索"}
          </h2>
          <p className="home-article-lead">{articleLead}</p>
          <div className="home-article-actions">
            <SketchButton
              variant="primary"
              onClick={() => article && onOpenArticle(article)}
              disabled={!article}
            >
              阅读全文 <ArrowRight size={15} />
            </SketchButton>
            <SketchButton onClick={onNewNote}>
              <NotePencil size={15} /> 写笔记
            </SketchButton>
          </div>
          <div className="home-article-footer">
            <span>
              {article
                ? `来源：${article.feed_title} · ${formatRelativeTime(article.published_at)}`
                : "来源：RSS Reader"}
            </span>
          </div>
        </SketchCard>

        <div className="home-cross-side">
          <SketchCard interactive rotation="a" onClick={() => onOpenGeography(geoEntity?.id)}>
            <div className="home-cross-head">
              <SketchBadge tone="green">
                <MapPin size={12} /> {isGeoLinked ? "本文提及地点" : "继续探索地点"}
              </SketchBadge>
              <CaretRight size={14} />
            </div>
            <strong className="home-cross-title">{placeName}</strong>
            <p className="home-cross-desc">{placeSummary}</p>
          </SketchCard>

          <SketchCard interactive rotation="b" onClick={() => onOpenHistory(historyStory?.id)}>
            <div className="home-cross-head">
              <SketchBadge tone="orange">
                <Notebook size={12} /> {isHistoryLinked ? "关联历史溯源" : "历史推荐"}
              </SketchBadge>
              <CaretRight size={14} />
            </div>
            <strong className="home-cross-title">{historyTitle}</strong>
            <p className="home-cross-desc">{historySummary}</p>
          </SketchCard>

          <SketchCard interactive rotation="c" onClick={() => onOpenLanguage()}>
            <div className="home-cross-head">
              <SketchBadge tone="blue">
                <Translate size={12} /> 语言学习 · {language}
              </SketchBadge>
              <CaretRight size={14} />
            </div>
            <strong className="home-cross-title">今天待复习 {languageDueCount} 条</strong>
            <p className="home-cross-desc">
              {languageDueCount > 0
                ? "先从今天到期的复习开始；答错的会自动进入错题本。"
                : "暂时没有到期的复习，可以学一课新内容。"}
            </p>
          </SketchCard>
        </div>
      </section>

      {/* Recent activity */}
      <section className="home-activity-grid">
        <SketchCard className="home-activity-section">
          <SketchSectionHeader
            title="最近笔记"
            en="Notes"
            action={
              <button type="button" className="home-link-btn" onClick={onNewNote}>
                新建 <ArrowRight size={13} />
              </button>
            }
          />
          {recentFiles.length === 0 ? (
            <p className="home-empty">还没有编辑记录，从一篇文章开始写下你的理解。</p>
          ) : (
            <ul className="home-mini-list">
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
        </SketchCard>

        <SketchCard className="home-activity-section">
          <SketchSectionHeader title="最近探索" en="Continue Exploring" />
          <ul className="home-mini-list">
            {geoEntity ? (
              <li>
                <button type="button" onClick={() => onOpenGeography(geoEntity.id)}>
                  <MapPin size={16} />
                  <span>{geoEntity.name}</span>
                  <small>Geography · {geoEntity.entity_type}</small>
                  <ArrowRight size={14} />
                </button>
              </li>
            ) : null}
            {historyStory ? (
              <li>
                <button type="button" onClick={() => onOpenHistory(historyStory.id)}>
                  <Notebook size={16} />
                  <span>{historyStory.title_zh_cn}</span>
                  <small>History · Story</small>
                  <ArrowRight size={14} />
                </button>
              </li>
            ) : null}
            {!geoEntity && !historyStory ? (
              <li>
                <button type="button" onClick={() => onOpenGeography()}>
                  <MapPin size={16} />
                  <span>打开 Geography 开始探索</span>
                  <small>沿着地点与关系继续</small>
                  <ArrowRight size={14} />
                </button>
              </li>
            ) : null}
          </ul>
        </SketchCard>
      </section>
    </div>
  );
}

function ModuleCard({
  title,
  links,
  illustration,
  rotation,
}: {
  title: string;
  links: Array<{ label: string; icon?: ReactNode; onClick: () => void }>;
  illustration: SketchIllustrationName;
  rotation: "a" | "b" | "c";
}) {
  return (
    <article className={`sketch-module sketch-rot-${rotation}`}>
      <div className="sketch-module-illustration">
        <SketchIllustration name={illustration} size={72} />
      </div>
      <h3 className="sketch-module-title">{title}</h3>
      <div className="sketch-module-links">
        {links.map((link) => (
          <button key={link.label} type="button" className="sketch-module-link" onClick={link.onClick}>
            {link.icon}
            <span>{link.label}</span>
            <span className="sketch-module-arrow" aria-hidden>
              →
            </span>
          </button>
        ))}
      </div>
    </article>
  );
}
