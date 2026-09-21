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
}

export interface DocumentDto {
  path: string;
  text: string;
}

export interface WorkspaceFile {
  path: string;
  relative_path: string;
}

/** Rust FeedDto(应用层 RSS DTO,snake_case 与设置保持一致) */
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

export type LanguageCode = "eng" | "jpn" | "cmn" | "yue";
export type LanguageItemType =
  | "WORD"
  | "PHRASE"
  | "SENTENCE"
  | "DIALOGUE"
  | "PASSAGE"
  | "GRAMMAR"
  | "PRONUNCIATION";
export type LearningStateKind = "new" | "learning" | "review" | "mastered";
export type ReviewRating = "again" | "hard" | "good" | "easy";
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
  stroke_count: number | null;
  grade: number | null;
  radical: number | null;
  jlpt: string | null;
}
export interface LicenseKindUnion {
  kind: string;
  attribution_required: boolean;
  commercial_use_allowed: boolean;
  redistribution_allowed: boolean;
  share_alike_required: boolean;
}
export interface LanguageSource {
  id: string;
  name: string;
  homepage: string;
  download_source: string;
  dataset_version: string;
  downloaded_at: number | null;
  license: LicenseKindUnion;
  license_url: string | null;
  attribution: string;
  commercial_use: boolean;
  redistribution: boolean;
  notes: string | null;
}
export interface LearningState {
  item_id: string;
  state: LearningStateKind;
  interval_days: number;
  ease: number;
  due_at: number;
  review_count: number;
  lapses: number;
  started_at: number;
  updated_at: number;
}
export interface WordDetail {
  item: LanguageItem;
  meanings: Meaning[];
  pronunciations: Pronunciation[];
  relations: RelationView[];
  examples: ExampleView[];
  sentences: SentenceRecord[];
  state: LearningState | null;
  favorite: boolean;
  source: LanguageSource | null;
  kanji: KanjiView | null;
}
export interface TodayPlan {
  due_reviews: number;
  new_words: number;
  sentences: number;
  listening: number;
  speaking: number;
  total: number;
}
export interface TodayView {
  language: string;
  plan: TodayPlan;
  languages: LanguageInfo[];
}
export interface ReviewCard {
  item: LanguageItem;
  state: LearningStateKind;
}
export interface ReviewOutcome {
  state: LearningStateKind;
  interval_days: number;
  ease: number;
  due_at: number;
  lapses: number;
}
export interface ProgressView {
  total: number;
  mastered: number;
  learning: number;
  reviews: number;
  favorites: number;
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
