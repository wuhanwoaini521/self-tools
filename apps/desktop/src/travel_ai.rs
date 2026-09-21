//! Travel AI 适配器（V5 Gate 5）：把 travel 既有能力暴露给 Personal AI 模块。
//!
//! - search_destination：复用搜索 Provider（按最新设置装配，同 enrichment 搜索）。
//! - trip_context / plan_preview：从 TravelStore 缓存读取（不触发生成、不写永久数据，
//!   §43：AI 不偷偷修改 itinerary；保存走既有流程）。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use devtoolbox_application::travel::{TravelAiPort, TravelSearchHit, TripContext};
use devtoolbox_core::travel::SearchOptions;
use devtoolbox_infrastructure::{TravelStore, build_providers};

/// 设置读取器（与 enrichment 同型）。
pub type SettingsLoader =
    Arc<dyn Fn() -> Result<devtoolbox_core::settings::AppSettings, String> + Send + Sync>;

fn normalize_domain(url: &str) -> String {
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    host.trim_start_matches("www.").to_lowercase()
}

pub struct TravelAiAdapter {
    client: reqwest::Client,
    settings: SettingsLoader,
    store: Arc<Mutex<TravelStore>>,
}

impl TravelAiAdapter {
    #[must_use]
    pub fn new(
        client: reqwest::Client,
        settings: SettingsLoader,
        store: Arc<Mutex<TravelStore>>,
    ) -> Self {
        Self {
            client,
            settings,
            store,
        }
    }

    fn find_guide(&self, city: &str) -> Option<devtoolbox_core::travel::CityGuide> {
        let now = devtoolbox_infrastructure::now_unix();
        let store = self.store.lock().expect("travel store poisoned");
        (1u8..=7).find_map(|days| store.get_guide(city, days, now).ok().flatten())
    }
}

#[async_trait]
impl TravelAiPort for TravelAiAdapter {
    fn configured(&self) -> bool {
        match (self.settings)() {
            Ok(settings) => !build_providers(
                settings.travel.search_backend.clone(),
                settings.travel.searxng_url.clone(),
                &self.client,
            )
            .is_empty(),
            Err(_) => false,
        }
    }

    async fn search_destination(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<TravelSearchHit>, String> {
        let settings = (self.settings)()?;
        let providers = build_providers(
            settings.travel.search_backend.clone(),
            settings.travel.searxng_url.clone(),
            &self.client,
        );
        for provider in &providers {
            match provider.search(query, SearchOptions { count: limit }).await {
                Ok(results) if !results.is_empty() => {
                    return Ok(results
                        .into_iter()
                        .take(limit)
                        .map(|result| {
                            let url = result.url;
                            let domain = normalize_domain(&url);
                            TravelSearchHit {
                                title: result.title,
                                url,
                                snippet: result.snippet,
                                domain,
                            }
                        })
                        .collect());
                }
                Ok(_) => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("search returned no results".to_string())
    }

    fn trip_context(&self, city: &str) -> Result<Option<TripContext>, String> {
        Ok(self.find_guide(city).map(|guide| map_guide(&guide, true)))
    }

    fn plan_preview(&self, city: &str, days: u8) -> Result<Option<TripContext>, String> {
        if let Some(guide) = self.find_guide(city) {
            return Ok(Some(map_guide(&guide, true)));
        }
        // 确定性骨架（§43：preview，不写永久数据；详细规划在 Travel 页发起）
        Ok(Some(TripContext {
            city: city.to_string(),
            days,
            summary: Some("尚未生成行程。请在 Travel 页以该城市发起研究，获得可保存的完整攻略；此处仅提供占位预览。".to_string()),
            highlights: Vec::new(),
            sources_count: 0,
            from_cache: false,
        }))
    }
}

fn map_guide(guide: &devtoolbox_core::travel::CityGuide, from_cache: bool) -> TripContext {
    TripContext {
        city: guide.city.name.clone(),
        days: guide.meta.days,
        summary: Some(guide.summary.clone()),
        highlights: guide.highlights.clone(),
        sources_count: guide.sources.len(),
        from_cache,
    }
}
