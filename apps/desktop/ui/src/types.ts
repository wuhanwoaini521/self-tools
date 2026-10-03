export type ThemeMode = "light" | "dark" | "system";
export type MarkdownView = "editor" | "split" | "preview";

/** 搜索后端（与 Rust TravelSearchBackend 的 snake_case 对应） */
export type TravelSearchBackend = "auto" | "searxng" | "baidu" | "bing";

export interface TravelSettings {
  search_backend: TravelSearchBackend;
  searxng_url: string | null;
  llm_base_url: string | null;
  llm_api_key: string | null;
  llm_model: string | null;
  amap_api_key: string | null;
  qweather_api_key: string | null;
  qweather_api_host: string | null;
  baidu_map_api_key: string | null;
}

export interface GeographySettings {
  amap_api_key: string | null;
  amap_security_js_code: string | null;
}

export interface AiSettings {
  provider: string | null;
  model: string | null;
  base_url: string | null;
  api_key: string | null;
  timeout_secs: number | null;
}

/** 允许根（文件 / 文档索引共用；`enabled=false` 只停用不删数据）。 */
export interface KnowledgeRoot {
  id: string;
  label: string;
  path: string;
  enabled: boolean;
}

/** Personal Knowledge 设置（V6；允许根为空时 Documents/Files 如实报告未配置）。 */
export interface KnowledgeSettings {
  /** 允许 AI 搜索 / 读取元数据 / 安全读取的文件根。 */
  file_roots: KnowledgeRoot[];
  /** 进入文档索引的根（空 = 复用 file_roots）。 */
  document_roots: KnowledgeRoot[];
  /** 单文档索引上限（字节）；超过则只索引元数据。 */
  max_document_bytes: number;
  /** 单次安全读取的字符上限。 */
  max_read_chars: number;
  /** 索引文件数上限。 */
  max_indexed_files: number;
  /** 启动时执行一次轻量同步。 */
  startup_sync: boolean;
}

export interface AppSettings {
  schema_version: number;
  recent_files: string[];
  workspace_path: string | null;
  theme_mode: ThemeMode;
  /** UI 风格主题 id,由前端 ThemeManager 注册表校验;未知值回退 Default */
  ui_theme: string;
  /** RSS 自动刷新间隔(分钟) */
  rss_refresh_minutes: number;
  editor_font_size: number;
  auto_save: boolean;
  markdown_default_view: MarkdownView;
  /** Travel 模块设置（全部可选，未配置时模块仍可用） */
  travel: TravelSettings;
  /** Geography 模块设置（全部可选，未配置时模块仍可用） */
  geography: GeographySettings;
  /** Personal AI 设置（全部可选，未配置时 AI Panel 显示未配置状态） */
  ai: AiSettings;
  /** Personal Knowledge 设置（V6；允许根为空时知识页如实报告未配置） */
  knowledge: KnowledgeSettings;
  /** Home Server 设置（V7；注册表为空 = 无能力，fail-closed） */
  server: ServerSettings;
}

/** 健康阈值（V7 §22）。 */
export interface ServerThresholds {
  disk_warn_ratio: number;
  disk_critical_ratio: number;
  memory_warn_ratio: number;
  cpu_warn_ratio: number;
}

/** 已注册服务（V7 §27；provider_ref 由 infrastructure 映射，不暴露给模型）。 */
export interface ServerServiceDescriptor {
  id: string;
  display_name: string;
  description: string;
  provider_type: "launchd" | "http" | "process" | "docker";
  provider_ref: string;
  health_check:
    | { kind: "none" }
    | { kind: "launchd" }
    | { kind: "http"; url: string };
  log_sources: {
    id: string;
    display_name: string;
    path: string;
  }[];
  allowed_actions: string[];
  tags: string[];
}

/** 已注册应用（V7 §43；URL 只允许 http/https）。 */
export interface ServerApplicationDescriptor {
  id: string;
  name: string;
  description: string;
  url: string;
  health_url?: string | null;
  service_id?: string | null;
  category: string;
  tags: string[];
}

/** MCP 传输设置（V8 §47；远程默认关闭）。 */
export interface McpSettings {
  enabled: boolean;
  stdio_enabled: boolean;
  http_enabled: boolean;
  bind: string;
  remote_enabled: boolean;
  port: number;
}

export interface ServerSettings {
  mcp: McpSettings;
  services: ServerServiceDescriptor[];
  applications: ServerApplicationDescriptor[];
  thresholds: ServerThresholds;
  confirmation_ttl_secs: number;
  cooldown_secs: number;
  max_system_per_session: number;
  audit_max_entries: number;
  audit_retention_days: number;
}

export interface DocumentDto {
  path: string;
  text: string;
}

export interface WorkspaceFile {
  path: string;
  relative_path: string;
}

/** Rust FeedDto(应用层 RSS DTO,snake_case 与设置保持一致)
 *  ADR-010：RSS 订阅**没有 kind** —— 新闻源不是 RSS 订阅的分类。 */
export interface FeedDto {
  id: number;
  title: string;
  url: string;
  site_url: string | null;
  unread_count: number;
  last_updated: number | null;
  last_error: string | null;
}

export interface ArticleDto {
  id: number;
  feed_id: number;
  feed_title: string;
  title: string;
  url: string;
  published_at: number | null;
  summary: string | null;
  is_read: boolean;
}

export interface RefreshReport {
  new_articles: number;
  failures: { feed_title: string; message: string }[];
}

// ---------- News（V12 / ADR-010：独立 bounded context） ----------

/** 新闻源摄取方式（技术属性，不是产品分类）。 */
export type NewsSourceType = "rss" | "atom" | "json_feed" | "api";

/** 新闻分类（News 页栏目）。 */
export type NewsCategoryId = "general" | "tech" | "finance" | "world" | "china";

/** 新闻源（news.db：系统 seed + 用户添加；**不是 RSS 订阅**）。 */
export interface NewsSource {
  id: number;
  name: string;
  url: string;
  source_type: NewsSourceType;
  category: NewsCategoryId;
  category_label: string;
  site_url: string | null;
  last_updated: number | null;
  last_error: string | null;
  unread_count: number;
}

/** 新闻文章（news.db）。 */
export interface NewsArticle {
  id: number;
  source_id: number;
  source: string;
  title: string;
  url: string;
  author: string | null;
  image_url: string | null;
  published_at: number | null;
  summary: string | null;
  is_read: boolean;
  starred: boolean;
}

/** 推荐源候选（后端 `core::news::recommended_sources()` 的镜像）。 */
export interface RecommendedSource {
  name: string;
  url: string;
  site_url: string | null;
  category: NewsCategoryId;
  category_label: string;
  note: string;
}

/** `news_sources` 返回的聚合。 */
export interface NewsOverview {
  sources: NewsSource[];
  health: "healthy" | "degraded";
}

// ---------- Travel ----------

export type ContentState = "full" | "snippet_only" | "unavailable";
export type SourceLevel = "S" | "A" | "B" | "C";
export type ResearchPhase =
  | "identify_city"
  | "plan_queries"
  | "search"
  | "fetch_documents"
  | "extract_facts"
  | "data_sources"
  | "rank_sources"
  | "validate_facts"
  | "generate_guide"
  | "save_guide";
export type StepStatus =
  | "pending"
  | "in_progress"
  | "done"
  | "failed"
  | "skipped";

export interface TravelResearchEvent {
  phase: ResearchPhase;
  status: StepStatus;
  message: string;
  seq: number;
}

export interface TravelDateRange {
  start: string;
  end: string;
}

export interface TravelResearchRequest {
  city: string;
  natural_language?: string | null;
  today?: string | null;
  arrival?: string | null;
  departure?: string | null;
  days: number;
  month: number | null;
  date_range: TravelDateRange | null;
  preferences: string[];
  force: boolean;
}

export interface TravelResearchSnapshot {
  session_id: string;
  done: boolean;
  error: string | null;
  from_cache: boolean;
  events: TravelResearchEvent[];
  guide: CityGuide | null;
}

export interface GuideSummary {
  city: string;
  days: number;
  updated_at: number;
  date_range: TravelDateRange | null;
}

export interface CityInfo {
  name: string;
  name_en: string | null;
  province: string | null;
  country: string | null;
}

export interface VerifiedValue {
  value: string;
  confidence: string;
  verified_sources: number;
  primary_source: string;
  has_conflict: boolean;
  verified: boolean;
}

export interface Attraction {
  id: string | null;
  name: string;
  normalized_name: string | null;
  poi_id: string | null;
  intro: string | null;
  why_go: string | null;
  why_for_this_trip: string | null;
  area: string | null;
  suggested_duration: string | null;
  opening_hours: VerifiedValue | null;
  ticket: VerifiedValue | null;
  reservation: VerifiedValue | null;
  tips: string[];
  best_for: string[];
  recommended_day: number | null;
  open_status: VerifiedValue | null;
  confidence: string | null;
  source_ids: string[];
  coordinates: MapCoordinates | null;
}

export interface MapCoordinates {
  longitude: number;
  latitude: number;
}

export interface Food {
  name: string;
  dish_type: string | null;
  intro: string | null;
  area: string | null;
  source_ids: string[];
}

export interface Place {
  name: string;
  area: string | null;
  note: string | null;
  signature_dish: string | null;
  why_pick: string | null;
  route_day: number | null;
  distance_to_route: string | null;
  confidence: string | null;
  poi_id: string | null;
  coordinates: MapCoordinates | null;
}

export interface DistrictInfo {
  name: string;
  note: string | null;
  landmarks: string[];
}

export interface TransportGuide {
  overview: string | null;
  airport: string | null;
  train_station: string | null;
  metro: string | null;
  bus_taxi: string | null;
  tips: string[];
}

export interface AccommodationArea {
  name: string;
  area: string | null;
  note: string | null;
  budget: string | null;
}

export interface ItineraryStop {
  name: string;
  note: string | null;
  time: string | null;
  duration: string | null;
  area: string | null;
  reason: string | null;
  travel_time: string | null;
}

export interface Itinerary {
  day: number;
  title: string | null;
  stops: ItineraryStop[];
}

export interface Itineraries {
  one_day: Itinerary | null;
  two_days: Itinerary | null;
  three_days: Itinerary | null;
}

export interface ItineraryDay {
  day: number;
  title: string | null;
  theme: string | null;
  stops: ItineraryStop[];
}

export interface QuickDecisions {
  best_area_to_stay: string | null;
  signature_food: string | null;
  trip_style: string | null;
  must_visit: string[];
  main_warning: string | null;
}

export interface EvidenceSummary {
  source_count: number;
  verified_count: number;
  snippet_only_count: number;
  conflict_count: number;
  quality: string;
}

export interface TravelTip {
  title: string;
  text: string;
}

export interface TravelWarning {
  title: string;
  text: string;
}

export interface TravelSource {
  url: string;
  title: string;
  host: string;
  level: SourceLevel;
  state: ContentState;
  published_at: number | null;
  fetched_at: number;
  score: number;
}

export interface GuideMeta {
  generated_at: number;
  updated_at: number;
  days: number;
  date_range: TravelDateRange | null;
  llm_used: boolean;
  notes: string[];
}

export interface WeatherDay {
  date: string;
  text_day: string;
  temp_min: string;
  temp_max: string;
}

export interface WeatherForecast {
  city: string;
  days: WeatherDay[];
}

export interface CityGuide {
  city: CityInfo;
  summary: string;
  highlights: string[];
  best_time: string | null;
  weather: WeatherForecast | null;
  districts: DistrictInfo[];
  attractions: Attraction[];
  foods: Food[];
  restaurants: Place[];
  transport: TransportGuide;
  accommodation_areas: AccommodationArea[];
  itineraries: Itineraries;
  local_tips: TravelTip[];
  warnings: TravelWarning[];
  quick_decisions: QuickDecisions;
  top_picks: Attraction[];
  alternatives: Attraction[];
  itinerary_days: ItineraryDay[];
  food_summary: string | null;
  stay_areas: AccommodationArea[];
  transport_summary: string | null;
  evidence: EvidenceSummary;
  sources: TravelSource[];
  meta: GuideMeta;
}

// ---------- History ----------

export interface SemanticHistoryHome {
  periods: {
    id: string;
    name_zh_cn: string;
    start_year: number | null;
    end_year: number | null;
  }[];
  stories: {
    id: string;
    title_zh_cn: string;
    summary_zh_cn?: string | null;
    usable?: boolean | null;
  }[];
  stats?: {
    people: number;
    places: number;
    works: number;
    events: number;
    periods: number;
    regimes: number;
    stories: number;
    event_relations: number;
    event_evidences: number;
    person_relations: number;
    historical_texts: number;
  } | null;
}

// ---------- Geography Explorer ----------

export type GeoEntityType =
  | "world"
  | "country"
  | "region"
  | "province"
  | "city"
  | "river"
  | "mountain"
  | "mountain_range"
  | "plateau"
  | "plain"
  | "basin"
  | "desert"
  | "lake"
  | "ocean"
  | "sea"
  | "island"
  | "archipelago"
  | "climate_zone"
  | "tectonic_plate";
export type CoordinateSystem = "WGS84" | "GCJ02" | "BD09";
export type GeoRelationKind =
  | "LOCATED_IN"
  | "PART_OF"
  | "BORDERS"
  | "FLOWS_THROUGH"
  | "SOURCE_OF"
  | "FLOWS_INTO"
  | "CONNECTED_TO"
  | "NEAR"
  | "CROSSES"
  | "SURROUNDS"
  | "AFFECTS"
  | "INFLUENCES"
  | "FORMED_BY"
  | "CAUSES"
  | "BELONGS_TO_CLIMATE_ZONE";
export type GeoRecommendationKind = "entity" | "question";

export interface GeoCoordinate {
  system: CoordinateSystem;
  latitude: number;
  longitude: number;
}
export interface GeoProperty {
  key: string;
  label: string;
  value: string;
  unit: string | null;
  source_ids: string[];
}
export interface GeoEntity {
  id: string;
  entity_type: GeoEntityType;
  name: string;
  name_en: string | null;
  aliases: string[];
  coordinates: GeoCoordinate | null;
  geometry: unknown;
  parent_id: string | null;
  properties: GeoProperty[];
  summary: string;
  source_ids: string[];
}
export interface GeoRelation {
  from_id: string;
  to_id: string;
  kind: GeoRelationKind;
  note: string | null;
  source_ids: string[];
}
export interface GeoSource {
  id: string;
  dataset: string;
  version: string;
  url: string;
  license: string;
  updated_at: string;
  fields: string[];
}
export interface GeoRelationView {
  relation: GeoRelation;
  entity: GeoEntity;
}
export interface GeoEntityDetail {
  entity: GeoEntity;
  relations: GeoRelationView[];
  sources: GeoSource[];
  favorite: boolean;
}
export interface GeoSearchGroup {
  entity_type: GeoEntityType;
  items: GeoEntity[];
}
export interface GeoRecommendation {
  kind: GeoRecommendationKind;
  title: string;
  question: string;
  entity_id: string;
  tags: string[];
}
export interface GeoMapPoint {
  entity_id: string;
  name: string;
  entity_type: GeoEntityType;
  coordinate: GeoCoordinate;
}
export interface GeoMapLine {
  from_id: string;
  to_id: string;
  kind: GeoRelationKind;
  from: GeoCoordinate;
  to: GeoCoordinate;
}
export interface GeographyHome {
  recommendation: GeoRecommendation;
  featured: GeoEntity[];
  recent: GeoEntity[];
  favorite_ids: string[];
  map_points: GeoMapPoint[];
  map_lines: GeoMapLine[];
}

// ---------- Language ----------
// 逐字段对应 crates/core/src/language/** 的 serde 输出。

export type LanguageCode = "eng" | "jpn" | "cmn" | "yue";
export type LanguageItemType =
  | "WORD"
  | "PHRASE"
  | "SENTENCE"
  | "DIALOGUE"
  | "PASSAGE"
  | "GRAMMAR"
  | "PRONUNCIATION";
/** 学习层条目类型（进入学习闭环的四类）。 */
export type LearningItemType = "word" | "phrase" | "sentence" | "article";
export type Difficulty = "unknown" | "easy" | "medium" | "hard";
export type PronunciationScheme =
  | "ARPABET"
  | "IPA"
  | "PINYIN"
  | "JYUTPING"
  | "KANA"
  | "ROMAJI";

export interface LanguageItem {
  id: string;
  language: LanguageCode;
  item_type: LanguageItemType;
  text: string;
  reading: string | null;
  romanization: string | null;
  meta: unknown;
  source: string;
}

/** 学习层统一条目（word / phrase / sentence / article 同一形状）。 */
export interface LanguageLearningItem {
  id: string;
  type: LearningItemType;
  language: LanguageCode;
  content: string;
  translation: string | null;
  pronunciation: string | null;
  romanization: string | null;
  difficulty: Difficulty;
  tags: string[];
  source: string;
}

export interface LanguageSearchHit {
  item: LanguageItem;
  matched: string;
}
export interface LanguageInfo {
  code: string;
  name: string;
  native_name: string;
  words: number;
  phrases: number;
  sentences: number;
  total: number;
}
export interface Meaning {
  id: string;
  item_id: string;
  pos: string | null;
  gloss: string | null;
  raw: string | null;
  sense_key: string | null;
  lang: string | null;
  rank: number;
  source: string;
}
export interface Pronunciation {
  id: string;
  item_id: string;
  scheme: PronunciationScheme;
  phonemes: string;
  tone: number | null;
  variant: string | null;
  source: string;
}
export interface LanguageRelation {
  id: string;
  from_item_id: string;
  to_item_id: string;
  kind: string;
  note: string | null;
  source: string;
}
export interface RelationView {
  relation: LanguageRelation;
  item: LanguageItem;
  label: string;
}
export interface ExampleView {
  text: string;
  translation: string | null;
  source: string;
}
export interface SentenceRecord {
  sentence_id: string;
  language: LanguageCode;
  text: string;
  author: string | null;
  license: string;
  source: string;
}
export interface KanjiView {
  readings: string[];
  meanings: string[];
  stroke_count: number | null;
  grade: number | null;
  jlpt: number | null;
  frequency_rank: number | null;
}
export interface SourceLicense {
  kind: string;
  attribution_required: boolean;
  commercial_use: boolean;
  redistribution: boolean;
  share_alike: boolean;
}
export interface LanguageSource {
  id: string;
  name: string;
  homepage: string;
  download_source: string;
  dataset_version: string;
  downloaded_at: number | null;
  license: SourceLicense;
  license_url: string | null;
  attribution: string;
  commercial_use: boolean;
  redistribution: boolean;
  notes: string | null;
}
export interface WordDetail {
  item: LanguageItem;
  meanings: Meaning[];
  pronunciations: Pronunciation[];
  relations: RelationView[];
  examples: ExampleView[];
  sentences: SentenceRecord[];
  source: LanguageSource | null;
  kanji: KanjiView | null;
}

// ---------- 学习卡片（进 Language 直接给这个）----------

/** 一件今天该学的东西。 */
export interface StudyCard {
  item: LanguageLearningItem;
  /** 来自平台复习队列（已学过，到期该复习）。 */
  from_review: boolean;
  /** 复习卡 id；新内容没有卡（第一次作答后才建卡）。 */
  card_id: string | null;
}

// ---------- Lesson ----------

export interface LessonStep {
  item_id: string;
  type: LearningItemType;
  content: string;
  translation: string | null;
}
export interface Lesson {
  id: string;
  title: string;
  language: LanguageCode;
  description: string | null;
  steps: LessonStep[];
  created_at: number;
  updated_at: number;
}
/** Lesson + 恢复位置 + 每步的完整学习条目。 */
export interface LessonView extends Lesson {
  step_index: number;
  items: LanguageLearningItem[];
}
export interface ContinueLesson {
  lesson: Lesson;
  step_index: number;
  completed_steps: number;
  total_steps: number;
  last_studied_at: number;
}

// ---------- 错题 ----------

export interface Mistake {
  id: string;
  item_id: string;
  type: LearningItemType;
  language: LanguageCode;
  content: string;
  question: string;
  user_answer: string;
  correct_answer: string;
  error_count: number;
  last_missed_at: number;
}

// ---------- 句子学习 ----------

export interface SentenceChunk {
  text: string;
  item_id: string | null;
  meaning: string | null;
  reading: string | null;
}
export interface SentenceStudy {
  id: string;
  language: LanguageCode;
  original: string;
  translation: string | null;
  reading: string | null;
  romanization: string | null;
  chunks: SentenceChunk[];
  key_words: string[];
  grammar: string | null;
  usage: string | null;
  license: string | null;
  author: string | null;
}

// ---------- 进度（来自平台 learning_progress） ----------

/** 薄弱项：掌握度低且最近学过。 */
export interface WeakItem {
  entity_id: string;
  entity_type: string;
  content: string;
  translation: string | null;
  mastery_score: number;
  incorrect_count: number;
  status: LearningStatus;
  difficulty: Difficulty;
  last_studied_at: number;
}

export interface DatasetManifest {
  id: string;
  name: string;
  language: string;
  version: string;
  downloaded_at: number | null;
  source_id: string;
  checksum: string | null;
  raw_file: string | null;
  record_count: number;
  importer_version: number;
  imported_at: number;
}
export interface SourceInfo {
  source: LanguageSource;
  item_count: number;
  manifest: DatasetManifest | null;
}
export interface DatasetReport {
  id: string;
  name: string;
  inserted: number;
  updated: number;
}
export interface StarterReport {
  datasets: DatasetReport[];
  total_inserted: number;
  total_updated: number;
}
export interface SpeakingScore {
  accuracy: number;
  completeness: number;
  fluency: number;
}

// ==================== Learning OS Types (V11) ====================
// 以下类型逐字段对应 `crates/core/src/learning/model.rs` 的 serde 输出
// （snake_case 字段名、snake_case 枚举值）。此前此处是一套自造形状
// （`title`/`correct_streak`/`ease_factor`/`card_id`…），与后端没有一个字段对得上，
// 导致 `learning_record_event` 在全部 8 个调用点反序列化失败，学习事件从未落库。

export type LearningActionKind =
  | "view"
  | "study"
  | "complete"
  | "review"
  | "answer"
  | "correct"
  | "incorrect"
  | "bookmark"
  | "note"
  | "ask_ai"
  // `LearningAction::Custom(String)` 的未标记分支：任何其它字符串按原样落库。
  | (string & {});

export interface LearningEvent {
  /** 可留空：由后端按 `module:entity_type:entity_id:timestamp` 生成稳定主键。 */
  id?: string;
  module: string;
  entity_type: string;
  entity_id: string;
  entity_title?: string | null;
  action: LearningActionKind;
  /** 可留空（0）：由后端取当前时间。 */
  timestamp?: number;
  duration_ms?: number | null;
  metadata?: Record<string, unknown>;
  source?: string | null;
}

export type LearningStatus =
  | "not_started"
  | "learning"
  | "familiar"
  | "mastered";

export interface LearningProgress {
  entity_key: string;
  module: string;
  entity_type: string;
  entity_id: string;
  entity_title: string;
  status: LearningStatus;
  study_count: number;
  review_count: number;
  correct_count: number;
  incorrect_count: number;
  /** 0..100，确定性加权（正确率 / 深度 / 间隔 / 新鲜度）。 */
  mastery_score: number;
  last_studied_at: number;
  next_review_at: number | null;
  interval_days: number;
  ease: number;
  custom_tags: string[];
}

export type UniversalReviewCardType =
  | "recall"
  | "multiple_choice"
  | "qa"
  | "map_locate"
  | "fill_blank";

export type UniversalReviewRating = "again" | "hard" | "good" | "easy";

export interface UniversalReviewCard {
  id: string;
  module: string;
  entity_id: string;
  entity_type: string;
  card_type: UniversalReviewCardType;
  prompt: string;
  answer: string;
  options: string[] | null;
  hint: string | null;
  context: string | null;
  due_at: number;
  interval_days: number;
  ease: number;
  mastery_score: number;
  repetition_count: number;
  lapses: number;
  last_reviewed_at: number | null;
  created_at: number;
}

export interface ReviewQueueItem {
  card: UniversalReviewCard;
  is_overdue: boolean;
  urgency_score: number;
}

export interface ReviewQueueStats {
  total_due: number;
  due_count: number;
  overdue_count: number;
  upcoming_count: number;
  by_module: Record<string, number>;
  mastered_count: number;
  learning_count: number;
  total_cards: number;
}

export interface ReviewScheduleOutcome {
  interval_days: number;
  ease: number;
  due_at: number;
  repetition_count: number;
  lapses: number;
  is_correct: boolean;
}

export type GraphEntityType =
  | "person"
  | "place"
  | "event"
  | "time"
  | "concept"
  | "article"
  | "language"
  | "topic";

export type GraphRelationKind =
  | "located_in"
  | "occurred_at"
  | "participated_in"
  | "created_by"
  | "mentions"
  | "references"
  | "parent_of"
  | "related_to";

export interface GraphNode {
  id: string;
  name: string;
  entity_type: GraphEntityType;
  module: string;
  summary?: string | null;
  degree?: number;
  mastery_score?: number;
  learning_status?: LearningStatus;
}

export interface GraphEdge {
  id?: string;
  source_id?: string;
  target_id?: string;
  source?: string;
  target?: string;
  relation_kind?: GraphRelationKind;
  relation?: GraphRelationKind;
  label?: string | null;
  weight?: number;
  source_module?: string;
}

export interface GraphNeighborhood {
  root_id?: string | null;
  center?: GraphNode;
  nodes: GraphNode[];
  edges: GraphEdge[];
  hops?: number;
  total_nodes?: number;
  total_edges?: number;
}

export interface CollectionItem {
  id: string;
  collection_id: string;
  module: string;
  entity_type: string;
  entity_id: string;
  title: string;
  note?: string | null;
  created_at: number;
}

export interface Collection {
  id: string;
  title: string;
  description?: string | null;
  tags: string[];
  items_count: number;
  created_at: number;
  updated_at: number;
}

export interface ContinueItem {
  module: string;
  entity_type: string;
  entity_id: string;
  title: string;
  subtitle?: string | null;
  progress_percent?: number | null;
  last_studied_at: number;
  action_target: string;
}

export interface ExploreRecommendation {
  id: string;
  title: string;
  summary: string;
  module: string;
  entity_type: string;
  entity_id: string;
  reason: string;
  connected_entity_title?: string | null;
  tags: string[];
}

export interface TodayDashboardData {
  date_str: string;
  greeting: string;
  studied_topics_today: number;
  pending_reviews_count: number;
  average_mastery: number;
  recent_streak_days: number;
  continue_items: ContinueItem[];
  review_stats: ReviewQueueStats;
  explore_recommendations: ExploreRecommendation[];
  today_news_summary?: string | null;
  recent_collections: Collection[];
  recent_bookmarks: ContinueItem[];
}


// ---------- 英语课程（NCE 主课程） ----------
//
// 对应 `crates/core/src/language/course.rs`。命名保持与 Rust 侧一致（snake_case
// 序列化），前端只补 UI 需要的联合类型，不在此处派生领域规则。

export type LessonStatus = "not_started" | "learning" | "completed" | "review";

export type LessonStage =
  | "vocabulary"
  | "listen"
  | "read"
  | "sentence"
  | "shadow"
  | "quiz"
  | "done";

export interface CourseBook {
  id: string;
  course_id: string;
  book_no: number;
  title: string;
  subtitle: string | null;
  total_lessons: number;
}

export interface CourseLesson {
  id: string;
  book_id: string;
  lesson_no: number;
  title: string;
  audio_path: string | null;
  duration_ms: number | null;
  sentence_count: number;
  vocab_count: number;
}

export interface LessonListEntry {
  id: string;
  book_id: string;
  lesson_no: number;
  title: string;
  audio_path: string | null;
  duration_ms: number | null;
  sentence_count: number;
  vocab_count: number;
  status: LessonStatus;
  percent: number;
}

export interface BookSummary {
  total_lessons: number;
  completed_lessons: number;
  learning_lessons: number;
  study_seconds: number;
  vocab_total: number;
}

export interface BookView {
  book: CourseBook;
  summary: BookSummary;
  lessons: LessonListEntry[];
}

export interface LessonSentence {
  id: string;
  lesson_id: string;
  sequence: number;
  start_ms: number;
  end_ms: number;
  english: string;
  chinese: string | null;
}

export interface LessonVocab {
  lesson_id: string;
  word: string;
  surface: string | null;
  sentence_id: string | null;
  context: string | null;
  phonetic: string | null;
  pos: string | null;
  translation_zh: string | null;
  definition_en: string | null;
  frequency: number;
  tags: string[];
  importance: number;
}

/** 生词 + 用户状态（平台进度推导：new / learning / known）。 */
export interface VocabWithState extends LessonVocab {
  state: "new" | "learning" | "known";
  seen_count: number;
}

export interface LessonProgress {
  lesson_id: string;
  stage: LessonStage;
  position_ms: number;
  sentence_seq: number;
  vocab_index: number;
  shadow_seq: number;
  quiz_score: number | null;
  completed_at: number | null;
  study_seconds: number;
  updated_at: number;
}

export interface LessonDetail {
  lesson: CourseLesson;
  book: CourseBook | null;
  sentences: LessonSentence[];
  vocab: VocabWithState[];
  progress: LessonProgress;
}

export interface LearningPlan {
  language: LanguageCode;
  course_id: string | null;
  book_id: string | null;
  daily_minutes: number;
  new_words_per_day: number;
  updated_at: number;
}

export interface TodayDashboard {
  imported: boolean;
  dict_ready: boolean;
  plan: LearningPlan | null;
  continue_lesson: LessonListEntry | null;
  next_lesson: LessonListEntry | null;
  current_book: CourseBook | null;
  book_summary: BookSummary | null;
  due_reviews: number;
  study_seconds_today: number;
  streak_days: number;
  words_learned: number;
  recent_lessons: LessonListEntry[];
}

export interface WordEntry {
  word: string;
  lemma: string;
  phonetic: string | null;
  pos: string | null;
  translation_zh: string | null;
  definition_en: string | null;
  frequency: number;
  bnc: number;
  tags: string[];
  collins: number;
  forms: Array<[string, string]>;
}

export interface WordOccurrence {
  word: string;
  source_type: string;
  source_id: string;
  sentence: string | null;
  occurred_at: number;
}

export interface WordLookup {
  entry: WordEntry | null;
  seen_count: number;
  occurrences: WordOccurrence[];
  learning: LearningProgress | null;
}

export type WordMark = "know" | "fuzzy" | "unknown";

export type QuizItem =
  | {
      kind: "vocabulary";
      word: string;
      phonetic: string | null;
      options: string[];
      answer: number;
    }
  | {
      kind: "fill_blank";
      sentence: string;
      chinese: string | null;
      answer: string;
    }
  | {
      kind: "dictation";
      lesson_id: string;
      sentence_seq: number;
      start_ms: number;
      end_ms: number;
      answer: string;
    }
  | { kind: "translate"; chinese: string; reference: string };

export interface QuizAnswer {
  item_index: number;
  correct: boolean;
  user_answer: string | null;
}

export interface QuizResult {
  lesson_id: string;
  total: number;
  correct: number;
  score: number;
  wrong_words: string[];
  finished_at: number;
}

export interface ProgressPatch {
  stage?: LessonStage | null;
  position_ms?: number | null;
  sentence_seq?: number | null;
  vocab_index?: number | null;
  shadow_seq?: number | null;
  study_seconds_delta?: number | null;
}

export interface EnglishProgress {
  lessons_completed: number;
  lessons_learning: number;
  study_seconds_total: number;
  words_learned: number;
  words_mastered: number;
  review_mastery: number;
  due_reviews: number;
  streak_days: number;
  books: Array<{ book: CourseBook; summary: BookSummary }>;
}

export interface EnglishSearchResult {
  words: WordEntry[];
  lessons: CourseLesson[];
}

// ---------- 导入 ----------

export interface NceLessonScan {
  lesson_no: number;
  title: string;
  has_lrc: boolean;
  has_audio: boolean;
}

export interface NceBookScan {
  book_no: number;
  folder: string;
  lessons: NceLessonScan[];
}

export interface NceScanReport {
  books: NceBookScan[];
  total_lessons: number;
  issues: string[];
}

export interface NceImportProgress {
  stage: string;
  book_no: number;
  lesson_no: number;
  done: number;
  total: number;
  message: string;
}

export interface NceImportReport {
  books: number;
  lessons: number;
  sentences: number;
  vocab: number;
  media_files: number;
  media_bytes: number;
  skipped: number;
  cancelled: boolean;
  issues: string[];
}

export interface DictImportReport {
  entries: number;
  skipped: number;
  cancelled: boolean;
}

export interface DictStatus {
  ready: boolean;
  count: number;
}
