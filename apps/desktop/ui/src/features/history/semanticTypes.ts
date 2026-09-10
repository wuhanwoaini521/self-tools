export interface SemanticPeriod {
  id: string;
  name_zh_cn: string;
  name_raw?: string | null;
  start_year: number | null;
  end_year: number | null;
  date_precision?: string | null;
  description_zh_cn?: string | null;
  quality_status?: string | null;
  source_type?: string | null;
  source_ids?: string | null;
}

export interface SemanticRegime {
  id: string;
  name_zh_cn: string;
  start_year: number | null;
  end_year: number | null;
  description_zh_cn?: string | null;
  quality_status?: string | null;
}

/** 时期页事件列表行（后端 PeriodEventItem）。 */
export interface SemanticPeriodEvent {
  id: string;
  name_zh_cn: string;
  event_type?: string | null;
  start_year: number | null;
  end_year: number | null;
  importance?: string | null;
  summary_zh_cn?: string | null;
  result_zh_cn?: string | null;
  people_count: number;
  relation_count: number;
  evidence_count: number;
}

/** 时期页核心人物（按参与事件数倒序）。 */
export interface SemanticPeriodPerson {
  person_id: string;
  canonical_name_zh_cn: string;
  event_count: number;
}

/** 全库统计（DatasetStats 镜像）。 */
export interface SemanticStats {
  people: number;
  places: number;
  person_relations: number;
  person_places: number;
  works: number;
  historical_texts: number;
  events: number;
  periods: number;
  regimes: number;
  stories: number;
  event_relations: number;
  event_evidences: number;
}

export interface SemanticStory {
  id: string;
  title_zh_cn: string;
  start_year: number | null;
  end_year: number | null;
  summary_zh_cn?: string | null;
  background_zh_cn?: string | null;
  result_zh_cn?: string | null;
  story_type?: string | null;
  importance?: string | null;
  period_id?: string | null;
  quality_status?: string | null;
  source_type?: string | null;
  source_ids?: string | null;
  usable?: boolean | null;
}

export interface SemanticStoryEvent {
  story_id: string;
  event_id: string;
  sequence: number | null;
  role?: string | null;
  importance?: string | null;
  transition_text_zh_cn?: string | null;
  quality_status?: string | null;
  name_zh_cn: string;
  event_type?: string | null;
  start_year: number | null;
  end_year: number | null;
  date_precision?: string | null;
  summary_zh_cn?: string | null;
  result_zh_cn?: string | null;
  event_quality_status?: string | null;
  source_type?: string | null;
  source_ids?: string | null;
}

export interface SemanticEvent {
  id: string;
  name_zh_cn: string;
  event_type?: string | null;
  start_year: number | null;
  end_year: number | null;
  date_precision?: string | null;
  period_id?: string | null;
  regime_id?: string | null;
  summary_zh_cn?: string | null;
  background_zh_cn?: string | null;
  process_zh_cn?: string | null;
  result_zh_cn?: string | null;
  impact_zh_cn?: string | null;
  importance?: string | null;
  quality_status?: string | null;
  source_type?: string | null;
  source_ids?: string | null;
}

export interface SemanticEventPerson {
  event_id: string;
  person_id: string;
  role: string;
  role_zh_cn?: string | null;
  side?: string | null;
  importance?: string | null;
  source_id?: string | null;
  quality_status?: string | null;
  person_name?: string | null;
  description?: string | null;
  link_quality_status?: string | null;
  link_confidence?: number | null;
  link_reason?: string | null;
  birth_year?: number | null;
  death_year?: number | null;
  person_quality_status?: string | null;
}

export interface SemanticEventPlace {
  event_id: string;
  place_id?: string | null;
  place_name_raw?: string | null;
  role?: string | null;
  sequence?: number | null;
  source_id?: string | null;
  quality_status?: string | null;
  link_status?: string | null;
  place_name?: string | null;
  description_zh_cn?: string | null;
  link_quality_status?: string | null;
  link_confidence?: number | null;
  link_reason?: string | null;
  historical_name?: string | null;
  modern_name?: string | null;
  longitude?: number | null;
  latitude?: number | null;
}

export interface SemanticEventRelation {
  source_event_id: string;
  target_event_id: string;
  relation_type: string;
  confidence?: number | null;
  description_zh_cn?: string | null;
  source_id?: string | null;
  quality_status?: string | null;
  source_event_name?: string | null;
  target_event_name?: string | null;
}

export interface SemanticHistoricalText {
  event_id: string;
  historical_text_id: string;
  role?: string | null;
  sequence?: number | null;
  source_id?: string | null;
  quality_status?: string | null;
  title_zh_cn?: string | null;
  work_title?: string | null;
  chapter?: string | null;
  original_text?: string | null;
  original_simplified?: string | null;
  translation_zh_cn?: string | null;
  source_quality_status?: string | null;
  link_quality_status?: string | null;
  link_confidence?: number | null;
  link_reason?: string | null;
  translation_source?: string | null;
  alignment_quality?: string | null;
}

/** Backbone V2 的章节级史料证据（event_evidence）：作品 / 篇目 / 术语 + 复核状态。 */
export interface SemanticEventEvidence {
  id: string;
  event_id: string;
  historical_text_id?: string | null;
  work?: string | null;
  term?: string | null;
  chapter_hint?: string | null;
  context_keywords?: string | null;
  evidence_role?: string | null;
  link_status?: string | null;
  link_quality_status?: string | null;
  link_confidence?: number | null;
  review_note?: string | null;
  source_type?: string | null;
  source_id?: string | null;
  quality_status?: string | null;
  rejected_text_ids?: string | null;
}

export interface SemanticSource {
  id: string;
  dataset?: string | null;
  snapshot_version?: string | null;
  dataset_version?: string | null;
  source_type?: string | null;
  license?: string | null;
  quality?: string | null;
  quality_status?: string | null;
  original_url?: string | null;
}

export interface SemanticPerson {
  id: string;
  canonical_name_zh_cn: string;
  name_raw?: string | null;
  birth_year?: number | null;
  death_year?: number | null;
  gender?: string | null;
  quality_status?: string | null;
  created_from_source?: string | null;
  intro_zh_cn?: string | null;
}

export interface SemanticPersonRelation {
  person_a_id: string;
  person_a_name?: string | null;
  person_b_id: string;
  person_b_name?: string | null;
  relation_type: string;
  start_year?: number | null;
  end_year?: number | null;
  source_ids?: string | null;
  confidence?: number | null;
  relation_name_zh_cn?: string | null;
  relation_category?: string | null;
}

export interface SemanticPersonPlace {
  person_id: string;
  place_id: string;
  place_name?: string | null;
  historical_name?: string | null;
  longitude?: number | null;
  latitude?: number | null;
  relation_type: string;
  start_year?: number | null;
  end_year?: number | null;
  source_id: string;
}

export interface SemanticPersonEvent {
  event_id: string;
  event_name: string;
  start_year?: number | null;
  end_year?: number | null;
  summary_zh_cn?: string | null;
  role_zh_cn?: string | null;
}

export interface SemanticPersonStory {
  story_id: string;
  title_zh_cn: string;
  start_year?: number | null;
  end_year?: number | null;
  summary_zh_cn?: string | null;
}

export interface SemanticHome {
  periods: SemanticPeriod[];
  stories: SemanticStory[];
  stats?: SemanticStats | null;
}

export interface SemanticPeriodDetail {
  period: SemanticPeriod;
  regimes: SemanticRegime[];
  stories: SemanticStory[];
  events: SemanticPeriodEvent[];
  people: SemanticPeriodPerson[];
}

export interface SemanticStoryDetail {
  story: SemanticStory;
  events: SemanticStoryEvent[];
  people: SemanticEventPerson[];
  places: SemanticEventPlace[];
  historical_texts: SemanticHistoricalText[];
  evidences: SemanticEventEvidence[];
  sources: SemanticSource[];
}

export interface SemanticEventDetail {
  event: SemanticEvent;
  people: SemanticEventPerson[];
  places: SemanticEventPlace[];
  relations: SemanticEventRelation[];
  historical_texts: SemanticHistoricalText[];
  evidences: SemanticEventEvidence[];
  sources: SemanticSource[];
}

export interface SemanticPersonDetail {
  person: SemanticPerson;
  relations: SemanticPersonRelation[];
  places: SemanticPersonPlace[];
  events: SemanticPersonEvent[];
  stories: SemanticPersonStory[];
  sources: SemanticSource[];
}

export interface SemanticSearchHit {
  id: string;
  kind: "person" | "story" | "event" | "work";
  title: string;
  subtitle?: string | null;
  start_year?: number | null;
  end_year?: number | null;
}

export interface SemanticSearchGroup {
  kind: SemanticSearchHit["kind"];
  items: SemanticSearchHit[];
}

/** 史籍作品（WorkResult 镜像）。 */
export interface SemanticWork {
  id: string;
  title: string;
  title_zh_cn?: string | null;
  source_id?: string | null;
  quality_status?: string | null;
}

/** 作品下的章节全文（HistoricalTextResult 镜像，未绑定具体事件）。 */
export interface SemanticWorkText {
  id: string;
  title_zh_cn?: string | null;
  book_id?: string | null;
  work_title?: string | null;
  chapter?: string | null;
  original_text?: string | null;
  original_simplified?: string | null;
  translation_zh_cn?: string | null;
  translation_source?: string | null;
  alignment_quality?: string | null;
  source_id?: string | null;
  quality_status?: string | null;
  translation_type?: string | null;
}

export interface SemanticWorkDetail {
  work: SemanticWork;
  texts: SemanticWorkText[];
  sources: SemanticSource[];
}
