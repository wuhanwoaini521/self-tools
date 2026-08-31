import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowSquareOut, Cloud, CloudRain, CloudSun, Snowflake, Star, Sun, Warning, WarningCircle } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import type { Attraction, CityGuide, SourceLevel, TravelDateRange, TravelSource, VerifiedValue, WeatherForecast } from "../../types";
import { formatDateTime, isTauriRuntime } from "../../utils";

/**
 * 结构化攻略展示（需求 #十四 / #十五 / #二十七）：
 * 每个区块缺失时显示「暂无可靠数据」，绝不编造。
 */

function Section({ title, children }: { title: string; children: ReactNode }) {
  return <section className="travel-section">
    <h2>{title}</h2>
    <div className="travel-section-body">{children}</div>
  </section>;
}

function EmptySection({ hint }: { hint?: string }) {
  return <p className="travel-empty">暂无可靠数据{hint ? `（${hint}）` : ""}</p>;
}

function VerifiedBadge({ value }: { value: VerifiedValue }) {
  return <span className="travel-verified" title={`${value.verified_sources} 个来源一致 · 主来源 ${value.primary_source}`}>
    {value.value}
    {value.has_conflict ? <i className="travel-conflict" title="信息存在多个版本，已按权威来源优先">⚠ 多版本</i>
      : <i title={`可信度 ${value.confidence}`}>✓ {value.confidence}</i>}
  </span>;
}

function Stars({ level }: { level: SourceLevel }) {
  const STARS: Record<SourceLevel, number> = { S: 5, A: 4, B: 3, C: 2 };
  const count = STARS[level] ?? 2;
  return <span className="travel-stars" aria-label={`${level} 级来源`}>
    {Array.from({ length: 5 }, (_, index) => <Star key={index} size={11} className={index < count ? "filled" : "blank"} weight="fill" />)}
  </span>;
}

function openLink(url: string) {
  if (isTauriRuntime()) void openUrl(url);
  else window.open(url, "_blank");
}

function AttractionBlock({ item }: { item: Attraction }) {
  const verified = [item.opening_hours, item.ticket, item.reservation]
    .filter((v): v is VerifiedValue => v !== null);
  return <div className="travel-attraction">
    <b>{item.name}</b>
    {item.area ? <em>{item.area}</em> : null}
    {item.intro ? <p>{item.intro}</p> : null}
    {item.suggested_duration ? <p>建议时长：{item.suggested_duration}</p> : null}
    {verified.length > 0 ? <ul className="travel-verified-list">
      {item.opening_hours ? <li>开放时间 <VerifiedBadge value={item.opening_hours} /></li> : null}
      {item.ticket ? <li>门票 <VerifiedBadge value={item.ticket} /></li> : null}
      {item.reservation ? <li>预约 <VerifiedBadge value={item.reservation} /></li> : null}
    </ul> : null}
    {item.tips.length > 0 ? <ul className="travel-tips-list">{item.tips.map((tip, index) => <li key={index}>{tip}</li>)}</ul> : null}
  </div>;
}

function SourceRow({ source }: { source: TravelSource }) {
  return <div className="travel-source">
    <div className="travel-source-main">
      <b title={source.url}>{source.title}</b>
      <span className="travel-source-meta">
        <Stars level={source.level} />
        <i className={"travel-level travel-level-" + source.level.toLowerCase()}>{source.level}</i>
        {source.state === "snippet_only" ? <i className="travel-snippet-tag" title="网页未能抓取全文，仅采用搜索摘要（低可信度）">仅搜索摘要</i> : null}
        {source.state === "unavailable" ? <i className="travel-snippet-tag">不可用</i> : null}
        <small>{source.host} · {formatDateTime(source.fetched_at)} · 评分 {(source.score * 100).toFixed(0)}</small>
      </span>
    </div>
    <button title={source.url} onClick={() => openLink(source.url)}>
      <ArrowSquareOut size={14} />查看来源
    </button>
  </div>;
}

function WeatherIcon({ text }: { text: string }) {
  if (/雨|雷/.test(text)) return <CloudRain size={23} weight="fill" />;
  if (/雪|冰雹/.test(text)) return <Snowflake size={23} weight="fill" />;
  if (/晴/.test(text)) return <Sun size={23} weight="fill" />;
  if (/多云|阴/.test(text)) return <CloudSun size={23} weight="fill" />;
  return <Cloud size={23} weight="fill" />;
}

function weekday(date: string) {
  const value = new Date(`${date}T00:00:00`);
  return Number.isNaN(value.getTime()) ? date : new Intl.DateTimeFormat("zh-CN", { weekday: "short" }).format(value);
}

function WeatherCard({ forecast, dateRange }: { forecast: WeatherForecast; dateRange: TravelDateRange | null }) {
  const current = forecast.days[0];
  const selectedDays = dateRange ? forecast.days.filter((day) => day.date >= dateRange.start && day.date <= dateRange.end) : [];
  return <div className="travel-weather-card">
    <header><div><small>{forecast.city} · 和风天气近期预报</small><strong>{current.temp_max}°</strong><p>{current.text_day} <span>最低 {current.temp_min}°</span></p></div><WeatherIcon text={current.text_day} /></header>
    <div className="travel-weather-days" role="list">
      {forecast.days.map((day) => <div className={dateRange && day.date >= dateRange.start && day.date <= dateRange.end ? "selected" : ""} key={day.date} role="listitem">
        <small>{weekday(day.date)}</small><WeatherIcon text={day.text_day} /><b>{day.temp_max}°</b><span>{day.temp_min}°</span><em>{day.text_day}</em>
      </div>)}
    </div>
    {dateRange && selectedDays.length === 0 ? <p className="travel-weather-note">所选 {dateRange.start} 至 {dateRange.end} 不在当前 3 天预报窗口；攻略已按日期范围检索活动、客流与季节建议。</p> : null}
  </div>;
}

function GuideMap({ attractions, apiKey }: { attractions: Attraction[]; apiKey: string | null | undefined }) {
  const points = attractions
    .flatMap((attraction) => attraction.coordinates ? [{ name: attraction.name, coordinates: attraction.coordinates }] : [])
    .slice(0, 10);
  if (!apiKey?.trim() || points.length === 0) return null;
  const staticMarkers = points.map((point, index) => {
    const label = index < 9 ? String(index + 1) : "A";
    return `mid,0x4F7DF3,${label}:${point.coordinates.longitude.toFixed(6)},${point.coordinates.latitude.toFixed(6)}`;
  }).join("|");
  const mapUrl = `https://restapi.amap.com/v3/staticmap?${new URLSearchParams({ size: "1024*420", scale: "2", traffic: "1", markers: staticMarkers, key: apiKey.trim() })}`;
  const markerUrl = `https://uri.amap.com/marker?${new URLSearchParams({ markers: points.map((point) => `${point.coordinates.longitude},${point.coordinates.latitude},${point.name}`).join("|"), src: "self-tools", callnative: "0" })}`;
  return <Section title="景点地图">
    <div className="travel-map-card">
      <img src={mapUrl} alt={`${points.map((point) => point.name).join("、")}的地图标记`} />
      <div className="travel-map-caption"><span>{points.length} 个来自高德 POI 的景点标记</span><button onClick={() => openLink(markerUrl)}><ArrowSquareOut size={14} />在高德地图中查看</button></div>
    </div>
  </Section>;
}

interface TravelGuideProps {
  guide: CityGuide;
  fromCache: boolean;
  amapApiKey?: string | null;
}

export function TravelGuide({ guide, fromCache, amapApiKey }: TravelGuideProps) {
  const { city, meta } = guide;
  const region = [city.province, city.country].filter(Boolean).join(" · ") || "中国";
  const itineraries = [guide.itineraries.one_day, guide.itineraries.two_days, guide.itineraries.three_days]
    .filter((it): it is NonNullable<typeof guide.itineraries.one_day> => it !== null);

  return <article className="travel-guide">
    <header className="travel-guide-header">
      <h1>{city.name}{city.name_en ? <span>{city.name_en}</span> : null}</h1>
      <p>{region}</p>
      <div className="travel-guide-tags">
        <b>{meta.days} 天行程</b>
        {meta.date_range ? <b>{meta.date_range.start} 至 {meta.date_range.end}</b> : null}
        {meta.llm_used ? <b>AI 整理</b> : <b className="travel-tag-warn">未配置 LLM · 来源列表模式</b>}
        {fromCache ? <b>缓存结果</b> : null}
        <i>更新于 {formatDateTime(meta.updated_at)}</i>
      </div>
      {meta.notes.length > 0 ? <ul className="travel-guide-notes">{meta.notes.map((note, index) => <li key={index}>{note}</li>)}</ul> : null}
    </header>

    <Section title="城市速览">
      {guide.summary ? <p className="travel-summary">{guide.summary}</p> : <EmptySection />}
      {guide.highlights.length > 0 ? <div className="travel-chips">{guide.highlights.map((h, index) => <span key={index}>{h}</span>)}</div> : null}
    </Section>

    <Section title="什么时候去">
      {guide.best_time ? <p>{guide.best_time}</p> : <EmptySection hint="未找到可靠信息" />}
    </Section>

    {guide.weather && guide.weather.days.length > 0 ? <Section title="近期天气"><WeatherCard forecast={guide.weather} dateRange={meta.date_range} /></Section> : null}

    <Section title="值得去">
      {guide.attractions.length > 0
        ? <div className="travel-grid">{guide.attractions.map((item, index) => <AttractionBlock key={index} item={item} />)}</div>
        : <EmptySection />}
    </Section>

    <GuideMap attractions={guide.attractions} apiKey={amapApiKey} />

    <Section title="城市区域">
      {guide.districts.length > 0
        ? <ul className="travel-list">{guide.districts.map((district, index) =>
          <li key={index}><b>{district.name}</b>{district.note ? <p>{district.note}</p> : null}{district.landmarks.length > 0 ? <small>{district.landmarks.join("、")}</small> : null}</li>)}</ul>
        : <EmptySection />}
    </Section>

    <Section title="吃什么">
      {guide.foods.length === 0 && guide.restaurants.length === 0 ? <EmptySection />
        : <div className="travel-food">
          {guide.foods.length > 0 ? <ul className="travel-list">{guide.foods.map((food, index) =>
            <li key={index}><b>{food.name}</b>{food.dish_type ? <em>{food.dish_type}</em> : null}{food.intro ? <p>{food.intro}</p> : null}</li>)}</ul> : null}
          {guide.restaurants.length > 0 ? <ul className="travel-list">{guide.restaurants.map((place, index) =>
            <li key={index}><b>{place.name}</b>{place.area ? <em>{place.area}</em> : null}{place.note ? <p>{place.note}</p> : null}</li>)}</ul> : null}
        </div>}
    </Section>

    <Section title="住哪里">
      {guide.accommodation_areas.length > 0
        ? <ul className="travel-list">{guide.accommodation_areas.map((area, index) =>
          <li key={index}><b>{area.name}</b>{area.area ? <em>{area.area}</em> : null}{area.budget ? <small>{area.budget}</small> : null}{area.note ? <p>{area.note}</p> : null}</li>)}</ul>
        : <EmptySection />}
    </Section>

    <Section title="交通">
      <div className="travel-transport">
        {guide.transport.overview ? <p>{guide.transport.overview}</p> : null}
        {guide.transport.airport ? <p>✈️ 机场：{guide.transport.airport}</p> : null}
        {guide.transport.train_station ? <p>🚄 高铁 / 火车站：{guide.transport.train_station}</p> : null}
        {guide.transport.metro ? <p>🚇 地铁：{guide.transport.metro}</p> : null}
        {guide.transport.bus_taxi ? <p>🚌 公交 / 打车：{guide.transport.bus_taxi}</p> : null}
        {guide.transport.tips.length > 0 ? <ul className="travel-tips-list">{guide.transport.tips.map((tip, index) => <li key={index}>{tip}</li>)}</ul> : null}
      </div>
      {!(guide.transport.overview || guide.transport.airport || guide.transport.train_station || guide.transport.metro || guide.transport.bus_taxi || guide.transport.tips.length > 0) ? <EmptySection /> : null}
    </Section>

    <Section title="推荐路线">
      {itineraries.length > 0
        ? <div className="travel-itineraries">{itineraries.map((itinerary) =>
          <ol className="travel-itinerary" key={itinerary.day}>
            <li className="travel-itinerary-head">{itinerary.title ?? `第 ${itinerary.day} 天`}</li>
            {itinerary.stops.map((stop, index) => <li key={index}><b>{stop.name}</b>{stop.note ? <span>{stop.note}</span> : null}</li>)}
          </ol>)}</div>
        : <EmptySection hint="LLM 未生成路线" />}
    </Section>

    <Section title="本地 Tips">
      {guide.local_tips.length > 0
        ? <ul className="travel-list">{guide.local_tips.map((tip, index) =>
          <li key={index}><b>{tip.title}</b><p>{tip.text}</p></li>)}</ul>
        : <EmptySection />}
    </Section>

    <Section title="注意事项">
      {guide.warnings.length > 0
        ? <ul className="travel-warnings">{guide.warnings.map((warning, index) =>
          <li key={index}><Warning size={15} /><b>{warning.title}</b><p>{warning.text}</p></li>)}</ul>
        : <EmptySection />}
      {guide.attractions.some((a) => a.opening_hours?.has_conflict || a.ticket?.has_conflict || a.reservation?.has_conflict)
        ? <p className="travel-conflict-note"><WarningCircle size={14} />部分信息存在多个来源版本，已按权威来源优先处理，请以官方渠道为准。</p> : null}
    </Section>

    <section className="travel-section travel-section-sources">
      <details className="travel-sources-details">
        <summary><h2>信息来源</h2><small>{guide.sources.length} 个来源 · 点击展开</small></summary>
        <div className="travel-section-body">
          {guide.sources.length > 0
            ? <div className="travel-sources">{guide.sources.map((source, index) => <SourceRow key={index} source={source} />)}</div>
            : <EmptySection />}
        </div>
      </details>
    </section>
  </article>;
}
