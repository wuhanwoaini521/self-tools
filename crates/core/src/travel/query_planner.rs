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
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QueryTask {
    pub category: QueryCategory,
    pub query: String,
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

/// 偏好 → 追加查询模板。键为偏好关键词（大小写不敏感、包含匹配）。
const PREFERENCE_TEMPLATES: &[(&str, &[&str])] = &[
    (
        "历史",
        &["{city} 历史景点", "{city} 历史街区", "{city} 古迹"],
    ),
    ("文化", &["{city} 文化体验", "{city} 文艺"]),
    (
        "美食",
        &["{city} 本地小吃", "{city} 老字号", "{city} 特色餐厅"],
    ),
    ("自然", &["{city} 自然风光", "{city} 公园", "{city} 山水"]),
    (
        "散步",
        &["{city} City Walk", "{city} 散步路线", "{city} 步行街"],
    ),
    (
        "摄影",
        &["{city} 拍照打卡", "{city} 摄影机位", "{city} 最佳拍摄点"],
    ),
    ("亲子", &["{city} 亲子游", "{city} 适合孩子"]),
    ("购物", &["{city} 商圈", "{city} 逛街"]),
    ("夜生活", &["{city} 夜景", "{city} 夜生活"]),
    ("户外", &["{city} 徒步", "{city} 骑行路线"]),
    ("演出", &["{city} 演出", "{city} 戏剧"]),
];

/// 基础模板（固定类别，保证覆盖面：介绍 / 景点 / 美食 / 住宿 / 交通 / 路线 / 活动 / 注意事项）。
const BASE_TEMPLATES: &[(QueryCategory, &[&str])] = &[
    (QueryCategory::CityIntro, &["{city} 城市介绍"]),
    (
        QueryCategory::Attractions,
        &[
            "{city} 必去景点",
            "{city} 热门景点",
            "{city} 小众景点",
            "{city} 博物馆",
            "{city} 历史文化景点",
        ],
    ),
    (
        QueryCategory::Food,
        &["{city} 美食", "{city} 本地特色美食", "{city} 本地人推荐"],
    ),
    (
        QueryCategory::Accommodation,
        &["{city} 住宿区域", "{city} 住哪里方便"],
    ),
    (
        QueryCategory::Transport,
        &[
            "{city} 地铁",
            "{city} 公交",
            "{city} 火车站",
            "{city} 机场",
            "{city} 交通攻略",
        ],
    ),
    (
        QueryCategory::Activities,
        &["{city} 最新活动", "{city} 文旅", "{city} 近期活动"],
    ),
    (
        QueryCategory::Warnings,
        &[
            "{city} 避坑",
            "{city} 旅游注意事项",
            "{city} 景区开放时间",
            "{city} 景区预约",
            "{city} 景区门票",
        ],
    ),
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

        let mut push = |category: QueryCategory, template: &str| {
            let query = template.replace("{city}", city);
            if seen.insert(query.clone()) {
                tasks.push(QueryTask { category, query });
            }
        };

        // 1. 基础类别
        for (category, templates) in BASE_TEMPLATES {
            for template in *templates {
                push(*category, template);
            }
        }

        // 2. 行程天数：N 日游 + (N-1) 日游 + 一日游兜底
        let days = input.days.clamp(1, 7);
        let mut day_counts = vec![1];
        if days >= 2 {
            day_counts.push(days - 1);
        }
        day_counts.push(days);
        day_counts.sort_unstable();
        day_counts.dedup();
        for count in day_counts {
            let template = match count {
                1 => "{city} 一日游",
                2 => "{city} 两日游",
                _ => "{city} 三日游",
            };
            push(QueryCategory::Itinerary, template);
        }

        // 3. 月份：当月天气 / 活动 / 应季玩法
        if let Some(month) = input.month.filter(|m| (1..=12).contains(m)) {
            push(QueryCategory::Activities, &format!("{city} {month}月 旅游"));
            push(QueryCategory::Activities, &format!("{city} {month}月 天气"));
            push(QueryCategory::Activities, &format!("{city} {month}月 活动"));
        }

        // 4. 具体日期：用真实日期查活动、客流与近期天气，供后续推荐避开不适合的安排。
        if let Some(range) = &input.date_range {
            let period = format!("{} 至 {}", range.start, range.end);
            push(QueryCategory::Activities, &format!("{city} {period} 天气"));
            push(QueryCategory::Activities, &format!("{city} {period} 活动"));
            push(
                QueryCategory::Warnings,
                &format!("{city} {period} 旅游 客流"),
            );
        }

        // 5. 偏好扩展
        for preference in &input.preferences {
            let matched = PREFERENCE_TEMPLATES
                .iter()
                .find(|(keyword, _)| preference.to_lowercase().contains(&keyword.to_lowercase()));
            if let Some((_, templates)) = matched {
                for template in *templates {
                    push(QueryCategory::Attractions, template);
                }
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
    fn base_categories_cover_traffic_food_and_warnings() {
        let queries = plan("杭州", 3, None, &[]);
        assert!(queries.contains(&"杭州 城市介绍".to_string()));
        assert!(queries.contains(&"杭州 必去景点".to_string()));
        assert!(queries.contains(&"杭州 美食".to_string()));
        assert!(queries.contains(&"杭州 住宿区域".to_string()));
        assert!(queries.contains(&"杭州 地铁".to_string()));
        assert!(queries.contains(&"杭州 交通攻略".to_string()));
        assert!(queries.contains(&"杭州 避坑".to_string()));
        assert!(queries.contains(&"杭州 景区预约".to_string()));
    }

    #[test]
    fn itinerary_tasks_follow_requested_days() {
        let queries = plan("杭州", 3, None, &[]);
        assert!(queries.contains(&"杭州 三日游".to_string()));
        assert!(queries.contains(&"杭州 两日游".to_string()));
        assert!(queries.contains(&"杭州 一日游".to_string()));
        // 一天行程只生成一日游
        let one_day = plan("苏州", 1, None, &[]);
        assert!(one_day.contains(&"苏州 一日游".to_string()));
        assert!(!one_day.contains(&"苏州 两日游".to_string()));
    }

    #[test]
    fn month_appends_weather_and_activities() {
        let queries = plan("杭州", 3, Some(9), &[]);
        assert!(queries.contains(&"杭州 9月 天气".to_string()));
        assert!(queries.contains(&"杭州 9月 活动".to_string()));
        assert!(queries.contains(&"杭州 9月 旅游".to_string()));
    }

    #[test]
    fn invalid_month_or_days_is_tolerated() {
        let queries = plan("杭州", 0, Some(13), &[]);
        assert!(queries.contains(&"杭州 一日游".to_string()));
        assert!(queries.windows(2).all(|w| w[0] != w[1]), "no duplicates");
    }

    #[test]
    fn preferences_expand_with_matching_keywords() {
        let queries = plan("杭州", 3, None, &["历史", "美食", "散步"]);
        assert!(queries.contains(&"杭州 历史街区".to_string()));
        assert!(queries.contains(&"杭州 老字号".to_string()));
        assert!(queries.contains(&"杭州 City Walk".to_string()));
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
        assert!(queries.contains(&"大连 2026-09-05 至 2026-09-07 天气".to_string()));
        assert!(queries.contains(&"大连 2026-09-05 至 2026-09-07 旅游 客流".to_string()));
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
                .any(|t| t.category == QueryCategory::Transport && t.query.contains("地铁"))
        );
        assert!(
            tasks
                .iter()
                .any(|t| t.category == QueryCategory::Warnings && t.query.contains("避坑"))
        );
    }
}
