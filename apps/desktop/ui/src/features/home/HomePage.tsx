import { ArrowsClockwise, FileText, Note, Rss } from "@phosphor-icons/react";
import type { ArticleDto } from "../../types";
import { fileName, formatRelativeTime, greetingByHour } from "../../utils";

/**
 * Home / Dashboard:个人信息总览。
 * 第一版只做简单展示——最近 Markdown、最新 RSS、快捷入口;数据缺失时保持克制。
 */

interface HomePageProps {
  recentFiles: string[];
  latestArticles: ArticleDto[];
  rssRefreshing: boolean;
  onOpenNote: (path: string) => void;
  onOpenArticle: (article: ArticleDto) => void;
  onNewNote: () => void;
  onRefreshRss: () => void;
}

export function HomePage({ recentFiles, latestArticles, rssRefreshing, onOpenNote, onOpenArticle, onNewNote, onRefreshRss }: HomePageProps) {
  const hour = new Date().getHours();
  return <div className="page-scroll home-page">
    <header className="home-greeting">
      <h1>{greetingByHour(hour)}</h1>
      <p>这是你的个人工作台 —— 笔记、订阅与常用工具都从这里开始。</p>
    </header>
    <div className="home-grid">
      <section className="home-card">
        <header><h2>Quick Actions</h2></header>
        <div className="home-actions">
          <button onClick={onNewNote}><Note size={18} />新建笔记</button>
          <button onClick={onRefreshRss} disabled={rssRefreshing}><ArrowsClockwise size={18} className={rssRefreshing ? "spin" : undefined} />刷新 RSS</button>
        </div>
      </section>
      <section className="home-card">
        <header><h2>Recent Notes</h2><FileText size={16} /></header>
        {recentFiles.length === 0
          ? <p className="home-empty">还没有编辑记录,从 Markdown 模块开始写作吧。</p>
          : <ul>{recentFiles.slice(0, 5).map((filePath) => <li key={filePath}><button title={filePath} onClick={() => onOpenNote(filePath)}><span>{fileName(filePath)}</span><small>{filePath}</small></button></li>)}</ul>}
      </section>
      <section className="home-card">
        <header><h2>Latest RSS</h2><Rss size={16} /></header>
        {latestArticles.length === 0
          ? <p className="home-empty">暂无订阅内容,到 RSS 模块添加一个 Feed。</p>
          : <ul>{latestArticles.slice(0, 6).map((article) => <li key={article.id}><button className={article.is_read ? "read" : ""} onClick={() => onOpenArticle(article)}><span>{article.title}</span><small>{article.feed_title} · {formatRelativeTime(article.published_at)}</small>{!article.is_read && <i className="home-unread-dot" />}</button></li>)}</ul>}
      </section>
    </div>
  </div>;
}
