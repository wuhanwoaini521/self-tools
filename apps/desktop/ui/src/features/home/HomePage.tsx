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
  ReviewQueueStats,
  TodayDashboardData,
  TravelPlan,
} from "../../types";
import { errorMessage, fileName } from "../../utils";
import { learningClient } from "../learning/learningClient";
import { newsClient } from "../news/newsClient";
import { stripRssHtml } from "../rss/rssContent";
import { sourceRoute } from "../learning/sourceNavigation";
import { travelClient } from "../travel/travelClient";
import "./home.css";

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

function reviewSegments(stats: ReviewQueueStats) {
  return [
    { key: "overdue", label: "逾期", value: Math.max(0, stats.overdue_count) },
    { key: "due", label: "到期", value: Math.max(0, stats.due_count - stats.overdue_count) },
    { key: "upcoming", label: "未到期", value: Math.max(0, stats.upcoming_count) },
    { key: "learning", label: "学习中", value: Math.max(0, stats.learning_count) },
    { key: "mastered", label: "已掌握", value: Math.max(0, stats.mastered_count) },
  ];
}

/** 复习队列构成条：每段宽度=该状态卡片占比，只画有数据的段。 */
function ReviewMeter({ stats, large = false }: { stats: ReviewQueueStats; large?: boolean }) {
  const segments = reviewSegments(stats).filter((segment) => segment.value > 0);
  const total = segments.reduce((sum, segment) => sum + segment.value, 0);
  if (!total) return null;
  return (
    <div className={`hd-meter${large ? " hd-meter-lg" : ""}`} aria-hidden="true">
      {segments.map((segment) => (
        <span
          key={segment.key}
          className={`hd-seg hd-seg-${segment.key}`}
          style={{ width: `${(segment.value / total) * 100}%` }}
        />
      ))}
    </div>
  );
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
    <div className="page-scroll personal-hub-page home-dash">
      <header className="hd-head">
        <div className="hd-head-main">
          <p className="hd-kicker">{dateLabel}</p>
          <h1 className="hd-title">{greeting}，继续上次的内容。</h1>
          <p className="hd-sub">
            {today.status === "ready"
              ? `今天有 ${today.data.pending_reviews_count} 张卡片待复习，已连续学习 ${today.data.recent_streak_days} 天。收藏、行程与新闻更新都在下面。`
              : "从待复习内容继续，也可以查看收藏的知识、行程和新闻更新。"}
          </p>
        </div>
        <div className="hd-head-actions">
          <button type="button" className="hd-btn" onClick={onNewNote}>
            <NotePencil size={15} /> 写笔记
          </button>
          <button type="button" className="hd-btn hd-btn-primary" onClick={() => navigate("#review")}>
            <Cards size={15} /> 打开复习中心
          </button>
        </div>
      </header>

      <section className="hd-metrics" aria-label="今天的学习概览">
        <article className="hd-metric">
          <span className="hd-metric-label"><Cards size={14} /> 今日待复习</span>
          <strong className="hd-metric-value">
            {today.status === "ready" ? today.data.pending_reviews_count : "—"}
            <small>张卡片</small>
          </strong>
          <span className="hd-metric-note">
            {today.status === "ready"
              ? `逾期 ${today.data.review_stats.overdue_count} · 未到期 ${today.data.review_stats.upcoming_count}`
              : "读取你的学习进度"}
          </span>
          {today.status === "ready" ? <ReviewMeter stats={today.data.review_stats} /> : null}
        </article>
        <article className="hd-metric">
          <span className="hd-metric-label"><Brain size={14} /> 平均掌握度</span>
          <strong className="hd-metric-value">
            {today.status === "ready" ? `${Math.round(today.data.average_mastery)}%` : "—"}
          </strong>
          <span className="hd-metric-note">来自已有学习记录</span>
          {today.status === "ready" ? (
            <div className="hd-meter" aria-hidden="true">
              <span
                className="hd-seg hd-seg-due"
                style={{ width: `${Math.max(0, Math.min(100, today.data.average_mastery))}%` }}
              />
            </div>
          ) : null}
        </article>
        <article className="hd-metric">
          <span className="hd-metric-label"><CheckCircle size={14} /> 连续学习</span>
          <strong className="hd-metric-value">
            {today.status === "ready" ? today.data.recent_streak_days : "—"}
            <small>天</small>
          </strong>
          <span className="hd-metric-note">
            {latestStudy ? `最近学习 ${monthDay(latestStudy.last_studied_at)}` : "从首次学习开始记录"}
          </span>
        </article>
        <article className="hd-metric">
          <span className="hd-metric-label"><BookOpenText size={14} /> 今日学习</span>
          <strong className="hd-metric-value">
            {today.status === "ready" ? today.data.studied_topics_today : "—"}
            <small>个主题</small>
          </strong>
          <span className="hd-metric-note">今天新增的学习记录</span>
        </article>
      </section>

      <div className="hd-split">
        <div className="hd-col">
          <section className="hd-panel" aria-labelledby="hd-learning-title">
            <div className="hd-panel-head">
              <h2 id="hd-learning-title">接着上次继续</h2>
              <button type="button" className="hd-link" onClick={() => navigate("#review")}>
                复习中心 <ArrowRight size={13} />
              </button>
            </div>
            <ResourceState
              resource={today}
              retry={retryToday}
              isEmpty={(data) => data.continue_items.length === 0 && data.pending_reviews_count === 0}
              empty={(
                <div className="hd-empty">
                  <BookOpenText size={20} />
                  <strong>这里会接住你正在学的内容</strong>
                  <p>开始一次学习或复习后，你的进度会出现在这里。</p>
                  <button type="button" onClick={() => navigate("#history")}>浏览 History <ArrowRight size={13} /></button>
                </div>
              )}
            >
              {(data) => (
                <>
                  {data.pending_reviews_count > 0 ? (
                    <button type="button" className="hd-row hd-row-strong" onClick={() => navigate("#review")}>
                      <span className="hd-row-icon"><Cards size={16} /></span>
                      <span className="hd-row-main">
                        <small>复习队列</small>
                        <strong>{data.pending_reviews_count} 张卡片等你复习</strong>
                        <span>答题后，原有学习进度会继续累积。</span>
                      </span>
                      <ArrowRight size={15} />
                    </button>
                  ) : null}
                  {continueItems.length > 0 ? (
                    <div className="hd-rows">
                      {continueItems.map((item) => (
                        <button
                          type="button"
                          className="hd-row"
                          key={`${item.module}:${item.entity_type}:${item.entity_id}`}
                          onClick={() => navigate(routeForEntity(item.module, item.entity_type, item.entity_id))}
                        >
                          <span className="hd-row-icon"><BookOpenText size={16} /></span>
                          <span className="hd-row-main">
                            <small>{homeContentLabel(item.module, item.entity_type, "学习内容")}</small>
                            <strong>{item.title}</strong>
                            <span>最近学习 {shortDate(item.last_studied_at)}</span>
                          </span>
                          {item.progress_percent == null ? null : (
                            <span className="hd-row-badge">掌握 {Math.round(item.progress_percent)}%</span>
                          )}
                          <ArrowRight size={15} />
                        </button>
                      ))}
                    </div>
                  ) : data.pending_reviews_count === 0 ? (
                    <div className="hd-empty hd-empty-compact">
                      <p>还没有进行中的学习项。</p>
                      <button type="button" onClick={() => navigate("#geography")}>去探索 <ArrowRight size={13} /></button>
                    </div>
                  ) : null}
                  <div className="hd-panel-foot">
                    <span>今日学习 <strong>{data.studied_topics_today}</strong> 个主题</span>
                    <button type="button" onClick={() => navigate("#collections")}>管理学习资料 <ArrowRight size={13} /></button>
                  </div>
                </>
              )}
            </ResourceState>
          </section>

          <section className="hd-panel" aria-labelledby="hd-reading-title">
            <div className="hd-panel-head">
              <h2 id="hd-reading-title">新闻更新</h2>
              <button type="button" className="hd-link" onClick={() => navigate("#news")}>
                打开 News <ArrowRight size={13} />
              </button>
            </div>
            <ResourceState
              resource={headlines}
              retry={retryHeadlines}
              isEmpty={(articles) => articles.length === 0}
              empty={(
                <div className="hd-empty hd-empty-compact">
                  <Newspaper size={20} />
                  <p>当前没有可显示的新闻。打开 News 查看新闻来源。</p>
                  <button type="button" onClick={() => navigate("#news")}>查看新闻来源 <ArrowRight size={13} /></button>
                </div>
              )}
            >
              {(articles: NewsArticle[]) => (
                <div className="hd-rows">
                  {articles.map((article) => (
                    <button type="button" className="hd-row hd-row-news" key={article.id} onClick={() => navigate("#news")}>
                      <span className="hd-row-main">
                        <small>
                          {article.source}
                          <i>{shortDate(article.published_at)}</i>
                          {article.is_read ? null : <em>未读</em>}
                        </small>
                        <strong>{article.title}</strong>
                        {article.summary ? <span>{stripRssHtml(article.summary, article.url)}</span> : null}
                      </span>
                      <ArrowRight size={15} />
                    </button>
                  ))}
                </div>
              )}
            </ResourceState>
          </section>

          <section className="hd-panel" aria-labelledby="hd-travel-title">
            <div className="hd-panel-head">
              <h2 id="hd-travel-title">继续规划旅程</h2>
              <button type="button" className="hd-link" onClick={() => navigate("#travel")}>
                打开 Travel <ArrowRight size={13} />
              </button>
            </div>
            <ResourceState
              resource={travelPlans}
              retry={retryTravelPlans}
              isEmpty={(plans) => plans.length === 0}
              empty={(
                <div className="hd-empty hd-empty-compact">
                  <Compass size={20} />
                  <p>还没有保存的行程。创建一份行程后，可以随时回来继续安排地点与日期。</p>
                  <button type="button" onClick={() => navigate("#travel")}>开始规划 <ArrowRight size={13} /></button>
                </div>
              )}
            >
              {(plans) => (
                <div className="hd-rows">
                  {plans.slice(0, 4).map((plan: TravelPlan) => (
                    <button
                      type="button"
                      className="hd-row"
                      key={`${plan.context.city}:${plan.context.days}:${plan.context.date_range?.start ?? ""}`}
                      onClick={() => navigate(travelPlanRoute(plan))}
                    >
                      <span className="hd-row-icon"><MapPin size={16} /></span>
                      <span className="hd-row-main">
                        <small>{plan.context.city || "未命名目的地"}</small>
                        <strong>{plan.title || `${plan.context.days} 天行程`}</strong>
                        <span>
                          <CalendarDots size={12} />
                          {plan.context.date_range
                            ? `${plan.context.date_range.start} — ${plan.context.date_range.end}`
                            : `最近更新 ${shortDate(plan.updated_at)}`}
                        </span>
                      </span>
                      <ArrowRight size={15} />
                    </button>
                  ))}
                </div>
              )}
            </ResourceState>
          </section>
        </div>

        <div className="hd-col">
          <section className="hd-panel" aria-labelledby="hd-review-title">
            <div className="hd-panel-head">
              <h2 id="hd-review-title">复习队列组成</h2>
              <button type="button" className="hd-link" onClick={() => navigate("#review")}>
                开始复习 <ArrowRight size={13} />
              </button>
            </div>
            {today.status === "ready" ? (
              today.data.review_stats.total_cards > 0 ? (
                <>
                  <ReviewMeter stats={today.data.review_stats} large />
                  <ul className="hd-legend">
                    {reviewSegments(today.data.review_stats).map((segment) => (
                      <li key={segment.key}>
                        <i className={`hd-dot hd-dot-${segment.key}`} aria-hidden="true" />
                        <span>{segment.label}</span>
                        <b>{segment.value}</b>
                      </li>
                    ))}
                  </ul>
                </>
              ) : (
                <p className="hd-muted">还没有复习卡片，开始学习后这里会显示队列构成。</p>
              )
            ) : (
              <p className="hd-muted">正在读取复习队列…</p>
            )}
          </section>

          <section className="hd-panel" aria-labelledby="hd-knowledge-title">
            <div className="hd-panel-head">
              <h2 id="hd-knowledge-title">最近收藏</h2>
              <button type="button" className="hd-link" onClick={() => navigate("#collections")}>
                全部收藏 <ArrowRight size={13} />
              </button>
            </div>
            <ResourceState
              resource={savedReferences}
              retry={retrySavedReferences}
              isEmpty={(items) => items.length === 0}
              empty={(
                <div className="hd-empty hd-empty-compact">
                  <FolderSimple size={20} />
                  <p>把历史或地理条目加入专题合集，之后可从这里继续查看。</p>
                  <button type="button" onClick={() => navigate("#collections")}>打开专题合集 <ArrowRight size={13} /></button>
                </div>
              )}
            >
              {(references) => (
                <div className="hd-rows">
                  {references.map(({ collection, item }) => (
                    <button
                      type="button"
                      className="hd-row hd-row-saved"
                      key={item.id}
                      onClick={() => navigate(routeForEntity(item.module, item.entity_type, item.entity_id))}
                    >
                      <span className={`hd-dot hd-dot-${item.module}`} aria-hidden="true" />
                      <span className="hd-row-main">
                        <small>{homeContentLabel(item.module, item.entity_type, "收藏内容")} / {collection.title}</small>
                        <strong>{item.title}</strong>
                        <span>{item.note || "还没有个人笔记"}</span>
                      </span>
                      <ArrowRight size={15} />
                    </button>
                  ))}
                </div>
              )}
            </ResourceState>
          </section>

          <section className="hd-panel" aria-labelledby="hd-notes-title">
            <div className="hd-panel-head">
              <h2 id="hd-notes-title">最近写作</h2>
              <button type="button" className="hd-link" onClick={onNewNote}>
                新建笔记 <NotePencil size={13} />
              </button>
            </div>
            {recentFiles.length === 0 ? (
              <div className="hd-empty hd-empty-compact">
                <NotePencil size={20} />
                <p>还没有最近编辑的笔记。</p>
                <button type="button" onClick={onNewNote}>写下第一条 <ArrowRight size={13} /></button>
              </div>
            ) : (
              <ul className="hd-notes">
                {recentFiles.slice(0, 4).map((path) => (
                  <li key={path}>
                    <button type="button" onClick={() => onOpenNote(path)}>
                      <span className="hd-row-icon"><NotePencil size={15} /></span>
                      <span className="hd-row-main">
                        <strong>{fileName(path)}</strong>
                        <small>{path}</small>
                      </span>
                      <ArrowRight size={14} />
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>
      </div>

      <footer className="hd-footer">
        <ClockCounterClockwise size={14} />
        <span>学习进度、收藏与行程保存在本地，刷新页面后可以继续。</span>
        <button type="button" onClick={() => navigate("#search")}>搜索所有内容 <ArrowRight size={13} /></button>
      </footer>
    </div>
  );
}
