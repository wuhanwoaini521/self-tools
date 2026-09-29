import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowRight, ArrowSquareOut, CalendarBlank, CaretDown, Clock, Cloud, CloudRain, CloudSun, Compass, House, MapPin, Snowflake, Star, Sun, Warning, WarningCircle } from "@phosphor-icons/react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { Attraction, CityGuide, ItineraryDay, ItineraryStop, SourceLevel, TravelDateRange, TravelSource, VerifiedValue, WeatherForecast } from "../../types";
import { formatDateTime, isTauriRuntime } from "../../utils";
import { loadAmap, type AmapMapInstance, type AmapNamespace, type AmapOverlay } from "../geography/AmapMap";

function openLink(url: string) { if (isTauriRuntime()) void openUrl(url); else window.open(url, "_blank"); }

function Section({ title, eyebrow, children, className = "" }: { title: string; eyebrow?: string; children: ReactNode; className?: string }) {
  return <section className={`travel-section ${className}`}><header>{eyebrow ? <small>{eyebrow}</small> : null}<h2>{title}</h2></header><div className="travel-section-body">{children}</div></section>;
}

function VerifiedBadge({ value }: { value: VerifiedValue }) {
  return <span className={`travel-verified ${value.verified ? "" : "pending"}`} title={`${value.verified_sources} 个来源 · 主来源 ${value.primary_source}`}>
    {value.value}<i>{value.verified ? `✓ ${value.confidence}` : "待确认"}</i>{value.has_conflict ? <i className="travel-conflict">⚠ 多版本</i> : null}
  </span>;
}

function Stars({ level }: { level: SourceLevel }) {
  const count: Record<SourceLevel, number> = { S: 5, A: 4, B: 3, C: 2 };
  return <span className="travel-stars" aria-label={`${level} 级来源`}>{Array.from({ length: 5 }, (_, index) => <Star key={index} size={11} className={index < count[level] ? "filled" : "blank"} weight="fill" />)}</span>;
}

function WeatherIcon({ text }: { text: string }) {
  if (/雨|雷/.test(text)) return <CloudRain size={22} weight="fill" />;
  if (/雪|冰雹/.test(text)) return <Snowflake size={22} weight="fill" />;
  if (/晴/.test(text)) return <Sun size={22} weight="fill" />;
  if (/多云|阴/.test(text)) return <CloudSun size={22} weight="fill" />;
  return <Cloud size={22} weight="fill" />;
}

function weekday(date: string) { const value = new Date(`${date}T00:00:00`); return Number.isNaN(value.getTime()) ? date : new Intl.DateTimeFormat("zh-CN", { weekday: "short" }).format(value); }

function WeatherCard({ forecast, dateRange }: { forecast: WeatherForecast; dateRange: TravelDateRange | null }) {
  const selected = dateRange ? forecast.days.filter((day) => day.date >= dateRange.start && day.date <= dateRange.end) : forecast.days;
  const current = selected[0] ?? forecast.days[0];
  if (!current) return null;
  return <div className="travel-weather-card"><div className="travel-weather-current"><div><small>{forecast.city} · 近期预报</small><strong>{current.temp_max}°</strong><p>{current.text_day} <span>最低 {current.temp_min}°</span></p></div><WeatherIcon text={current.text_day} /></div><div className="travel-weather-days" role="list">{forecast.days.map((day) => <div className={dateRange && day.date >= dateRange.start && day.date <= dateRange.end ? "selected" : ""} key={day.date} role="listitem"><small>{weekday(day.date)}</small><WeatherIcon text={day.text_day} /><b>{day.temp_max}°</b><span>{day.temp_min}°</span><em>{day.text_day}</em></div>)}</div>{dateRange && selected.length === 0 ? <p className="travel-weather-note">所选日期不在当前预报窗口，天气仅供参考。</p> : null}</div>;
}

function QualitySummary({ guide }: { guide: CityGuide }) {
  const evidence = guide.evidence;
  if (!evidence || evidence.source_count === 0) return null;
  return <details className="travel-quality"><summary><span>数据质量</span><b><i className="travel-quality-dot" />{evidence.quality}</b><small>{evidence.source_count} sources · {evidence.verified_count} verified</small><CaretDown size={15} /></summary><div><span>全文 / 结构化支持 {Math.max(0, evidence.source_count - evidence.snippet_only_count)}</span><span>仅搜索摘要 {evidence.snippet_only_count}</span><span>冲突信息 {evidence.conflict_count}</span></div></details>;
}

function attractionKey(item: Attraction) {
  return item.poi_id || item.id || item.normalized_name || item.name.trim().toLocaleLowerCase();
}

function attractionLabel(item: Attraction) {
  return item.why_for_this_trip || item.why_go || item.intro || "已从城市景点数据中找到，可加入行程后安排日期。";
}

function uniqueAttractions(items: Attraction[]) {
  const seen = new Set<string>();
  return items.filter((item) => {
    const key = attractionKey(item).trim().toLocaleLowerCase();
    if (!item.name.trim() || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

interface SavedTravelPlan {
  selectedKeys: string[];
  assignedDays: Record<string, number>;
}

function readTravelPlan(key: string, catalog: Attraction[], initial: Attraction[], dayCount: number): SavedTravelPlan {
  try {
    const saved = localStorage.getItem(key);
    if (saved) {
      const plan = JSON.parse(saved) as SavedTravelPlan;
      const validKeys = new Set(catalog.map(attractionKey));
      return {
        selectedKeys: plan.selectedKeys.filter((itemKey) => validKeys.has(itemKey)),
        assignedDays: Object.fromEntries(Object.entries(plan.assignedDays).filter(([itemKey, day]) => validKeys.has(itemKey) && day >= 1 && day <= dayCount)),
      };
    }
  } catch {
    // 浏览器存储不可用时，本次页面仍可正常编辑。
  }
  return {
    selectedKeys: initial.map(attractionKey),
    assignedDays: Object.fromEntries(initial.map((item) => [attractionKey(item), item.recommended_day && item.recommended_day <= dayCount ? item.recommended_day : 1])),
  };
}

function AttractionCard({ item, index, alternative = false, action }: { item: Attraction; index: number; alternative?: boolean; action?: ReactNode }) {
  const why = item.why_for_this_trip || item.why_go || item.intro;
  return <article className={`travel-curated-card ${alternative ? "alternative" : ""}`}><div className="travel-card-index">{alternative ? "备选" : String(index + 1).padStart(2, "0")}</div><div className="travel-card-content"><div className="travel-card-title"><h3>{item.name}</h3>{item.recommended_day ? <span>Day {item.recommended_day}</span> : null}</div>{why ? <p>{why}</p> : null}<div className="travel-card-meta">{item.area ? <span><MapPin size={13} />{item.area}</span> : null}{item.suggested_duration ? <span><Clock size={13} />{item.suggested_duration}</span> : null}{item.best_for.length > 0 ? <span>{item.best_for.join(" · ")}</span> : null}</div>{item.why_for_this_trip && item.why_for_this_trip !== why ? <small className="travel-card-fit">这次旅行：{item.why_for_this_trip}</small> : null}{(item.opening_hours || item.ticket || item.reservation) ? <div className="travel-card-facts">{item.opening_hours ? <span>开放 <VerifiedBadge value={item.opening_hours} /></span> : null}{item.ticket ? <span>门票 <VerifiedBadge value={item.ticket} /></span> : null}{item.reservation ? <span>预约 <VerifiedBadge value={item.reservation} /></span> : null}</div> : null}{action ? <div className="travel-card-actions">{action}</div> : null}</div></article>;
}

function DayStop({ stop, index }: { stop: ItineraryStop; index: number }) {
  return <li className="travel-day-stop"><div className="travel-day-stop-time">{stop.time || (index === 0 ? "09:00" : "—")}</div><div className="travel-day-stop-line"><span /><i /></div><div className="travel-day-stop-body"><div><h3>{stop.name}</h3>{stop.area ? <small>{stop.area}</small> : null}</div>{stop.reason || stop.note ? <p>{stop.reason || stop.note}</p> : null}<div className="travel-day-stop-meta">{stop.duration ? <span><Clock size={13} />{stop.duration}</span> : null}{stop.travel_time ? <span><ArrowRight size={13} />{stop.travel_time}</span> : null}</div></div></li>;
}

function DayItineraryCard({ day }: { day: ItineraryDay }) {
  return <article className="travel-day-card"><header><div><span>DAY {day.day}</span><h3>{day.title || `第 ${day.day} 天`}</h3></div>{day.theme ? <p>{day.theme}</p> : null}</header>{day.stops.length > 0 ? <ol>{day.stops.map((stop, index) => <DayStop key={`${stop.name}-${index}`} stop={stop} index={index} />)}</ol> : <p className="travel-day-empty">这一天暂无足够可靠的景点数据，建议以市内活动为主。</p>}</article>;
}

function GuideMap({ attractions, days, apiKey, securityJsCode }: { attractions: Attraction[]; days: ItineraryDay[]; apiKey?: string | null; securityJsCode?: string | null }) {
  const [selectedDay, setSelectedDay] = useState<number | null>(days[0]?.day ?? null);
  const [mapError, setMapError] = useState<string | null>(null);
  const mapContainer = useRef<HTMLDivElement>(null);
  const points = useMemo(() => attractions.filter((item) => item.coordinates && (selectedDay === null || item.recommended_day === selectedDay)).slice(0, 10), [attractions, selectedDay]);
  const hasMapConfig = Boolean(apiKey?.trim() && securityJsCode?.trim());
  const mapUrl = `https://uri.amap.com/marker?${new URLSearchParams({ markers: points.map((point) => `${point.coordinates!.longitude},${point.coordinates!.latitude},${point.name}`).join("|"), src: "self-tools", callnative: "0" })}`;

  useEffect(() => {
    const container = mapContainer.current;
    if (!container || !hasMapConfig || points.length === 0) return;
    let cancelled = false;
    let map: AmapMapInstance | null = null;
    setMapError(null);
    void loadAmap(apiKey!, securityJsCode!).then((amap: AmapNamespace) => {
      if (cancelled) return;
      const first = points[0].coordinates!;
      map = new amap.Map(container, { center: [first.longitude, first.latitude], zoom: 11, viewMode: "2D", resizeEnable: true, mapStyle: "amap://styles/normal" });
      const markers: AmapOverlay[] = points.map((point, index) => new amap.Marker({
        position: [point.coordinates!.longitude, point.coordinates!.latitude],
        title: point.name,
        content: `<div class="travel-amap-marker">${index + 1}</div>`,
        offset: [-12, -12],
      }));
      map.add(markers);
      map.setFitView(markers, false, [42, 42, 42, 42], 15);
    }).catch((error: unknown) => {
      if (!cancelled) setMapError(error instanceof Error ? error.message : "高德地图加载失败");
    });
    return () => {
      cancelled = true;
      map?.destroy();
    };
  }, [apiKey, hasMapConfig, points, securityJsCode]);

  if (points.length === 0) return null;
  return <Section title="路线位置" eyebrow="MAP"><div className="travel-map-card">{days.length > 0 ? <div className="travel-map-tabs"><button className={selectedDay === null ? "active" : ""} onClick={() => setSelectedDay(null)}>全部</button>{days.map((day) => <button className={selectedDay === day.day ? "active" : ""} key={day.day} onClick={() => setSelectedDay(day.day)}>Day {day.day}</button>)}</div> : null}<div className="travel-map-canvas-wrap">{hasMapConfig ? <div ref={mapContainer} className="travel-map-canvas" role="img" aria-label="行程 POI 高德地图" /> : <div className="travel-map-unconfigured"><Compass size={25} /><span>配置高德 JS API 后可在此显示真实地图</span><small>复用「设置 → Geography」中的高德 Key 和安全密钥</small></div>}{mapError ? <div className="travel-map-status" role="alert">地图加载失败：{mapError}</div> : null}</div><div className="travel-map-points">{points.map((point, index) => <span key={`${point.name}-${index}`}><b>{index + 1}</b>{point.name}</span>)}</div><button className="travel-link-button" onClick={() => openLink(mapUrl)}><ArrowSquareOut size={14} />在高德地图中查看</button></div></Section>;
}

function SourceRow({ source }: { source: TravelSource }) {
  return <div className="travel-source"><div className="travel-source-main"><b title={source.url}>{source.title}</b><span className="travel-source-meta"><Stars level={source.level} /><i className={`travel-level travel-level-${source.level.toLowerCase()}`}>{source.level}</i>{source.state === "snippet_only" ? <i className="travel-snippet-tag">仅搜索摘要</i> : null}<small>{source.host} · {formatDateTime(source.fetched_at)}</small></span></div><button title={source.url} onClick={() => openLink(source.url)}><ArrowSquareOut size={14} />查看来源</button></div>;
}

interface TravelGuideProps { guide: CityGuide; fromCache: boolean; amapApiKey?: string | null; amapSecurityJsCode?: string | null; }

export function TravelGuide({ guide, fromCache, amapApiKey, amapSecurityJsCode }: TravelGuideProps) {
  const { city, meta } = guide;
  const region = [city.province, city.country].filter(Boolean).join(" · ") || "中国";
  const initialPicks = (guide.top_picks.length > 0 ? guide.top_picks : guide.attractions).slice(0, 6);
  const catalog = useMemo(() => uniqueAttractions([...guide.attractions, ...guide.top_picks, ...guide.alternatives]), [guide]);
  const storageKey = `travel-plan:${city.name}:${meta.days}:${meta.date_range?.start ?? ""}:${meta.date_range?.end ?? ""}`;
  const [plan, setPlan] = useState(() => readTravelPlan(storageKey, catalog, initialPicks, meta.days));
  const [attractionSearch, setAttractionSearch] = useState("");
  useEffect(() => {
    try { localStorage.setItem(storageKey, JSON.stringify(plan)); } catch { /* 存储不可用时保留本次会话编辑。 */ }
  }, [plan, storageKey]);
  const catalogByKey = useMemo(() => new Map(catalog.map((item) => [attractionKey(item), item])), [catalog]);
  const selectedAttractions = useMemo(() => plan.selectedKeys.map((key) => catalogByKey.get(key)).filter((item): item is Attraction => Boolean(item)), [catalogByKey, plan.selectedKeys]);
  const assignedAttractions = useMemo(() => selectedAttractions.map((item) => ({ ...item, recommended_day: plan.assignedDays[attractionKey(item)] ?? item.recommended_day ?? 1 })), [plan.assignedDays, selectedAttractions]);
  const selectedKeys = new Set(plan.selectedKeys);
  const availableAttractions = catalog.filter((item) => !selectedKeys.has(attractionKey(item)) && `${item.name} ${item.area ?? ""} ${item.best_for.join(" ")}`.toLocaleLowerCase().includes(attractionSearch.trim().toLocaleLowerCase()));
  const days = guide.itinerary_days.map((day) => {
    const stops = day.stops.filter((stop) => {
      const match = catalog.find((item) => item.name.trim().toLocaleLowerCase() === stop.name.trim().toLocaleLowerCase());
      return !match || (selectedKeys.has(attractionKey(match)) && (plan.assignedDays[attractionKey(match)] ?? match.recommended_day ?? 1) === day.day);
    });
    const stopNames = new Set(stops.map((stop) => stop.name.trim().toLocaleLowerCase()));
    const additions = selectedAttractions.filter((item) => (plan.assignedDays[attractionKey(item)] ?? item.recommended_day ?? 1) === day.day && !stopNames.has(item.name.trim().toLocaleLowerCase())).map((item): ItineraryStop => ({
      name: item.name,
      note: null,
      time: null,
      duration: item.suggested_duration,
      area: item.area,
      reason: attractionLabel(item),
      travel_time: null,
    }));
    return { ...day, stops: [...stops, ...additions] };
  });
  const mainWarning = guide.quick_decisions.main_warning || (guide.warnings[0] ? `${guide.warnings[0].title}：${guide.warnings[0].text}` : null);
  return <article className="travel-guide"><header className="travel-guide-header"><div className="travel-guide-kicker"><span>TRAVEL EDITION</span>{fromCache ? <i>缓存结果</i> : null}</div><h1>{city.name}{city.name_en ? <span>{city.name_en}</span> : null}</h1><p>{region}</p><div className="travel-guide-trip"><CalendarBlank size={15} /><b>{meta.days} 天</b>{meta.date_range ? <span>{meta.date_range.start} 至 {meta.date_range.end}</span> : <span>按你的节奏规划</span>}<span className="travel-guide-update">更新于 {formatDateTime(meta.updated_at)}</span></div>{guide.summary ? <p className="travel-guide-lede">{guide.summary}</p> : null}</header>
    <section className="travel-conclusion"><div className="travel-conclusion-title"><small>先看结论</small><h2>{guide.quick_decisions.trip_style || "这座城市，适合这样玩"}</h2></div><div className="travel-conclusion-grid"><div><House size={17} /><span>建议住</span><b>{guide.quick_decisions.best_area_to_stay || guide.accommodation_areas[0]?.name || "按路线选择市区"}</b></div><div><span className="travel-conclusion-star">★</span><span>当前行程</span><b>{selectedAttractions.slice(0, 3).map((item) => item.name).join(" · ") || "尚未选择景点"}</b></div><div><span className="travel-conclusion-food">⌁</span><span>必吃</span><b>{guide.quick_decisions.signature_food || guide.foods[0]?.name || "本地特色美食"}</b></div>{mainWarning ? <div className="warning"><Warning size={17} /><span>注意</span><b>{mainWarning}</b></div> : null}</div></section>
    <QualitySummary guide={guide} />
    <div className="travel-guide-layout">
      <main className="travel-guide-main">
        {days.length > 0 ? <Section title={`${meta.days} 天怎么玩`} eyebrow="YOUR ROUTE" className="travel-itinerary-section"><div className="travel-days">{days.map((day) => <DayItineraryCard key={day.day} day={day} />)}</div></Section> : null}
        {selectedAttractions.length > 0 ? <Section title="本次行程推荐" eyebrow="YOUR PICKS"><div className="travel-curated-list">{assignedAttractions.map((item, index) => {
          const key = attractionKey(item);
          return <AttractionCard key={key} item={item} index={index} action={<><label>安排到 <select aria-label={`将${item.name}安排到哪一天`} value={item.recommended_day} onChange={(event) => setPlan((current) => ({ ...current, assignedDays: { ...current.assignedDays, [key]: Number(event.target.value) } }))}>{Array.from({ length: Math.max(1, meta.days) }, (_, dayIndex) => <option key={dayIndex + 1} value={dayIndex + 1}>Day {dayIndex + 1}</option>)}</select></label><button type="button" className="travel-remove-pick" onClick={() => setPlan((current) => ({ ...current, selectedKeys: current.selectedKeys.filter((selectedKey) => selectedKey !== key) }))}>移出行程</button></>} />;
        })}</div></Section> : <p className="travel-no-picks">还没有选择景点。可以从下方景点目录添加。</p>}
        <Section title={`发现更多景点 · ${catalog.length}`} eyebrow="CITY ATTRACTIONS" className="travel-discovery-section">
          <p className="travel-discovery-intro">这里汇总了本次城市研究发现的景点。加入后可安排到任意一天，也可以随时移出。</p>
          <label className="travel-discovery-search"><span>搜索景点或区域</span><input type="search" value={attractionSearch} onChange={(event) => setAttractionSearch(event.target.value)} placeholder={`在${city.name}的已发现景点中搜索`} /></label>
          {availableAttractions.length > 0 ? <div className="travel-discovery-list">{availableAttractions.map((item) => {
            const key = attractionKey(item);
            return <article className="travel-discovery-item" key={key}><div><h3>{item.name}</h3>{item.area ? <small><MapPin size={12} />{item.area}</small> : null}<p>{attractionLabel(item)}</p></div><button type="button" onClick={() => setPlan((current) => ({ selectedKeys: [...current.selectedKeys, key], assignedDays: { ...current.assignedDays, [key]: item.recommended_day && item.recommended_day <= meta.days ? item.recommended_day : 1 } }))}>加入行程</button></article>;
          })}</div> : <p className="travel-discovery-empty">{catalog.length === 0 ? `暂时没有发现${city.name}的景点目录；请配置 Travel 的高德 POI Key 后重新研究。` : availableAttractions.length === 0 && !attractionSearch.trim() ? "这些景点都已加入当前行程。" : "没有匹配的景点，试试其他关键词。"}</p>}
        </Section>
        <GuideMap attractions={assignedAttractions} days={days} apiKey={amapApiKey} securityJsCode={amapSecurityJsCode} />{(guide.foods.length > 0 || guide.restaurants.length > 0) ? <Section title="吃什么" eyebrow="LOCAL FLAVOUR"><div className="travel-food-intro">{guide.food_summary ? <p>{guide.food_summary}</p> : <p>先认准本地代表性吃法，再按当天路线选择最近的一家。</p>}{guide.foods.length > 0 ? <div className="travel-food-tags">{guide.foods.slice(0, 4).map((food) => <span key={food.name}><b>{food.name}</b>{food.dish_type ? ` · ${food.dish_type}` : ""}</span>)}</div> : null}</div>{guide.restaurants.length > 0 ? <div className="travel-restaurant-list">{guide.restaurants.slice(0, 5).map((place, index) => <div key={`${place.name}-${index}`}><span>{index + 1}</span><div><b>{place.name}</b>{place.area ? <small>{place.area}</small> : null}{place.route_day ? <small>Day {place.route_day}{place.distance_to_route ? ` · ${place.distance_to_route}` : ""}</small> : null}<p>{place.why_pick || place.note || place.signature_dish || "适合作为路线中的就餐选择"}</p></div></div>)}</div> : null}</Section> : null}</main><aside className="travel-guide-sidebar">{guide.weather && guide.weather.days.length > 0 ? <Section title={meta.date_range ? "这几天的情况" : "近期天气"} eyebrow="WEATHER"><WeatherCard forecast={guide.weather} dateRange={meta.date_range} /></Section> : null}{guide.accommodation_areas.length > 0 ? <Section title="住哪里" eyebrow="STAY"><div className="travel-stay-list">{guide.accommodation_areas.slice(0, 3).map((area, index) => <div key={`${area.name}-${index}`}><b>{area.name}</b>{area.note ? <p>{area.note}</p> : null}{area.budget ? <small>{area.budget}</small> : null}</div>)}</div></Section> : null}{guide.transport_summary || guide.transport.overview ? <Section title="怎么移动" eyebrow="TRANSPORT"><p className="travel-sidebar-copy">{guide.transport_summary || guide.transport.overview}</p>{guide.transport.tips.length > 0 ? <ul className="travel-sidebar-list">{guide.transport.tips.slice(0, 3).map((tip) => <li key={tip}>{tip}</li>)}</ul> : null}</Section> : null}{guide.local_tips.length > 0 ? <Section title="小贴士" eyebrow="NOTES"><ul className="travel-sidebar-list">{guide.local_tips.slice(0, 4).map((tip) => <li key={tip.title}><b>{tip.title}</b>{tip.text}</li>)}</ul></Section> : null}</aside></div>
    {(guide.warnings.length > 0 || guide.attractions.some((item) => item.opening_hours?.has_conflict || item.ticket?.has_conflict || item.reservation?.has_conflict)) ? <Section title="需要留意" eyebrow="ALERTS" className="travel-alert-section"><ul className="travel-warnings">{guide.warnings.slice(0, 4).map((warning) => <li key={warning.title}><Warning size={15} /><div><b>{warning.title}</b><p>{warning.text}</p></div></li>)}</ul>{guide.attractions.some((item) => item.opening_hours?.has_conflict || item.ticket?.has_conflict || item.reservation?.has_conflict) ? <p className="travel-conflict-note"><WarningCircle size={14} />部分硬事实存在多个版本，请以官方渠道为准。</p> : null}</Section> : null}
    {guide.sources.length > 0 ? <section className="travel-source-footer"><details><summary><span>信息来源 · {guide.sources.length}</span><b>数据可信度：{guide.evidence.quality || "有限"}</b><CaretDown size={15} /></summary><div className="travel-sources">{guide.sources.map((source, index) => <SourceRow key={`${source.url}-${index}`} source={source} />)}</div></details></section> : null}</article>;
}
