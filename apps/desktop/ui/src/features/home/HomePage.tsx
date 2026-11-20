import {
  ArrowRight,
  BookOpenText,
  CalendarDots,
  Cards,
  ClockCounterClockwise,
  Compass,
  MapPin,
  Newspaper,
  ArrowClockwise,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import type { TravelPlan } from "../../types";
import { errorMessage } from "../../utils";
import { homeClient, type HomeHeadline } from "./homeClient";
import { sourceRoute } from "../shared/sourceNavigation";
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
  onNavigate,
}: HomePageProps) {
  const todayLoader = useCallback(() => homeClient.today(), []);
  const travelLoader = useCallback(() => homeClient.recentTravelPlans(5), []);
  const newsLoader = useCallback(() => homeClient.headlines(4), []);

  const [today, retryToday] = useHomeResource(todayLoader, active);
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
  const continueItems = today.status === "ready" ? today.data.continue_items.slice(0, 4) : [];

  return (
    <div className="page-scroll personal-hub-page home-dash">
      <header className="hd-head">
        <div className="hd-head-main">
          <p className="hd-kicker">{dateLabel}</p>
          <h1 className="hd-title">{greeting}，继续上次的内容。</h1>
          <p className="hd-sub">
            {today.status === "ready"
              ? `今天有 ${today.data.pending_reviews_count} 张卡片待复习，已连续学习 ${today.data.recent_streak_days} 天。下面是继续学习、最近行程和资讯。`
              : "从待复习内容继续，也可以查看行程和资讯。"}
          </p>
        </div>
        <div className="hd-head-actions">
          <button type="button" className="hd-btn" onClick={() => navigate("#learn")}>
            <BookOpenText size={15} /> 学习
          </button>
          <button type="button" className="hd-btn" onClick={() => navigate("#travel")}>
            <Compass size={15} /> 旅行
          </button>
          <button type="button" className="hd-btn hd-btn-primary" onClick={() => navigate("#news")}>
            <Newspaper size={15} /> 资讯
          </button>
        </div>
      </header>

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
                    <button type="button" onClick={() => navigate("#learn")}>学习入口 <ArrowRight size={13} /></button>
                  </div>
                </>
              )}
            </ResourceState>
          </section>
        </div>

        <div className="hd-col">
          <section className="hd-panel" aria-labelledby="hd-travel-title">
            <div className="hd-panel-head">
              <h2 id="hd-travel-title">最近行程</h2>
              <button type="button" className="hd-link" onClick={() => navigate("#travel")}>
                打开旅行 <ArrowRight size={13} />
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
          <section className="hd-panel" aria-labelledby="hd-reading-title">
            <div className="hd-panel-head">
              <h2 id="hd-reading-title">资讯</h2>
              <button type="button" className="hd-link" onClick={() => navigate("#news")}>
                打开资讯 <ArrowRight size={13} />
              </button>
            </div>
            <ResourceState
              resource={headlines}
              retry={retryHeadlines}
              isEmpty={(articles) => articles.length === 0}
              empty={(
                <div className="hd-empty hd-empty-compact">
                  <Newspaper size={20} />
                  <p>当前没有可显示的新闻。打开资讯查看来源，RSS 在同一入口的另一个 Tab。</p>
                  <button type="button" onClick={() => navigate("#news")}>查看新闻来源 <ArrowRight size={13} /></button>
                </div>
              )}
            >
              {(articles: HomeHeadline[]) => (
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
                        {article.summary ? <span>{article.summary}</span> : null}
                      </span>
                      <ArrowRight size={15} />
                    </button>
                  ))}
                </div>
              )}
            </ResourceState>
          </section>
        </div>
      </div>

      <footer className="hd-footer">
        <ClockCounterClockwise size={14} />
        <span>学习、行程与新闻进度在后端；关闭应用后资讯不会继续抓取。</span>
        <button type="button" onClick={() => navigate("#learn")}>进入学习 <ArrowRight size={13} /></button>
      </footer>
    </div>
  );
}
