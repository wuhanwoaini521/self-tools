import {
  ArrowDownRight,
  ArrowRight,
  BookOpenText,
  Brain,
  CalendarDots,
  Cards,
  CheckCircle,
  ClockCounterClockwise,
  Compass,
  FolderSimple,
  MapPin,
  NotePencil,
  Newspaper,
  ArrowClockwise,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import type {
  Collection,
  CollectionItem,
  ContinueItem,
  NewsArticle,
  TodayDashboardData,
  TravelPlan,
} from "../../types";
import { errorMessage, fileName } from "../../utils";
import { learningClient } from "../learning/learningClient";
import { newsClient } from "../news/newsClient";
import { stripRssHtml } from "../rss/rssContent";
import { sourceRoute } from "../learning/sourceNavigation";
import { travelClient } from "../travel/travelClient";

interface HomePageProps {
  active: boolean;
  recentFiles: string[];
  onOpenNote: (path: string) => void;
  onNewNote: () => void;
  onNavigate?: (route: string) => void;
}

type HomeResource<T> =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; data: T; refreshing: boolean; refreshError: string | null };

type SavedReference = { collection: Collection; item: CollectionItem };

function useHomeResource<T>(
  loader: () => Promise<T>,
  active: boolean,
): [HomeResource<T>, () => void] {
  const [resource, setResource] = useState<HomeResource<T>>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (!active) return;
    let current = true;
    setResource((previous) =>
      previous.status === "ready"
        ? { ...previous, refreshing: true, refreshError: null }
        : { status: "loading" },
    );
    void loader().then(
      (data) => {
        if (current) setResource({ status: "ready", data, refreshing: false, refreshError: null });
      },
      (cause: unknown) => {
        if (current) {
          const message = errorMessage(cause);
          setResource((previous) =>
            previous.status === "ready"
              ? { ...previous, refreshing: false, refreshError: message }
              : { status: "error", message },
          );
        }
      },
    );
    return () => {
      current = false;
    };
  }, [active, attempt, loader]);

  return [resource, () => setAttempt((value) => value + 1)];
}

function ResourceState<T>({
  resource,
  retry,
  empty,
  isEmpty,
  children,
}: {
  resource: HomeResource<T>;
  retry: () => void;
  empty: ReactNode;
  isEmpty?: (data: T) => boolean;
  children: (data: T) => ReactNode;
}) {
  if (resource.status === "loading") {
    return <div className="hub-state hub-loading" role="status" aria-live="polite">正在读取你的工作区…</div>;
  }
  if (resource.status === "error") {
    return (
      <div className="hub-state hub-error" role="alert">
        <p>暂时无法读取这部分数据。</p>
        <small>{resource.message}</small>
        <button type="button" onClick={retry}><ArrowClockwise size={14} /> 重试</button>
      </div>
    );
  }
  const content = isEmpty?.(resource.data) ? empty : children(resource.data);
  return (
    <>
      {resource.refreshing ? (
        <div className="hub-refreshing" role="status" aria-live="polite">正在同步最新数据…</div>
      ) : null}
      {resource.refreshError ? (
        <div className="hub-refresh-error" role="alert">
          <span>
            <strong>更新失败，仍显示上次读取的数据。</strong>
            <small>{resource.refreshError}</small>
          </span>
          <button type="button" onClick={retry}><ArrowClockwise size={14} /> 重试</button>
        </div>
      ) : null}
      {content}
    </>
  );
}

function routeForEntity(module: string, entityType: string, entityId: string): string {
  return sourceRoute(module, entityType, entityId) ?? `#${module}?id=${encodeURIComponent(entityId)}`;
}

const HOME_MODULE_LABELS: Record<string, string> = {
  geography: "地理",
  history: "历史",
  language: "语言学习",
  learning: "学习",
  news: "新闻",
  travel: "行程",
};

const HOME_ENTITY_LABELS: Record<string, string> = {
  article: "文章",
  event: "事件",
  lesson: "课程",
  person: "人物",
  place: "地点",
  story: "故事",
  word: "词汇",
};

function homeContentLabel(module: string, entityType: string, fallback: string): string {
  const labels = [HOME_MODULE_LABELS[module], HOME_ENTITY_LABELS[entityType]].filter(
    (label): label is string => Boolean(label),
  );
  return labels.length > 0 ? labels.join(" · ") : fallback;
}

function monthDay(timestamp: number | null | undefined): string {
  if (!timestamp) return "日期未记录";
  return new Date(timestamp * 1000).toLocaleDateString("zh-CN", { month: "short", day: "numeric" });
}

function shortDate(timestamp: number | null | undefined): string {
  if (!timestamp) return "日期未记录";
  return new Date(timestamp * 1000).toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export function travelPlanRoute(plan: TravelPlan): string {
  const { context } = plan;
  const query = new URLSearchParams({ city: context.city, days: String(context.days) });
  if (context.date_range) {
    query.set("date_start", context.date_range.start);
    query.set("date_end", context.date_range.end);
  }
  return `#travel?${query.toString()}`;
}

export function HomePage({
  active,
  recentFiles,
  onOpenNote,
  onNewNote,
  onNavigate,
}: HomePageProps) {
  const todayLoader = useCallback(() => learningClient.getToday(), []);
  const collectionLoader = useCallback(async (): Promise<SavedReference[]> => {
    const collections = await learningClient.listCollections();
    const recent = [...collections]
      .sort((left, right) => right.updated_at - left.updated_at)
      .slice(0, 5);
    const groups = await Promise.all(
      recent.map(async (collection) => ({
        collection,
        items: await learningClient.listCollectionItems(collection.id),
      })),
    );
    return groups
      .flatMap(({ collection, items }) =>
        items.map((item) => ({ collection, item })),
      )
      .sort((left, right) => right.item.added_at - left.item.added_at)
      .slice(0, 5);
  }, []);
  const travelLoader = useCallback(() => travelClient.recentPlans(5), []);
  const newsLoader = useCallback(() => newsClient.headlines("all", null, null, 4), []);

  const [today, retryToday] = useHomeResource(todayLoader, active);
  const [savedReferences, retrySavedReferences] = useHomeResource(collectionLoader, active);
  const [travelPlans, retryTravelPlans] = useHomeResource(travelLoader, active);
  const [headlines, retryHeadlines] = useHomeResource(newsLoader, active);

  const greeting = (() => {
    const hour = new Date().getHours();
    if (hour < 12) return "早上好";
    if (hour < 18) return "下午好";
    return "晚上好";
  })();
  const dateLabel = new Date().toLocaleDateString("zh-CN", {
    weekday: "long",
    month: "long",
    day: "numeric",
  });
  const navigate = (route: string) => onNavigate?.(route);
  const dueCount = today.status === "ready" ? today.data.pending_reviews_count : null;
  const continueItems = today.status === "ready" ? today.data.continue_items.slice(0, 4) : [];
  const latestStudy = today.status === "ready"
    ? today.data.continue_items.reduce<ContinueItem | null>((latest, item) => {
        if (!item.last_studied_at) return latest;
        return !latest || item.last_studied_at > latest.last_studied_at ? item : latest;
      }, null)
    : null;

  return (
    <div className="page-scroll personal-hub-page">
      <header className="hub-header">
        <div>
          <p className="hub-date">{dateLabel}</p>
          <h1>{greeting}，继续上次的内容。</h1>
          <p>从待复习内容继续，也可以查看已收藏的知识、行程、新闻更新和最近笔记。</p>
        </div>
        <div className="hub-header-actions">
          <button type="button" className="hub-button" onClick={onNewNote}>
            <NotePencil size={16} /> 写笔记
          </button>
          <button type="button" className="hub-button hub-button-primary" onClick={() => navigate("#review")}>
            <Cards size={17} /> 打开复习中心 <ArrowRight size={15} />
          </button>
        </div>
      </header>

      <section className="hub-focus-grid" aria-label="今天的工作台概览">
        <button type="button" className="hub-focus-card hub-focus-review" onClick={() => navigate("#review")}>
          <span className="hub-focus-icon"><Cards size={18} /></span>
          <span className="hub-focus-copy">
            <small>今日待复习</small>
            <strong>{dueCount === null ? "—" : `${dueCount} 张卡片`}</strong>
            <span>{dueCount === null ? "读取你的学习进度" : dueCount > 0 ? "回到间隔重复队列" : "暂无到期卡片，也可继续学习"}</span>
          </span>
          <ArrowDownRight size={20} />
        </button>
        <div className="hub-stat-card">
          <span className="hub-stat-icon"><CheckCircle size={18} /></span>
          <small>连续学习</small>
          <strong>{today.status === "ready" ? `${today.data.recent_streak_days} 天` : "—"}</strong>
          <span>{latestStudy ? `最近学习 ${monthDay(latestStudy.last_studied_at)}` : "学习记录会从首次学习开始"}</span>
        </div>
        <div className="hub-stat-card">
          <span className="hub-stat-icon"><Brain size={18} /></span>
          <small>学习概览</small>
          <strong>{today.status === "ready" ? `${Math.round(today.data.average_mastery)}%` : "—"}</strong>
          <span>平均掌握度</span>
        </div>
      </section>

      <div className="hub-main-grid">
        <section className="hub-section hub-learning-section" aria-labelledby="hub-learning-title">
          <div className="hub-section-heading">
            <div><h2 id="hub-learning-title">接着上次继续</h2></div>
            <button type="button" onClick={() => navigate("#review")}>复习中心 <ArrowRight size={14} /></button>
          </div>
          <ResourceState
            resource={today}
            retry={retryToday}
            isEmpty={(data) => data.continue_items.length === 0 && data.pending_reviews_count === 0}
            empty={(
              <div className="hub-empty">
                <BookOpenText size={21} />
                <strong>这里会接住你正在学的内容</strong>
                <p>开始一次学习或复习后，你的进度会出现在这里。</p>
                <button type="button" onClick={() => navigate("#history")}>浏览 History <ArrowRight size={14} /></button>
              </div>
            )}
          >
            {(data) => (
              <>
                {data.pending_reviews_count > 0 ? (
                  <button type="button" className="hub-review-banner" onClick={() => navigate("#review")}>
                    <span className="hub-review-mark"><Cards size={19} /></span>
                    <span><small>复习队列</small><strong>{data.pending_reviews_count} 张卡片等你复习</strong><em>答题后，原有学习进度会继续累积。</em></span>
                    <ArrowRight size={18} />
                  </button>
                ) : null}
                {continueItems.length > 0 ? (
                  <div className="hub-continue-list">
                    {continueItems.map((item) => (
                      <button
                        type="button"
                        className="hub-continue-item"
                        key={`${item.module}:${item.entity_type}:${item.entity_id}`}
                        onClick={() => navigate(routeForEntity(item.module, item.entity_type, item.entity_id))}
                      >
                        <span className="hub-row-icon"><BookOpenText size={17} /></span>
                        <span className="hub-row-copy"><small>{homeContentLabel(item.module, item.entity_type, "学习内容")}</small><strong>{item.title}</strong><span>最近学习 {shortDate(item.last_studied_at)}</span></span>
                        {item.progress_percent == null ? null : <span className="hub-progress-label">掌握 {Math.round(item.progress_percent)}%</span>}
                        <ArrowRight size={15} />
                      </button>
                    ))}
                  </div>
                ) : data.pending_reviews_count === 0 ? (
                  <div className="hub-empty hub-empty-compact"><p>还没有进行中的学习项。</p><button type="button" onClick={() => navigate("#geography")}>去探索 <ArrowRight size={14} /></button></div>
                ) : null}
                <div className="hub-learning-footer">
                  <span>今日学习 <strong>{data.studied_topics_today}</strong> 个主题</span>
                  <button type="button" onClick={() => navigate("#collections")}>管理学习资料 <ArrowRight size={14} /></button>
                </div>
              </>
            )}
          </ResourceState>
        </section>

        <section className="hub-section hub-knowledge-section" aria-labelledby="hub-knowledge-title">
          <div className="hub-section-heading">
            <div><h2 id="hub-knowledge-title">最近收藏的条目</h2></div>
            <button type="button" onClick={() => navigate("#collections")}>全部收藏 <ArrowRight size={14} /></button>
          </div>
          <ResourceState
            resource={savedReferences}
            retry={retrySavedReferences}
            isEmpty={(items) => items.length === 0}
            empty={(
              <div className="hub-empty">
                <FolderSimple size={21} />
                <strong>还没有收藏条目</strong>
                <p>把历史或地理条目加入专题合集，之后可从这里继续查看。</p>
                <button type="button" onClick={() => navigate("#collections")}>打开专题合集 <ArrowRight size={14} /></button>
              </div>
            )}
          >
            {(references) => (
              <div className="hub-saved-list">
                {references.map(({ collection, item }) => (
                  <article className="hub-saved-item" key={item.id}>
                    <button type="button" className="hub-saved-source" onClick={() => navigate(routeForEntity(item.module, item.entity_type, item.entity_id))}>
                      <span className={`hub-source-dot hub-source-${item.module}`} aria-hidden="true" />
                      <span className="hub-row-copy"><small>{homeContentLabel(item.module, item.entity_type, "收藏内容")} / {collection.title}</small><strong>{item.title}</strong>
                        {item.note ? <span className="hub-note-preview">{item.note}</span> : <span className="hub-note-preview hub-muted-note">还没有个人笔记</span>}
                      </span>
                      <ArrowRight size={14} />
                    </button>
                    <button type="button" className="hub-edit-note" onClick={() => navigate("#collections")}><NotePencil size={13} /> 编辑笔记</button>
                  </article>
                ))}
              </div>
            )}
          </ResourceState>
        </section>
      </div>

      <section className="hub-section hub-travel-section" aria-labelledby="hub-travel-title">
        <div className="hub-section-heading">
          <div><h2 id="hub-travel-title">继续规划旅程</h2></div>
          <button type="button" onClick={() => navigate("#travel")}>打开 Travel <ArrowRight size={14} /></button>
        </div>
        <ResourceState
          resource={travelPlans}
          retry={retryTravelPlans}
          isEmpty={(plans) => plans.length === 0}
          empty={(
            <div className="hub-empty hub-empty-horizontal">
              <Compass size={22} />
              <div><strong>还没有保存的行程</strong><p>创建一份行程后，可以随时回来继续安排地点与日期。</p></div>
              <button type="button" onClick={() => navigate("#travel")}>开始规划 <ArrowRight size={14} /></button>
            </div>
          )}
        >
          {(plans) => (
            <div className="hub-travel-list">
              {plans.slice(0, 4).map((plan: TravelPlan) => (
                <button type="button" className="hub-travel-card" key={`${plan.context.city}:${plan.context.days}:${plan.context.date_range?.start ?? ""}`} onClick={() => navigate(travelPlanRoute(plan))}>
                  <span className="hub-row-copy"><small><MapPin size={12} /> {plan.context.city || "未命名目的地"}</small><strong>{plan.title || `${plan.context.days} 天行程`}</strong><span><CalendarDots size={13} /> {plan.context.date_range ? `${plan.context.date_range.start} — ${plan.context.date_range.end}` : `最近更新 ${shortDate(plan.updated_at)}`}</span></span>
                  <ArrowRight size={16} />
                </button>
              ))}
            </div>
          )}
        </ResourceState>
      </section>

      <div className="hub-bottom-grid">
        <section className="hub-section hub-reading-section" aria-labelledby="hub-reading-title">
          <div className="hub-section-heading">
            <div><h2 id="hub-reading-title">新闻更新</h2></div>
            <button type="button" onClick={() => navigate("#news")}>打开 News <ArrowRight size={14} /></button>
          </div>
          <ResourceState
            resource={headlines}
            retry={retryHeadlines}
            isEmpty={(articles) => articles.length === 0}
            empty={<div className="hub-empty hub-empty-compact"><Newspaper size={20} /><p>当前没有可显示的新闻。打开 News 查看新闻来源。</p><button type="button" onClick={() => navigate("#news")}>查看新闻来源 <ArrowRight size={14} /></button></div>}
          >
            {(articles: NewsArticle[]) => (
              <div className="hub-headline-list">
                {articles.map((article) => (
                  <button type="button" className="hub-headline" key={article.id} onClick={() => navigate("#news")}>
                    <span className="hub-headline-meta"><span>{article.source}</span><time>{shortDate(article.published_at)}</time>{article.is_read ? null : <i>未读</i>}</span>
                    <strong>{article.title}</strong>
                    {article.summary ? <span className="hub-headline-summary">{stripRssHtml(article.summary, article.url)}</span> : null}
                  </button>
                ))}
              </div>
            )}
          </ResourceState>
        </section>

        <section className="hub-section hub-notes-section" aria-labelledby="hub-notes-title">
          <div className="hub-section-heading">
            <div><h2 id="hub-notes-title">最近写作</h2></div>
            <button type="button" onClick={onNewNote}>新建笔记 <NotePencil size={14} /></button>
          </div>
          {recentFiles.length === 0 ? (
            <div className="hub-empty hub-empty-compact"><NotePencil size={20} /><p>还没有最近编辑的笔记。</p><button type="button" onClick={onNewNote}>写下第一条 <ArrowRight size={14} /></button></div>
          ) : (
            <ul className="hub-note-list">
              {recentFiles.slice(0, 4).map((path) => (
                <li key={path}><button type="button" onClick={() => onOpenNote(path)}><span><NotePencil size={15} /></span><strong>{fileName(path)}</strong><small>{path}</small><ArrowRight size={14} /></button></li>
              ))}
            </ul>
          )}
        </section>
      </div>

      <footer className="hub-footer"><ClockCounterClockwise size={14} /> 学习进度、收藏与行程保存在本地，刷新页面后可以继续。 <button type="button" onClick={() => navigate("#search")}>搜索所有内容 <ArrowRight size={13} /></button></footer>
    </div>
  );
}
