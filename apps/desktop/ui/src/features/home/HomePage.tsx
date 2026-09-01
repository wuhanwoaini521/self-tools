import {
  ArrowRight,
  BookOpen,
  CheckCircle,
  Clock,
  FileText,
  MapPin,
  Note,
  NotePencil,
  Rss,
  Translate,
} from "@phosphor-icons/react";
import type { ReactNode } from "react";
import type { ArticleDto, GeographyHome, HistoryHome, ReviewCard, TodayView } from "../../types";
import { fileName, formatRelativeTime, greetingByHour } from "../../utils";
import { stripRssHtml } from "../rss/rssContent";

interface HomePageProps {
  recentFiles: string[];
  latestArticles: ArticleDto[];
  geographyHome: GeographyHome | null;
  historyHome: HistoryHome | null;
  todayView: TodayView | null;
  reviewCard: ReviewCard | null;
  rssRefreshing: boolean;
  onOpenNote: (path: string) => void;
  onOpenArticle: (article: ArticleDto) => void;
  onOpenGeography: (id?: string) => void;
  onOpenHistory: (id?: string) => void;
  onOpenLanguage: (id?: string) => void;
  onNewNote: () => void;
  onRefreshRss: () => void;
}

const LANGUAGE_LABELS: Record<string, string> = { eng: "英语", jpn: "日语", cmn: "普通话", yue: "粤语" };

function dateLabel(article: ArticleDto | null) {
  if (!article?.published_at) return "今天";
  return new Date(article.published_at * 1000).toLocaleDateString("zh-CN", { month: "2-digit", day: "2-digit" });
}

function HomeSectionHeading({ eyebrow, title, action }: { eyebrow: string; title: string; action?: ReactNode }) {
  return <header className="home-section-heading"><div><span>{eyebrow}</span><h2>{title}</h2></div>{action}</header>;
}

export function HomePage({
  recentFiles, latestArticles, geographyHome, historyHome, todayView, reviewCard, rssRefreshing,
  onOpenNote, onOpenArticle, onOpenGeography, onOpenHistory, onOpenLanguage, onNewNote, onRefreshRss,
}: HomePageProps) {
  const hour = new Date().getHours();
  const article = latestArticles[0] ?? null;
  const recommendation = geographyHome?.recommendation ?? null;
  const geoEntity = [...(geographyHome?.featured ?? []), ...(geographyHome?.recent ?? [])]
    .find((item) => item.id === recommendation?.entity_id) ?? null;
  const historyNode = historyHome?.recommendation ?? null;
  const languageItem = reviewCard?.item ?? null;
  const language = LANGUAGE_LABELS[todayView?.language ?? "jpn"] ?? "日语";
  const placeName = geoEntity?.name ?? recommendation?.title ?? "从一个地点开始";
  const placeEnglish = geoEntity?.name_en ?? "A place to explore";
  const placeSummary = geoEntity?.summary ?? recommendation?.question ?? "打开 Geography，沿着地点、地形与关系继续探索。";
  const historyTitle = historyNode?.title ?? "从一段历史推荐开始";
  const historySummary = historyNode?.summary ?? "打开 History，沿着时间、人物与事件理解内容背后的来路。";
  const articleLead = article?.summary
    ? stripRssHtml(article.summary, article.url)
    : "首页会把最新订阅和地理、历史、语言学习线索放在一起，帮助你从阅读自然地走向理解。";

  return <div className="page-scroll home-page home-story-page">
    <header className="home-story-header">
      <div>
        <div className="home-story-kicker"><BookOpen size={18} weight="duotone" /><span>STORY TO STUDY</span></div>
        <h1>从一篇好文章，连接世界与知识</h1>
        <p>{greetingByHour(hour)} · 今天沿着一条内容线索，看看它发生在哪里、从何而来，以及如何继续学习。</p>
      </div>
      <div className="home-story-date"><Clock size={16} />{new Date().toLocaleDateString("zh-CN", { year: "numeric", month: "2-digit", day: "2-digit", weekday: "short" })}</div>
    </header>

    <main className="home-story-layout">
      <section className="home-story-column">
        <article className="home-article-panel">
          <header className="home-article-meta">
            <button type="button" className="home-back-link" onClick={onRefreshRss} disabled={rssRefreshing}><Rss size={16} /><span>{rssRefreshing ? "正在刷新订阅" : "回到订阅源"}</span></button>
            <span>{article ? `${article.feed_title} · ${dateLabel(article)}` : "RSS · 等待第一篇文章"}</span>
          </header>
          <div className="home-article-content">
            <div className="home-article-copy">
              <h2>{article?.title ?? "从一篇文章，开始一次跨领域探索"}</h2>
              <p className="home-article-lead">{articleLead}</p>
              <p className="home-article-body">{article ? "先读懂这篇文章，再沿着页面提供的地点与历史入口继续展开；每一个入口都保留回到原文的路径。" : "添加 RSS Feed 后，这里会展示最新文章，并自动提供可验证的继续探索入口。"}</p>
              <div className="home-article-actions"><button type="button" className="home-primary-link" onClick={() => article && onOpenArticle(article)} disabled={!article}>阅读全文 <ArrowRight size={16} /></button><span className="home-article-source">{article ? `来源：${article.feed_title} · ${formatRelativeTime(article.published_at)}` : "来源：RSS Reader"}</span></div>
            </div>
            <section className="home-place-card">
              <div className="home-place-copy"><span>继续探索地点</span><strong>{placeName}</strong><small>{placeEnglish}</small><button type="button" onClick={() => onOpenGeography(recommendation?.entity_id)}>{placeSummary} <ArrowRight size={15} /></button></div>
            </section>
          </div>
          <footer className="home-article-footer"><button type="button" onClick={() => article && onOpenArticle(article)} disabled={!article}><Note size={17} />稍后读</button><button type="button" onClick={() => onOpenHistory(historyNode?.id)}><BookOpen size={17} />查看关联知识</button><button type="button" className="home-note-action" onClick={onNewNote}><NotePencil size={17} />写笔记</button></footer>
        </article>

        <section className="home-history-context"><div className="home-context-icon"><Clock size={21} /></div><div className="home-context-copy"><div><span>历史推荐</span><small>{historyNode ? "来自本地历史资料库" : "等待历史资料"}</small></div><strong>{historyTitle}</strong><p>{historySummary}</p></div><button type="button" onClick={() => onOpenHistory(historyNode?.id)}>了解更多历史 <ArrowRight size={16} /></button></section>
      </section>

      <aside className="home-language-panel">
        <header><div><Translate size={18} /><h2>语言学习</h2></div><span>{language}</span></header>
        <div className="home-language-inner"><span className="home-language-label">今日学习词汇</span><strong>{languageItem?.text ?? "vision"}</strong>{languageItem?.reading || languageItem?.romanization ? <small>{languageItem.reading ?? languageItem.romanization}</small> : <small>/ˈvɪʒən/</small>}<div className="home-language-divider" /><span className="home-language-label">今日计划</span><p>{todayView?.plan.total ?? 0} 项学习任务 · 复习优先</p><div className="home-language-example"><span>例句</span><p>{languageItem ? "从一个词开始，把文章读得更深一点。" : "The next step starts with a clear vision."}</p></div><button type="button" className="home-language-practice" onClick={() => onOpenLanguage(languageItem?.id)}>开始练习 <ArrowRight size={17} /></button></div>
        <button type="button" className="home-language-more" onClick={() => onOpenLanguage()}>查看更多学习内容 <ArrowRight size={15} /></button>
      </aside>
    </main>

    <section className="home-activity-grid">
      <section className="home-activity-section"><HomeSectionHeading eyebrow="WORKSPACE" title="最近笔记" action={<button type="button" onClick={onNewNote}>新建 <ArrowRight size={14} /></button>} />{recentFiles.length === 0 ? <p className="home-empty">还没有编辑记录，从一篇文章开始写下你的理解。</p> : <ul>{recentFiles.slice(0, 4).map((filePath) => <li key={filePath}><button type="button" onClick={() => onOpenNote(filePath)}><FileText size={16} /><span>{fileName(filePath)}</span><small>{filePath}</small><ArrowRight size={14} /></button></li>)}</ul>}</section>
      <section className="home-activity-section"><HomeSectionHeading eyebrow="CONTINUE EXPLORING" title="最近探索" />{<ul className="home-exploration-list">{geoEntity ? <li><button type="button" onClick={() => onOpenGeography(geoEntity.id)}><MapPin size={16} /><span>{geoEntity.name}</span><small>Geography · {geoEntity.entity_type}</small><ArrowRight size={14} /></button></li> : null}{historyNode ? <li><button type="button" onClick={() => onOpenHistory(historyNode.id)}><Clock size={16} /><span>{historyNode.title}</span><small>History · {historyNode.kind}</small><ArrowRight size={14} /></button></li> : null}{!geoEntity && !historyNode ? <li><button type="button" onClick={() => onOpenGeography()}><MapPin size={16} /><span>打开 Geography 开始探索</span><small>沿着地点与关系继续</small><ArrowRight size={14} /></button></li> : null}</ul>}</section>
    </section>

    <footer className="home-offline-note"><CheckCircle size={16} />跨模块内容优先使用本地数据；RSS 更新后，首页会同步最新阅读入口。</footer>
  </div>;
}
