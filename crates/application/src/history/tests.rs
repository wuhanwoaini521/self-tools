//! History 用例层测试：Fake Port（不启动 DuckDB），验证聚合 / 分组 / 截断 /
//! 来源 ID 合并 / period 解析 / 空结果与错误传播等用例决策。

use std::sync::{Arc, Mutex};

use devtoolbox_core::history_records::{
    DatasetStats, EventEvidenceResult, EventHistoricalTextResult, EventPersonResult,
    EventPlaceResult, EventRelationResult, EventResult, HistoricalTextResult, PeriodEventItem,
    PeriodPersonItem, PeriodResult, PersonEventResult, PersonPlaceResult, PersonRelationResult,
    PersonResult, PersonStoryResult, RegimeResult, SourceResult, StoryEventResult, StoryResult,
    WorkResult,
};

use super::HistoryQueryPort;
use super::HistoryService;
use super::ports::HistoryPortError;
use crate::ApplicationError;

type FakeLink = Arc<Mutex<FakePortData>>;

// ---------- Fixture 构造（字段与 infra 结果类型一一对应） ----------

fn person(id: &str, canonical: &str) -> PersonResult {
    PersonResult {
        id: id.into(),
        canonical_name_zh_cn: canonical.into(),
        name_raw: None,
        birth_year: Some(600),
        death_year: Some(649),
        gender: None,
        quality_status: None,
        created_from_source: None,
        intro_zh_cn: Some("简介".into()),
    }
}

fn story(id: &str, title: &str, summary: Option<&str>) -> StoryResult {
    StoryResult {
        id: id.into(),
        title_zh_cn: title.into(),
        start_year: Some(600),
        end_year: Some(650),
        summary_zh_cn: summary.map(str::to_owned),
        background_zh_cn: None,
        result_zh_cn: None,
        story_type: None,
        importance: None,
        period_id: None,
        quality_status: None,
        source_type: None,
        source_ids: None,
        usable: None,
        title_raw: None,
        period_ids: None,
        source_reference: None,
    }
}

fn event(id: &str, name: &str, source_ids: Option<&str>) -> EventResult {
    EventResult {
        id: id.into(),
        name_zh_cn: name.into(),
        event_type: None,
        start_year: Some(640),
        end_year: None,
        date_precision: None,
        period_id: None,
        regime_id: None,
        summary_zh_cn: Some("事件摘要".into()),
        background_zh_cn: None,
        process_zh_cn: None,
        result_zh_cn: None,
        impact_zh_cn: None,
        importance: None,
        quality_status: None,
        source_type: None,
        source_ids: source_ids.map(str::to_owned),
        period_ids: None,
        dynasty_ids: None,
        regime_ids: None,
        source_reference: None,
    }
}

fn work(id: &str, title: &str, title_zh: Option<&str>, source_id: Option<&str>) -> WorkResult {
    WorkResult {
        id: id.into(),
        title: title.into(),
        title_zh_cn: title_zh.map(str::to_owned),
        source_id: source_id.map(str::to_owned),
        quality_status: None,
    }
}

fn period(id: &str) -> PeriodResult {
    PeriodResult {
        id: id.into(),
        name_zh_cn: "唐朝".into(),
        start_year: Some(618),
        end_year: Some(907),
        date_precision: None,
        description_zh_cn: None,
        quality_status: None,
        source_type: None,
        source_ids: None,
        name_raw: None,
        source_reference: None,
    }
}

fn source(id: &str) -> SourceResult {
    SourceResult {
        id: id.into(),
        dataset: None,
        snapshot_version: None,
        dataset_version: None,
        source_type: None,
        license: None,
        quality: None,
        quality_status: None,
        original_url: None,
    }
}

fn story_event(story_id: &str, source_ids: Option<&str>) -> StoryEventResult {
    StoryEventResult {
        story_id: story_id.into(),
        event_id: "ev".into(),
        sequence: None,
        role: None,
        importance: None,
        transition_text_zh_cn: None,
        quality_status: None,
        name_zh_cn: "事件".into(),
        event_type: None,
        start_year: None,
        end_year: None,
        date_precision: None,
        summary_zh_cn: None,
        result_zh_cn: None,
        event_quality_status: None,
        source_type: None,
        source_ids: source_ids.map(str::to_owned),
    }
}

fn event_person(source_id: Option<&str>) -> EventPersonResult {
    EventPersonResult {
        event_id: "e".into(),
        person_id: "p".into(),
        role: "主角".into(),
        role_zh_cn: None,
        side: None,
        importance: None,
        source_id: source_id.map(str::to_owned),
        quality_status: None,
        person_name: None,
        description: None,
        link_quality_status: None,
        link_confidence: None,
        link_reason: None,
        birth_year: None,
        death_year: None,
        person_quality_status: None,
    }
}

fn event_place(source_id: Option<&str>) -> EventPlaceResult {
    EventPlaceResult {
        event_id: "e".into(),
        place_id: Some("pl".into()),
        place_name_raw: None,
        role: None,
        sequence: None,
        source_id: source_id.map(str::to_owned),
        quality_status: None,
        link_status: None,
        place_name: None,
        description_zh_cn: None,
        link_quality_status: None,
        link_confidence: None,
        link_reason: None,
        historical_name: None,
        modern_name: None,
        longitude: None,
        latitude: None,
    }
}

fn event_relation(source_id: Option<&str>) -> EventRelationResult {
    EventRelationResult {
        source_event_id: "e1".into(),
        target_event_id: "e2".into(),
        relation_type: "precedes".into(),
        confidence: None,
        description_zh_cn: None,
        source_id: source_id.map(str::to_owned),
        quality_status: None,
        source_event_name: None,
        target_event_name: None,
    }
}

fn event_text(source_id: Option<&str>) -> EventHistoricalTextResult {
    EventHistoricalTextResult {
        event_id: "e".into(),
        historical_text_id: "t".into(),
        role: None,
        sequence: None,
        source_id: source_id.map(str::to_owned),
        quality_status: None,
        title_zh_cn: None,
        work_title: None,
        chapter: None,
        original_text: None,
        original_simplified: None,
        translation_zh_cn: None,
        source_quality_status: None,
        link_quality_status: None,
        link_confidence: None,
        link_reason: None,
        temporal_score: None,
        person_score: None,
        place_score: None,
        keyword_score: None,
        work_score: None,
        context_score: None,
        chapter_score: None,
        translation_source: None,
        alignment_quality: None,
    }
}

fn event_evidence(source_id: Option<&str>) -> EventEvidenceResult {
    EventEvidenceResult {
        id: "ev".into(),
        event_id: "e".into(),
        historical_text_id: None,
        work: None,
        term: None,
        chapter_hint: None,
        context_keywords: None,
        evidence_role: None,
        link_status: None,
        link_quality_status: None,
        link_confidence: None,
        review_note: None,
        source_type: None,
        source_id: source_id.map(str::to_owned),
        quality_status: None,
        rejected_text_ids: None,
    }
}

fn person_relation(source_ids: Option<&str>) -> PersonRelationResult {
    PersonRelationResult {
        person_a_id: "a".into(),
        person_a_name: None,
        person_b_id: "b".into(),
        person_b_name: None,
        relation_type: "师生".into(),
        start_year: None,
        end_year: None,
        source_ids: source_ids.map(str::to_owned),
        confidence: None,
        relation_name_zh_cn: None,
        relation_category: None,
    }
}

fn person_place(source_id: &str) -> PersonPlaceResult {
    PersonPlaceResult {
        person_id: "p".into(),
        place_id: "pl".into(),
        place_name: None,
        historical_name: None,
        longitude: None,
        latitude: None,
        relation_type: "籍贯".into(),
        start_year: None,
        end_year: None,
        source_id: source_id.into(),
    }
}

fn person_event() -> PersonEventResult {
    PersonEventResult {
        event_id: "e".into(),
        event_name: "玄武门之变".into(),
        start_year: None,
        end_year: None,
        summary_zh_cn: None,
        role_zh_cn: None,
    }
}

fn person_story() -> PersonStoryResult {
    PersonStoryResult {
        story_id: "s".into(),
        title_zh_cn: "唐朝开国".into(),
        start_year: None,
        end_year: None,
        summary_zh_cn: None,
    }
}

fn regime(id: &str) -> RegimeResult {
    RegimeResult {
        id: id.into(),
        name_zh_cn: "武周".into(),
        start_year: None,
        end_year: None,
        date_precision: None,
        period_id: None,
        parent_regime_id: None,
        capital_place_id: None,
        description_zh_cn: None,
        quality_status: None,
        source_type: None,
        source_ids: None,
    }
}

fn period_event_item(id: &str) -> PeriodEventItem {
    PeriodEventItem {
        id: id.into(),
        name_zh_cn: "事件".into(),
        event_type: None,
        start_year: None,
        end_year: None,
        importance: None,
        summary_zh_cn: None,
        result_zh_cn: None,
        people_count: 0,
        relation_count: 0,
        evidence_count: 0,
    }
}

fn period_person_item(id: &str) -> PeriodPersonItem {
    PeriodPersonItem {
        person_id: id.into(),
        canonical_name_zh_cn: "人物".into(),
        event_count: 0,
        birth_year: None,
        death_year: None,
        intro_zh_cn: None,
    }
}

fn historical_text(id: &str, source_id: Option<&str>) -> HistoricalTextResult {
    HistoricalTextResult {
        id: id.into(),
        title_zh_cn: None,
        book_id: None,
        work_title: None,
        chapter: None,
        original_text: None,
        original_simplified: None,
        translation_zh_cn: None,
        translation_source: None,
        alignment_quality: None,
        source_id: source_id.map(str::to_owned),
        quality_status: None,
        translation_type: None,
    }
}

// ---------- Fake 端口 ----------

#[derive(Default)]
struct FakePortData {
    stats: DatasetStats,
    periods: Vec<PeriodResult>,
    regimes: Vec<RegimeResult>,
    events_for_period: Vec<PeriodEventItem>,
    people_for_period: Vec<PeriodPersonItem>,
    relations_for_period: Vec<EventRelationResult>,
    stories: Vec<StoryResult>,
    stories_for_period: Vec<StoryResult>,
    story: Option<StoryResult>,
    story_events: Vec<StoryEventResult>,
    story_people: Vec<EventPersonResult>,
    story_places: Vec<EventPlaceResult>,
    story_texts: Vec<EventHistoricalTextResult>,
    story_evidences: Vec<EventEvidenceResult>,
    event: Option<EventResult>,
    event_people: Vec<EventPersonResult>,
    event_places: Vec<EventPlaceResult>,
    event_relations: Vec<EventRelationResult>,
    event_texts: Vec<EventHistoricalTextResult>,
    event_evidences: Vec<EventEvidenceResult>,
    person: Option<PersonResult>,
    person_relations: Vec<PersonRelationResult>,
    person_places: Vec<PersonPlaceResult>,
    person_events: Vec<PersonEventResult>,
    person_stories: Vec<PersonStoryResult>,
    work: Option<WorkResult>,
    historical_texts: Vec<HistoricalTextResult>,
    searched_people: Vec<PersonResult>,
    searched_events: Vec<EventResult>,
    searched_works: Vec<WorkResult>,
    sources: Vec<SourceResult>,
    /// true 时所有查询返回 HistoryPortError。
    fail: bool,
    /// 每次数据访问的方法名（用于“空查询不触碰数据源”等断言）。
    calls: Vec<&'static str>,
    /// 每次来源合并后请求的 id 集合。
    source_requests: Vec<Vec<String>>,
    /// search_people / search_events / get_work 收到的 limit。
    search_limits: Vec<i64>,
    /// get_people_for_period 收到的 limit。
    people_period_limits: Vec<i64>,
    /// get_historical_texts 收到的 (work, limit)。
    texts_queries: Vec<(Option<String>, i64)>,
}

fn err() -> HistoryPortError {
    HistoryPortError("boom".into())
}

impl HistoryQueryPort for FakeLink {
    fn get_dataset_stats(&self) -> Result<DatasetStats, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_dataset_stats");
        if port.fail {
            return Err(err());
        }
        Ok(port.stats.clone())
    }
    fn get_periods(&self) -> Result<Vec<PeriodResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_periods");
        if port.fail {
            return Err(err());
        }
        Ok(port.periods.clone())
    }
    fn get_regimes_by_period(
        &self,
        _period_id: &str,
    ) -> Result<Vec<RegimeResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_regimes_by_period");
        if port.fail {
            return Err(err());
        }
        Ok(port.regimes.clone())
    }
    fn get_events_for_period(
        &self,
        _period_id: &str,
    ) -> Result<Vec<PeriodEventItem>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_events_for_period");
        if port.fail {
            return Err(err());
        }
        Ok(port.events_for_period.clone())
    }
    fn get_people_for_period(
        &self,
        _period_id: &str,
        limit: i64,
    ) -> Result<Vec<PeriodPersonItem>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_people_for_period");
        port.people_period_limits.push(limit);
        if port.fail {
            return Err(err());
        }
        Ok(port.people_for_period.clone())
    }
    fn get_relations_for_period(
        &self,
        _period_id: &str,
    ) -> Result<Vec<EventRelationResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_relations_for_period");
        if port.fail {
            return Err(err());
        }
        Ok(port.relations_for_period.clone())
    }
    fn get_stories(&self) -> Result<Vec<StoryResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_stories");
        if port.fail {
            return Err(err());
        }
        Ok(port.stories.clone())
    }
    fn get_stories_for_period(
        &self,
        _period_id: Option<&str>,
    ) -> Result<Vec<StoryResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_stories_for_period");
        if port.fail {
            return Err(err());
        }
        Ok(port.stories_for_period.clone())
    }
    fn get_story(&self, _story_id: &str) -> Result<Option<StoryResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_story");
        if port.fail {
            return Err(err());
        }
        Ok(port.story.clone())
    }
    fn get_story_events(&self, _story_id: &str) -> Result<Vec<StoryEventResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_story_events");
        if port.fail {
            return Err(err());
        }
        Ok(port.story_events.clone())
    }
    fn get_story_people(
        &self,
        _story_id: &str,
    ) -> Result<Vec<EventPersonResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_story_people");
        if port.fail {
            return Err(err());
        }
        Ok(port.story_people.clone())
    }
    fn get_story_places(&self, _story_id: &str) -> Result<Vec<EventPlaceResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_story_places");
        if port.fail {
            return Err(err());
        }
        Ok(port.story_places.clone())
    }
    fn get_story_texts(
        &self,
        _story_id: &str,
    ) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_story_texts");
        if port.fail {
            return Err(err());
        }
        Ok(port.story_texts.clone())
    }
    fn get_story_evidences(
        &self,
        _story_id: &str,
    ) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_story_evidences");
        if port.fail {
            return Err(err());
        }
        Ok(port.story_evidences.clone())
    }
    fn get_event(&self, _event_id: &str) -> Result<Option<EventResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_event");
        if port.fail {
            return Err(err());
        }
        Ok(port.event.clone())
    }
    fn get_event_people(
        &self,
        _event_id: &str,
    ) -> Result<Vec<EventPersonResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_event_people");
        if port.fail {
            return Err(err());
        }
        Ok(port.event_people.clone())
    }
    fn get_event_places(&self, _event_id: &str) -> Result<Vec<EventPlaceResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_event_places");
        if port.fail {
            return Err(err());
        }
        Ok(port.event_places.clone())
    }
    fn get_event_relations(
        &self,
        _event_id: &str,
    ) -> Result<Vec<EventRelationResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_event_relations");
        if port.fail {
            return Err(err());
        }
        Ok(port.event_relations.clone())
    }
    fn get_event_texts(
        &self,
        _event_id: &str,
    ) -> Result<Vec<EventHistoricalTextResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_event_texts");
        if port.fail {
            return Err(err());
        }
        Ok(port.event_texts.clone())
    }
    fn get_event_evidences(
        &self,
        _event_id: &str,
    ) -> Result<Vec<EventEvidenceResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_event_evidences");
        if port.fail {
            return Err(err());
        }
        Ok(port.event_evidences.clone())
    }
    fn get_person(&self, _person_id: &str) -> Result<Option<PersonResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_person");
        if port.fail {
            return Err(err());
        }
        Ok(port.person.clone())
    }
    fn get_person_relations(
        &self,
        _person_id: &str,
    ) -> Result<Vec<PersonRelationResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_person_relations");
        if port.fail {
            return Err(err());
        }
        Ok(port.person_relations.clone())
    }
    fn get_person_places(
        &self,
        _person_id: &str,
    ) -> Result<Vec<PersonPlaceResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_person_places");
        if port.fail {
            return Err(err());
        }
        Ok(port.person_places.clone())
    }
    fn get_person_events(
        &self,
        _person_id: &str,
    ) -> Result<Vec<PersonEventResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_person_events");
        if port.fail {
            return Err(err());
        }
        Ok(port.person_events.clone())
    }
    fn get_person_stories(
        &self,
        _person_id: &str,
    ) -> Result<Vec<PersonStoryResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_person_stories");
        if port.fail {
            return Err(err());
        }
        Ok(port.person_stories.clone())
    }
    fn get_work_by_id(&self, _work_id: &str) -> Result<Option<WorkResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_work_by_id");
        if port.fail {
            return Err(err());
        }
        Ok(port.work.clone())
    }
    fn get_work(&self, _title: &str, limit: i64) -> Result<Vec<WorkResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_work");
        port.search_limits.push(limit);
        if port.fail {
            return Err(err());
        }
        Ok(port.searched_works.clone())
    }
    fn get_historical_texts(
        &self,
        work: Option<&str>,
        limit: i64,
    ) -> Result<Vec<HistoricalTextResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_historical_texts");
        port.texts_queries.push((work.map(str::to_owned), limit));
        if port.fail {
            return Err(err());
        }
        Ok(port.historical_texts.clone())
    }
    fn search_people(
        &self,
        _query: &str,
        limit: i64,
    ) -> Result<Vec<PersonResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("search_people");
        port.search_limits.push(limit);
        if port.fail {
            return Err(err());
        }
        Ok(port.searched_people.clone())
    }
    fn search_events(
        &self,
        _query: &str,
        limit: i64,
    ) -> Result<Vec<EventResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("search_events");
        port.search_limits.push(limit);
        if port.fail {
            return Err(err());
        }
        Ok(port.searched_events.clone())
    }
    fn get_sources_for_ids(&self, ids: &[String]) -> Result<Vec<SourceResult>, HistoryPortError> {
        let mut port = self.lock().expect("fake port poisoned");
        port.calls.push("get_sources_for_ids");
        port.source_requests.push(ids.to_vec());
        if port.fail {
            return Err(err());
        }
        Ok(port.sources.clone())
    }
}

fn service(fake: FakeLink) -> HistoryService {
    HistoryService::new(Box::new(fake))
}

/// 制造一个只有数据、没有任何前期查询的 Fake。
fn seeded_fake() -> FakePortData {
    let mut fake = FakePortData::default();
    fake.searched_people = vec![person("p1", "李世民"), person("p2", "魏征")];
    fake.searched_events = vec![event("e1", "玄武门之变", None)];
    fake.searched_works = vec![work("w1", "贞观政要", None, None)];
    fake.stories = vec![story("s1", "唐朝开国", Some("贞观之治的起点"))];
    fake
}

// ---------- 用例测试 ----------

#[test]
fn search_with_empty_query_returns_no_groups_without_touching_port() {
    let fake = Arc::new(Mutex::new(FakePortData::default()));
    let service = service(Arc::clone(&fake));
    let result = service.search("   ").expect("empty query is not an error");
    assert!(result.is_empty());
    assert!(
        fake.lock().unwrap().calls.is_empty(),
        "empty query must not touch the port"
    );
}

#[test]
fn search_groups_by_kind_in_fixed_order_and_skips_empty_groups() {
    let fake = Arc::new(Mutex::new(seeded_fake()));
    let service = service(Arc::clone(&fake));
    let groups = service.search("唐").expect("search succeeds");

    // person → story → event → work；search_people/search_events/get_work 各收 limit=8。
    let kinds = groups
        .iter()
        .map(|group| group.kind.as_str())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["person", "story", "event", "work"]);
    assert_eq!(fake.lock().unwrap().search_limits, [8, 8, 8]);

    let person_items = &groups[0].items;
    assert_eq!(person_items.len(), 2);
    assert_eq!(person_items[0].kind, "person");
    assert_eq!(person_items[0].id, "p1");
    assert_eq!(person_items[0].title, "李世民");
    assert_eq!(person_items[0].subtitle.as_deref(), Some("简介"));
    assert_eq!(person_items[0].start_year, Some(600));
    assert_eq!(person_items[0].end_year, Some(649));

    let story_items = &groups[1].items;
    assert_eq!(story_items.len(), 1);
    assert_eq!(story_items[0].id, "s1");
    assert_eq!(story_items[0].title, "唐朝开国");
    assert_eq!(story_items[0].subtitle.as_deref(), Some("贞观之治的起点"));
    assert_eq!(story_items[0].start_year, Some(600));
    assert_eq!(story_items[0].end_year, Some(650));

    let event_items = &groups[2].items;
    assert_eq!(event_items.len(), 1);
    assert_eq!(event_items[0].id, "e1");
    assert_eq!(event_items[0].title, "玄武门之变");
    assert_eq!(event_items[0].subtitle.as_deref(), Some("事件摘要"));

    let work_items = &groups[3].items;
    assert_eq!(work_items.len(), 1);
    assert_eq!(work_items[0].title, "贞观政要");
    assert_eq!(work_items[0].subtitle, None);
    assert_eq!(work_items[0].start_year, None);
}

#[test]
fn search_matches_story_by_title_or_summary_and_strips_whitespace() {
    let mut fake = FakePortData::default();
    fake.stories = vec![
        story("s-title", "长安城", None),
        story("s-summary", "无关标题", Some("在 贞观 年间……")),
        story("s-nomatch", "完全无关", None),
    ];
    let fake = Arc::new(Mutex::new(fake));
    let service = service(Arc::clone(&fake));
    let groups = service.search(" 贞观 ").expect("search succeeds");
    let story_ids = groups
        .iter()
        .find(|group| group.kind == "story")
        .map(|group| {
            group
                .items
                .iter()
                .map(|item| item.id.clone())
                .collect::<Vec<_>>()
        })
        .expect("story group exists");
    assert_eq!(story_ids, ["s-summary"]);
}

#[test]
fn search_truncates_story_group_to_eight() {
    let mut fake = FakePortData::default();
    fake.stories = (0..13)
        .map(|index| story(&format!("s{index}"), &format!("贞观{index}"), None))
        .collect();
    let fake = Arc::new(Mutex::new(fake));
    let service = service(Arc::clone(&fake));
    let groups = service.search("贞观").expect("search succeeds");
    let story_ids = groups
        .iter()
        .find(|group| group.kind == "story")
        .map(|group| {
            group
                .items
                .iter()
                .map(|item| item.id.clone())
                .collect::<Vec<_>>()
        })
        .expect("story group is present");
    assert_eq!(story_ids.len(), 8);
}

#[test]
fn search_work_hit_uses_title_zh_cn_with_title_fallback() {
    let mut fake = FakePortData::default();
    fake.searched_works = vec![
        work("w-zh", "ACTUAL_WORKS", Some("贞观政要"), None),
        work("w-raw", "资治通鉴", None, None),
    ];
    let fake = Arc::new(Mutex::new(fake));
    let service = service(Arc::clone(&fake));
    let groups = service.search("贞观").expect("search succeeds");
    let titles = groups
        .iter()
        .find(|group| group.kind == "work")
        .map(|group| {
            group
                .items
                .iter()
                .map(|item| item.title.clone())
                .collect::<Vec<_>>()
        })
        .expect("work group is present");
    assert_eq!(titles, ["贞观政要", "资治通鉴"]);
}

#[test]
fn home_returns_periods_stories_and_stats() {
    let mut fake = FakePortData::default();
    fake.periods = vec![period("tang")];
    fake.stories = vec![story("s1", "唐朝开国", None)];
    fake.stats.events = 618;
    let fake = Arc::new(Mutex::new(fake));
    let service = service(Arc::clone(&fake));
    let home = service.home().expect("home succeeds");
    assert_eq!(home.periods.len(), 1);
    assert_eq!(home.stories.len(), 1);
    assert_eq!(home.stats.events, 618);
}

#[test]
fn period_detail_resolves_period_then_assembles_sections() {
    let fake = Arc::new(Mutex::new({
        let mut data = FakePortData::default();
        data.periods = vec![period("p1"), period("p2")];
        data.regimes = vec![regime("r1"), regime("r2")];
        data.events_for_period = vec![period_event_item("e1")];
        data.people_for_period = vec![period_person_item("pe1")];
        data.stories_for_period = vec![story("s1", "唐朝开国", None)];
        data
    }));
    let service = service(Arc::clone(&fake));
    let detail = service
        .period_detail("p2")
        .expect("period_detail succeeds")
        .expect("p2 exists");
    assert_eq!(detail.period.id, "p2");
    assert_eq!(detail.regimes.len(), 2);
    assert_eq!(detail.events.len(), 1);
    assert_eq!(detail.people.len(), 1);
    assert_eq!(detail.stories.len(), 1);
    assert_eq!(
        fake.lock().unwrap().calls,
        [
            "get_periods",
            "get_events_for_period",
            "get_relations_for_period",
            "get_regimes_by_period",
            "get_stories_for_period",
            "get_people_for_period"
        ]
    );
    assert_eq!(fake.lock().unwrap().people_period_limits, [24]);
}

#[test]
fn period_detail_derives_stages_from_critical_anchors() {
    let mut data = FakePortData::default();
    data.periods = vec![PeriodResult {
        id: "period-republic".into(),
        name_zh_cn: "中华民国".into(),
        start_year: Some(1912),
        end_year: Some(1949),
        ..period("period-republic")
    }];
    // 锚点：1912（南北议和）、1919（五四）、1936（西安）、1937（七七）→ 1936/1937 合并；1945（日本投降）。
    let anchors = vec![
        ("e-1912", "南北议和与清帝退位", 1912, "critical"),
        ("e-1919", "五四运动", 1919, "critical"),
        ("e-1931", "九一八事变", 1931, "critical"),
        ("e-1936", "西安事变", 1936, "critical"),
        ("e-1937", "七七事变", 1937, "critical"),
        ("e-1945", "日本宣布投降", 1945, "critical"),
    ];
    let mut events = Vec::new();
    for (id, name, year, importance) in anchors {
        let mut event = period_event_item(id);
        event.name_zh_cn = name.into();
        event.start_year = Some(year);
        event.importance = Some(importance.into());
        events.push(event);
    }
    let mut fill = period_event_item("e-fill");
    fill.start_year = Some(1940);
    fill.importance = Some("major".into());
    events.push(fill);
    data.events_for_period = events;
    data.people_for_period = vec![period_person_item("pe1")];
    let fake = Arc::new(Mutex::new(data));
    let detail = service(Arc::clone(&fake))
        .period_detail("period-republic")
        .expect("period_detail succeeds")
        .expect("period exists");
    let stages = &detail.stages;
    assert_eq!(stages.len(), 5);
    assert_eq!(
        stages
            .iter()
            .map(|stage| (
                stage.start_year,
                stage.end_year,
                stage.opening_event_name.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            (1912, 1918, "南北议和与清帝退位"),
            (1919, 1930, "五四运动"),
            (1931, 1935, "九一八事变"),
            (1936, 1944, "西安事变"),
            (1945, 1949, "日本宣布投降"),
        ]
    );
    assert_eq!(
        stages[3].event_count, 3,
        "1936–1944 章含 西安事变 + 七七事变 + 1940 填充事件"
    );
}

#[test]
fn period_detail_derives_front_stage_when_anchor_comes_late() {
    // 战国式：首个 critical 锚点（-403 三家分晋）晚于时期开始（-475），
    // 前有真实事件 → 生成空白「开端章」（index 1，无 opening event）。
    let mut data = FakePortData::default();
    data.periods = vec![PeriodResult {
        id: "period-warring".into(),
        name_zh_cn: "战国".into(),
        start_year: Some(-475),
        end_year: Some(-221),
        ..period("period-warring")
    }];
    let mut pre = period_event_item("e-pre");
    pre.start_year = Some(-450);
    pre.importance = Some("major".into());
    let mut a1 = period_event_item("e-a1");
    a1.start_year = Some(-403);
    a1.importance = Some("critical".into());
    let mut a2 = period_event_item("e-a2");
    a2.start_year = Some(-260);
    a2.importance = Some("critical".into());
    data.events_for_period = vec![pre, a1, a2];
    let fake = Arc::new(Mutex::new(data));
    let detail = service(Arc::clone(&fake))
        .period_detail("period-warring")
        .expect("period_detail succeeds")
        .expect("period exists");
    let stages = &detail.stages;
    assert_eq!(stages.len(), 3, "开端章 + 2 锚点章");
    assert_eq!(stages[0].index, 1);
    assert_eq!(stages[0].start_year, -475);
    assert_eq!(stages[0].end_year, -404);
    assert!(stages[0].opening_event_id.is_empty(), "开端章无锚点事件");
    assert_eq!(stages[0].event_count, 1);
    assert_eq!(stages[1].index, 2);
    assert_eq!(stages[1].opening_event_name, "事件", "锚点章以关键事件命名");
}

#[test]
fn period_detail_without_critical_anchors_has_no_stages() {
    let mut data = FakePortData::default();
    data.periods = vec![period("p1")];
    let mut event = period_event_item("e1");
    event.start_year = Some(1912);
    event.importance = Some("major".into());
    data.events_for_period = vec![event];
    let fake = Arc::new(Mutex::new(data));
    let detail = service(Arc::clone(&fake))
        .period_detail("p1")
        .expect("period_detail succeeds")
        .expect("period exists");
    assert!(detail.stages.is_empty(), "无 critical 锚点 → 隐藏阶段模块");
}

#[test]
fn period_detail_for_unknown_period_returns_none_without_section_queries() {
    let fake = Arc::new(Mutex::new(FakePortData::default()));
    let service = service(Arc::clone(&fake));
    let detail = service.period_detail("p-missing").expect("no error");
    assert!(detail.is_none());
    assert_eq!(fake.lock().unwrap().calls, ["get_periods"]);
}

#[test]
fn story_detail_merges_sources_across_all_sections() {
    let mut story = story("s1", "唐朝开国", None);
    story.source_ids = Some(r#"["s1","s2"]"#.into());
    let fake = Arc::new(Mutex::new({
        let mut data = FakePortData::default();
        data.story = Some(story);
        data.story_events = vec![
            story_event("s1", Some(r#"["s3"]"#.into())),
            story_event("s1", None),
        ];
        data.story_people = vec![event_person(Some("s4")), event_person(None)];
        data.story_places = vec![event_place(Some("s5"))];
        data.story_texts = vec![event_text(Some("s6"))];
        data.story_evidences = vec![event_evidence(Some("s7")), event_evidence(None)];
        data.sources = vec![source("s1")];
        data
    }));
    let service = service(Arc::clone(&fake));
    let detail = service
        .story_detail("s1")
        .expect("story_detail succeeds")
        .expect("story exists");
    assert_eq!(detail.events.len(), 2);
    assert_eq!(detail.sources.len(), 1);
    assert_eq!(
        fake.lock().unwrap().source_requests,
        vec![vec!["s1", "s2", "s3", "s4", "s5", "s6", "s7"]]
    );
}

#[test]
fn event_detail_merges_sources_from_all_sections() {
    let fake = Arc::new(Mutex::new({
        let mut data = FakePortData::default();
        data.event = Some(event("e1", "玄武门之变", Some("[\"e1\",\"e2\"]")));
        data.event_people = vec![event_person(Some("e3"))];
        data.event_places = vec![event_place(Some("e4"))];
        data.event_relations = vec![event_relation(Some("e5")), event_relation(None)];
        data.event_texts = vec![event_text(Some("e6"))];
        data.event_evidences = vec![event_evidence(None)];
        data
    }));
    let service = service(Arc::clone(&fake));
    let detail = service
        .event_detail("e1")
        .expect("event_detail succeeds")
        .expect("event exists");
    assert_eq!(detail.relations.len(), 2);
    assert_eq!(
        fake.lock().unwrap().source_requests.last(),
        Some(&vec![
            "e1".to_owned(),
            "e2".to_owned(),
            "e3".to_owned(),
            "e4".to_owned(),
            "e5".to_owned(),
            "e6".to_owned()
        ])
    );
}

#[test]
fn person_detail_merges_sources_from_created_relations_and_places() {
    let mut person = person("p1", "李世民");
    person.created_from_source = Some("p1".into());
    let fake = Arc::new(Mutex::new({
        let mut data = FakePortData::default();
        data.person = Some(person);
        data.person_relations = vec![
            person_relation(Some("[\"p2\",\"p3\"]")),
            person_relation(None),
        ];
        data.person_places = vec![person_place("p4")];
        data.person_events = vec![person_event()];
        data.person_stories = vec![person_story()];
        data
    }));
    let service = service(Arc::clone(&fake));
    let detail = service
        .person_detail("p1")
        .expect("person_detail succeeds")
        .expect("person exists");
    assert_eq!(detail.events.len(), 1);
    assert_eq!(detail.stories.len(), 1);
    assert_eq!(
        fake.lock().unwrap().source_requests,
        vec![vec!["p1", "p2", "p3", "p4"]]
    );
}

#[test]
fn work_detail_queries_texts_by_work_title_and_merges_sources() {
    let work = work("w1", "旧唐书", Some("旧唐书"), Some("w1"));
    // 仅 title 参与文本查询（与 cutover 前一致）。
    let fake = Arc::new(Mutex::new({
        let mut data = FakePortData::default();
        data.work = Some(work);
        data.historical_texts = vec![
            historical_text("t1", Some("t2")),
            historical_text("t2", None),
        ];
        data
    }));
    let service = service(Arc::clone(&fake));
    let detail = service
        .work_detail("w1")
        .expect("work_detail succeeds")
        .expect("work exists");
    assert_eq!(detail.texts.len(), 2);
    assert_eq!(
        fake.lock().unwrap().texts_queries,
        vec![(Some("旧唐书".into()), 200)]
    );
    // BTreeSet 合并结果按 id 升序（与 Cutover 前行为一致）。
    assert_eq!(
        fake.lock().unwrap().source_requests,
        vec![vec!["t2".to_owned(), "w1".to_owned()]]
    );
}

#[test]
fn missing_entities_return_none_without_related_queries() {
    // 故事不存在 → 不查任何关联。
    let fake = Arc::new(Mutex::new(FakePortData::default()));
    let story_service = service(Arc::clone(&fake));
    assert!(
        story_service
            .story_detail("s1")
            .expect("no error")
            .is_none()
    );
    assert_eq!(fake.lock().unwrap().calls, ["get_story"]);

    // 事件不存在。
    let fake = Arc::new(Mutex::new(FakePortData::default()));
    let event_service = service(Arc::clone(&fake));
    assert!(
        event_service
            .event_detail("e1")
            .expect("no error")
            .is_none()
    );
    assert_eq!(fake.lock().unwrap().calls, ["get_event"]);

    // 人物不存在。
    let fake = Arc::new(Mutex::new(FakePortData::default()));
    let person_service = service(Arc::clone(&fake));
    assert!(
        person_service
            .person_detail("p1")
            .expect("no error")
            .is_none()
    );
    assert_eq!(fake.lock().unwrap().calls, ["get_person"]);

    // 作品不存在。
    let fake = Arc::new(Mutex::new(FakePortData::default()));
    let work_service = service(Arc::clone(&fake));
    assert!(work_service.work_detail("w1").expect("no error").is_none());
    assert_eq!(fake.lock().unwrap().calls, ["get_work_by_id"]);
}

#[test]
fn empty_related_data_still_builds_story_view_with_empty_sources() {
    let fake = Arc::new(Mutex::new({
        let mut data = FakePortData::default();
        data.story = Some(story("s1", "唐朝开国", None));
        data
    }));
    let service = service(Arc::clone(&fake));
    let detail = service
        .story_detail("s1")
        .expect("story_detail succeeds")
        .expect("story exists");
    assert!(detail.events.is_empty());
    assert!(detail.people.is_empty());
    assert!(detail.places.is_empty());
    assert!(detail.historical_texts.is_empty());
    assert!(detail.evidences.is_empty());
    assert!(detail.sources.is_empty());
    // 仍会以空 id 集合请求来源（与 Cutover 前行为一致）。
    assert_eq!(
        fake.lock().unwrap().source_requests,
        vec![Vec::<String>::new()]
    );
}

#[test]
fn port_failure_maps_to_history_application_error() {
    let fake = Arc::new(Mutex::new({
        let mut data = FakePortData::default();
        data.fail = true;
        data
    }));
    let service = service(Arc::clone(&fake));
    let error = service.home().expect_err("port failure propagates");
    match error {
        ApplicationError::History(HistoryPortError(message)) => {
            assert_eq!(message, "boom")
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}
