//! `TravelQueryPlanner` —— 旅行研究任务规划（需求 #六）。
//!
//! 输入城市与偏好，输出按类别组织的搜索任务。完全确定性、无 I/O、可单测；
//! LLM 的可选「扩展查询」由 application 层追加，不破坏本模块的纯度。

use serde::{Deserialize, Serialize};

/// 搜索任务类别（同时也是进度事件的消息分组）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryCategory {
    CityIntro,
    Attractions,
    Food,
    Accommodation,
    Transport,
    Itinerary,
    Activities,
    Warnings,
}

impl QueryCategory {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CityIntro => "城市介绍",
            Self::Attractions => "景点",
            Self::Food => "美食",
            Self::Accommodation => "住宿",
            Self::Transport => "交通",
            Self::Itinerary => "路线",
            Self::Activities => "活动",
            Self::Warnings => "注意事项",
        }
    }
}

/// 一个具体的搜索任务。
/// 搜索意图的语义类别。它与进度 UI 使用的 `QueryCategory` 分离，避免把
/// “历史 / 自然 / 美食”等用户偏好全部压成“景点”。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchIntentCategory {
    MustSee,
    CultureHistory,
    Nature,
    Food,
    Transport,
    AccommodationArea,
    CurrentStatus,
    Weather,
    LocalExperience,
    Event,
    Itinerary,
}

impl SearchIntentCategory {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "must_see" | "mustsee" | "attractions" | "景点" | "必去" => Some(Self::MustSee),
            "culture" | "history" | "culture_history" | "历史" | "文化" => {
                Some(Self::CultureHistory)
            }
            "nature" | "自然" | "户外" => Some(Self::Nature),
            "food" | "美食" => Some(Self::Food),
            "transport" | "交通" => Some(Self::Transport),
            "accommodation" | "accommodation_area" | "住宿" | "住哪里" => {
                Some(Self::AccommodationArea)
            }
            "current_status" | "opening_hours" | "status" | "开放状态" => {
                Some(Self::CurrentStatus)
            }
            "weather" | "天气" => Some(Self::Weather),
            "local_experience" | "photography" | "shopping" | "nightlife" | "本地体验" => {
                Some(Self::LocalExperience)
            }
            "event" | "活动" | "演出" => Some(Self::Event),
            "itinerary" | "路线" | "行程" => Some(Self::Itinerary),
            _ => None,
        }
    }
}

/// 一个具体的搜索意图。`priority` 为 0~1，供服务层控制抓取预算。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct QueryTask {
    pub category: QueryCategory,
    pub query: String,
    pub intent: SearchIntentCategory,
    pub purpose: String,
    pub priority: f32,
    pub freshness_required: bool,
    pub structured_data_preferred: bool,
}

/// 用户输入（需求 #六：城市 / 旅行时间 / 月份 / 偏好）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TravelQueryInput {
    pub city: String,
    /// 行程天数（1 ~ 7）。
    pub days: u8,
    /// 出行月份（1 ~ 12），可缺省。
    pub month: Option<u32>,
    /// 具体出行日期范围，用于检索该时段的天气、活动和客流信息。
    pub date_range: Option<TravelDateRange>,
    /// 偏好关键词（历史 / 美食 / 自然 / 散步 / 摄影 …），可缺省。
    pub preferences: Vec<String>,
}

/// 用户选择的出行日期范围（ISO `YYYY-MM-DD`）。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TravelDateRange {
    pub start: String,
    pub end: String,
}

struct IntentTemplate {
    intent: SearchIntentCategory,
    category: QueryCategory,
    query: &'static str,
    purpose: &'static str,
    priority: f32,
    freshness_required: bool,
    structured_data_preferred: bool,
}

const CORE_INTENTS: &[IntentTemplate] = &[
    IntentTemplate {
        intent: SearchIntentCategory::MustSee,
        category: QueryCategory::Attractions,
        query: "{city} 核心景点",
        purpose: "建立值得去的候选池",
        priority: 1.0,
        freshness_required: false,
        structured_data_preferred: true,
    },
    IntentTemplate {
        intent: SearchIntentCategory::Food,
        category: QueryCategory::Food,
        query: "{city} 本地特色美食",
        purpose: "识别城市代表性吃法",
        priority: 0.9,
        freshness_required: false,
        structured_data_preferred: true,
    },
    IntentTemplate {
        intent: SearchIntentCategory::AccommodationArea,
        category: QueryCategory::Accommodation,
        query: "{city} 住哪个区域方便",
        purpose: "比较住宿区域而非随机酒店",
        priority: 0.8,
        freshness_required: false,
        structured_data_preferred: false,
    },
    IntentTemplate {
        intent: SearchIntentCategory::Transport,
        category: QueryCategory::Transport,
        query: "{city} 市内交通与景点距离",
        purpose: "估算进出与市内移动成本",
        priority: 0.8,
        freshness_required: true,
        structured_data_preferred: false,
    },
    IntentTemplate {
        intent: SearchIntentCategory::CurrentStatus,
        category: QueryCategory::Warnings,
        query: "{city} 主要景区开放与预约",
        purpose: "核实开放、预约等硬事实",
        priority: 0.95,
        freshness_required: true,
        structured_data_preferred: false,
    },
    IntentTemplate {
        intent: SearchIntentCategory::MustSee,
        category: QueryCategory::CityIntro,
        query: "{city} 城市旅行概览",
        purpose: "确定城市旅行风格与区域",
        priority: 0.55,
        freshness_required: false,
        structured_data_preferred: false,
    },
];

pub struct TravelQueryPlanner;

impl TravelQueryPlanner {
    /// 生成完整搜索任务列表（已去重，保持类别插入顺序）。
    #[must_use]
    pub fn plan(input: &TravelQueryInput) -> Vec<QueryTask> {
        let city = input.city.trim();
        if city.is_empty() {
            return Vec::new();
        }
        let mut tasks: Vec<QueryTask> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        let mut push = |template: &IntentTemplate, query: String| {
            let query = query.trim().to_string();
            if query.is_empty() {
                return;
            }
            if seen.insert(query.clone()) {
                tasks.push(QueryTask {
                    category: template.category,
                    query,
                    intent: template.intent,
                    purpose: template.purpose.to_string(),
                    priority: template.priority,
                    freshness_required: template.freshness_required,
                    structured_data_preferred: template.structured_data_preferred,
                });
            }
        };

        for template in CORE_INTENTS {
            push(template, template.query.replace("{city}", city));
        }

        // 只有短途旅行才搜索真实的天数路线；4~7 天由 POI、距离和本地 Planner 组合，
        // 避免机械搜索“城市五日游”这种低信息量页面。
        let days = input.days.clamp(1, 7);
        if days <= 3 {
            let template = IntentTemplate {
                intent: SearchIntentCategory::Itinerary,
                category: QueryCategory::Itinerary,
                query: "",
                purpose: "参考短途路线节奏",
                priority: 0.65,
                freshness_required: false,
                structured_data_preferred: false,
            };
            let label = match days {
                1 => "一日游",
                2 => "两日游",
                _ => "三日游",
            };
            push(&template, format!("{city} {label} 路线"));
        }

        if input.month.is_some() || input.date_range.is_some() {
            let template = IntentTemplate {
                intent: SearchIntentCategory::Weather,
                category: QueryCategory::Activities,
                query: "",
                purpose: "补充目标日期的天气与季节影响",
                priority: 0.7,
                freshness_required: true,
                structured_data_preferred: true,
            };
            let query = input.date_range.as_ref().map_or_else(
                || format!("{city} {}月天气与出行建议", input.month.unwrap_or_default()),
                |range| format!("{city} {} 至 {} 天气与活动", range.start, range.end),
            );
            push(&template, query);
        }

        // 偏好一类只产生一个有语义的补缺查询，最多保留 3 个，控制普通旅行在 10 个以内。
        let preference_templates = [
            (
                "历史",
                SearchIntentCategory::CultureHistory,
                QueryCategory::Attractions,
                "历史与文化体验",
                "补充历史文化偏好",
            ),
            (
                "文化",
                SearchIntentCategory::CultureHistory,
                QueryCategory::Attractions,
                "文化体验",
                "补充文化偏好",
            ),
            (
                "美食",
                SearchIntentCategory::Food,
                QueryCategory::Food,
                "本地人常吃的代表性美食",
                "补充美食偏好",
            ),
            (
                "自然",
                SearchIntentCategory::Nature,
                QueryCategory::Attractions,
                "自然风景与半日游",
                "补充自然偏好",
            ),
            (
                "散步",
                SearchIntentCategory::LocalExperience,
                QueryCategory::Activities,
                "适合散步的街区与路线",
                "补充散步偏好",
            ),
            (
                "摄影",
                SearchIntentCategory::LocalExperience,
                QueryCategory::Activities,
                "摄影与日落机位",
                "补充摄影偏好",
            ),
            (
                "亲子",
                SearchIntentCategory::LocalExperience,
                QueryCategory::Activities,
                "亲子友好活动",
                "补充亲子偏好",
            ),
            (
                "购物",
                SearchIntentCategory::LocalExperience,
                QueryCategory::Activities,
                "本地商圈与特色购物",
                "补充购物偏好",
            ),
            (
                "夜生活",
                SearchIntentCategory::LocalExperience,
                QueryCategory::Activities,
                "夜间活动与夜景",
                "补充夜生活偏好",
            ),
            (
                "户外",
                SearchIntentCategory::Nature,
                QueryCategory::Attractions,
                "徒步与户外路线",
                "补充户外偏好",
            ),
            (
                "演出",
                SearchIntentCategory::Event,
                QueryCategory::Activities,
                "近期演出与活动",
                "补充活动偏好",
            ),
        ];
        for preference in input.preferences.iter().take(3) {
            let lowered = preference.to_lowercase();
            if let Some((_, intent, category, term, purpose)) = preference_templates
                .iter()
                .find(|(key, ..)| lowered.contains(key))
            {
                let template = IntentTemplate {
                    intent: *intent,
                    category: *category,
                    query: "",
                    purpose,
                    priority: 0.75,
                    freshness_required: *intent == SearchIntentCategory::Event,
                    structured_data_preferred: false,
                };
                push(&template, format!("{city} {term}"));
            }
        }

        tasks
    }
}

#[cfg(test)]
mod tests {
    use super::{QueryCategory, TravelDateRange, TravelQueryInput, TravelQueryPlanner};

    fn plan(city: &str, days: u8, month: Option<u32>, preferences: &[&str]) -> Vec<String> {
        TravelQueryPlanner::plan(&TravelQueryInput {
            city: city.to_string(),
            days,
            month,
            date_range: None,
            preferences: preferences.iter().map(|p| (*p).to_string()).collect(),
        })
        .into_iter()
        .map(|task| task.query)
        .collect()
    }

    #[test]
    fn empty_city_plan_is_empty() {
        assert!(plan("  ", 3, None, &[]).is_empty());
    }

    #[test]
    fn core_plan_is_small_and_semantic() {
        let queries = plan("杭州", 3, None, &[]);
        assert!(queries.len() <= 10);
        assert!(queries.iter().any(|q| q.contains("核心景点")));
        assert!(queries.iter().any(|q| q.contains("本地特色美食")));
    }

    #[test]
    fn itinerary_tasks_follow_requested_days() {
        let queries = plan("杭州", 3, None, &[]);
        assert!(queries.contains(&"杭州 三日游 路线".to_string()));
        assert!(!queries.contains(&"杭州 两日游 路线".to_string()));
        assert!(!queries.contains(&"杭州 一日游 路线".to_string()));
        // 一天行程只生成一日游
        let one_day = plan("苏州", 1, None, &[]);
        assert!(one_day.contains(&"苏州 一日游 路线".to_string()));
        assert!(
            !one_day
                .iter()
                .any(|query| query.contains("两日游") || query.contains("三日游"))
        );
    }

    #[test]
    fn month_appends_weather_and_activities() {
        let queries = plan("杭州", 3, Some(9), &[]);
        assert!(queries.contains(&"杭州 9月天气与出行建议".to_string()));
    }

    #[test]
    fn invalid_month_or_days_is_tolerated() {
        let queries = plan("杭州", 0, Some(13), &[]);
        assert!(queries.contains(&"杭州 一日游 路线".to_string()));
        assert!(queries.windows(2).all(|w| w[0] != w[1]), "no duplicates");
    }

    #[test]
    fn preferences_expand_with_matching_keywords() {
        let queries = plan("杭州", 3, None, &["历史", "美食", "散步"]);
        assert!(queries.contains(&"杭州 历史与文化体验".to_string()));
        assert!(queries.contains(&"杭州 本地人常吃的代表性美食".to_string()));
        assert!(queries.contains(&"杭州 适合散步的街区与路线".to_string()));
        // 未知偏好不产生额外查询
        let unknown = plan("杭州", 3, None, &["不知道什么偏好"]);
        assert_eq!(unknown.len(), plan("杭州", 3, None, &[]).len());
    }

    #[test]
    fn plan_is_deduplicated() {
        let queries = plan("杭州", 3, Some(3), &["历史"]);
        let unique: std::collections::HashSet<&String> = queries.iter().collect();
        assert_eq!(unique.len(), queries.len());
    }

    #[test]
    fn four_to_seven_days_do_not_search_fake_day_count() {
        for days in 4..=8 {
            let queries = plan("杭州", days, None, &[]);
            assert!(!queries.iter().any(|query| query.contains("日游")));
            assert!(queries.len() <= 10);
        }
    }

    #[test]
    fn date_range_appends_date_specific_queries() {
        let queries = TravelQueryPlanner::plan(&TravelQueryInput {
            city: "大连".to_string(),
            days: 3,
            month: Some(9),
            date_range: Some(TravelDateRange {
                start: "2026-09-05".to_string(),
                end: "2026-09-07".to_string(),
            }),
            preferences: vec![],
        })
        .into_iter()
        .map(|task| task.query)
        .collect::<Vec<_>>();
        assert!(queries.contains(&"大连 2026-09-05 至 2026-09-07 天气与活动".to_string()));
    }

    #[test]
    fn categories_are_assigned() {
        let tasks = TravelQueryPlanner::plan(&TravelQueryInput {
            city: "杭州".to_string(),
            days: 3,
            month: None,
            date_range: None,
            preferences: vec![],
        });
        assert!(
            tasks
                .iter()
                .any(|t| t.category == QueryCategory::Attractions && t.query.contains("景点"))
        );
        assert!(
            tasks
                .iter()
                .any(|t| t.category == QueryCategory::Transport && t.query.contains("交通"))
        );
        assert!(
            tasks
                .iter()
                .any(|t| t.category == QueryCategory::Warnings && t.query.contains("开放"))
        );
    }
}
