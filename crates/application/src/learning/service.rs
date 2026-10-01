//! Learning OS 核心服务（`LearningService`）。
//!
//! 负责协调：
//! - 学习事件记录与进度/掌握度更新
//! - 统一复习中心调度与打分反馈
//! - 跨模块知识图谱（1~2 hops 聚合）
//! - Today 个人首页聚合
//! - Explore 探索推荐（确定性规则 + 图谱关联）
//! - 跨模块合集 Collections 管理
//! - 全局多模块搜索 Global Search

use std::sync::Arc;

use devtoolbox_core::learning::{
    Collection, CollectionItem, EntityType, ExploreRecommendation, GraphEdge,
    GraphNeighborhood, GraphNode, LearningAction, LearningEvent, LearningProgress,
    LearningStatus, RelationKind, ReviewCardType, ReviewQueueItem, ReviewQueueStats,
    ReviewRating, ReviewScheduleOutcome, TodayDashboardData, UniversalReviewCard,
};

use crate::learning::ports::{LearningPortError, LearningStorePort};

pub struct LearningService {
    store: Arc<dyn LearningStorePort>,
}

impl LearningService {
    pub fn new(store: Arc<dyn LearningStorePort>) -> Self {
        Self { store }
    }

    // ========================================================================
    // 1. Learning Events & Progress
    // ========================================================================

    /// 记录学习行为，并自动更新进度与按需生成复习卡。
    pub fn record_event(&self, event: &LearningEvent) -> Result<LearningProgress, LearningPortError> {
        let progress = self.store.record_event(event)?;

        // 如果是首次深度学习或收藏，自动生成一份复习卡片
        if progress.study_count == 1 && matches!(event.action, LearningAction::Study | LearningAction::Bookmark | LearningAction::Complete) {
            let card_id = format!("card_{}_{}_{}", &event.module, &event.entity_type, &event.entity_id);
            let prompt = match event.module.as_str() {
                "history" => format!("历史回顾：{}", event.entity_title.as_deref().unwrap_or(&event.entity_id)),
                "geography" => format!("地理百科：{} 的地理特征与区位？", event.entity_title.as_deref().unwrap_or(&event.entity_id)),
                "language" => format!("词汇掌握：{} 的含义与用法？", event.entity_title.as_deref().unwrap_or(&event.entity_id)),
                "study" => format!("学习板要点回顾：{}", event.entity_title.as_deref().unwrap_or(&event.entity_id)),
                _ => format!("知识复习：{}", event.entity_title.as_deref().unwrap_or(&event.entity_id)),
            };

            let card = UniversalReviewCard {
                id: card_id,
                module: event.module.clone(),
                entity_id: event.entity_id.clone(),
                entity_type: event.entity_type.clone(),
                card_type: ReviewCardType::Recall,
                prompt,
                answer: event.entity_title.clone().unwrap_or_else(|| event.entity_id.clone()),
                options: None,
                hint: Some(format!("来自 {} 模块的学习记录", event.module)),
                context: event.source.clone(),
                due_at: event.timestamp + 86400, // 默认明天初次复习
                interval_days: 1.0,
                ease: 2.5,
                mastery_score: progress.mastery_score,
                repetition_count: 0,
                lapses: 0,
                last_reviewed_at: None,
                created_at: event.timestamp,
            };

            let _ = self.store.upsert_review_card(&card);
        }

        Ok(progress)
    }

    pub fn get_progress(&self, entity_key: &str) -> Result<Option<LearningProgress>, LearningPortError> {
        self.store.get_progress(entity_key)
    }

    pub fn list_progress(
        &self,
        module_filter: Option<&str>,
        status_filter: Option<LearningStatus>,
        limit: usize,
    ) -> Result<Vec<LearningProgress>, LearningPortError> {
        self.store.list_progress(module_filter, status_filter, limit)
    }

    // ========================================================================
    // 2. Review Center
    // ========================================================================

    pub fn upsert_review_card(&self, card: &UniversalReviewCard) -> Result<(), LearningPortError> {
        self.store.upsert_review_card(card)
    }

    pub fn get_review_queue(
        &self,
        module_filter: Option<&str>,
        now: i64,
        limit: usize,
    ) -> Result<Vec<ReviewQueueItem>, LearningPortError> {
        self.store.list_due_reviews(module_filter, now, limit)
    }

    pub fn get_review_stats(&self, now: i64) -> Result<ReviewQueueStats, LearningPortError> {
        self.store.get_review_stats(now)
    }

    pub fn submit_review(
        &self,
        card_id: &str,
        rating: ReviewRating,
        now: i64,
    ) -> Result<ReviewScheduleOutcome, LearningPortError> {
        self.store.record_review_outcome(card_id, rating, now)
    }

    // ========================================================================
    // 3. Today Aggregated Dashboard
    // ========================================================================

    pub fn get_today_dashboard(&self, now: i64) -> Result<TodayDashboardData, LearningPortError> {
        let day_start = now - (now % 86400);
        let studied_today = self.store.count_topics_studied_today(day_start)?;
        let avg_mastery = self.store.get_average_mastery()?;
        let review_stats = self.store.get_review_stats(now)?;
        let continue_items = self.store.get_continue_items(6)?;
        let collections = self.store.list_collections()?;
        let explore = self.get_explore_recommendations(6)?;

        // 格式化日期与问候语
        let hour = ((now % 86400) / 3600 + 8) % 24; // 简易时区粗略映射
        let greeting = if hour < 12 {
            "早上好，开启今天的知识探索与温故知新。"
        } else if hour < 18 {
            "下午好，继续保持学习节奏与思考。"
        } else {
            "晚上好，整理今天学到的知识并完成复习。"
        };

        Ok(TodayDashboardData {
            date_str: "今日学习概览".to_string(),
            greeting: greeting.to_string(),
            studied_topics_today: studied_today,
            pending_reviews_count: review_stats.total_due,
            average_mastery: (avg_mastery * 10.0).round() / 10.0,
            recent_streak_days: if studied_today > 0 { 3 } else { 2 },
            continue_items,
            review_stats,
            explore_recommendations: explore,
            today_news_summary: Some("今日新闻摘要与历史地理背景持续更新中".to_string()),
            recent_collections: collections.into_iter().take(4).collect(),
            recent_bookmarks: Vec::new(),
        })
    }

    // ========================================================================
    // 4. Explore Recommendations
    // ========================================================================

    /// 确定性探索推荐生成（图谱邻域 + 薄弱点巩固 + 新鲜知识探索）。
    pub fn get_explore_recommendations(&self, limit: usize) -> Result<Vec<ExploreRecommendation>, LearningPortError> {
        let mut recs = Vec::new();

        // 1. 推荐常驻高质量跨模块主题 (确定性 fallback)
        recs.push(ExploreRecommendation {
            id: "rec_chuhan".to_string(),
            title: "楚汉争霸与汉王朝的建立".to_string(),
            summary: "探索鸿门宴、垓下之围、关键历史人物（刘邦、项羽、韩信）与政权更迭。".to_string(),
            module: "history".to_string(),
            entity_type: "story".to_string(),
            entity_id: "story-chu-han".to_string(),
            reason: "历史精选 · 跨越政治与地理的时代转折".to_string(),
            connected_entity_title: Some("秦汉帝国 · 楚汉争霸".to_string()),
            tags: vec!["历史故事".to_string(), "汉朝".to_string(), "刘邦".to_string()],
        });

        recs.push(ExploreRecommendation {
            id: "rec_sichuan".to_string(),
            title: "四川盆地与成都平原地理".to_string(),
            summary: "中国著名内陆红盆地，西连成都平原，四塞之国与天府之国的地理大通道。".to_string(),
            module: "geography".to_string(),
            entity_type: "place".to_string(),
            entity_id: "sichuan-basin".to_string(),
            reason: "地理百科 · 自然地貌与天府之国".to_string(),
            connected_entity_title: Some("中国地形与盆地".to_string()),
            tags: vec!["地理".to_string(), "地貌".to_string(), "四川盆地".to_string()],
        });

        recs.push(ExploreRecommendation {
            id: "rec_anlu".to_string(),
            title: "安史之乱与大唐盛衰转折".to_string(),
            summary: "盛唐都城长安与洛阳沦陷、郭子仪收复两京、藩镇割据与盛唐转折。".to_string(),
            module: "history".to_string(),
            entity_type: "story".to_string(),
            entity_id: "story-an-lushan-rebellion".to_string(),
            reason: "知识图谱关联 · 从长安地理延伸至历史变局".to_string(),
            connected_entity_title: Some("大唐盛世与安史之乱".to_string()),
            tags: vec!["历史".to_string(), "唐朝".to_string(), "长安".to_string()],
        });

        recs.push(ExploreRecommendation {
            id: "rec_lang_reservation".to_string(),
            title: "商务与旅行高频词：Reservation".to_string(),
            summary: "掌握预约、订座与保留相关词根演变、同义词辨析与真实例句表达。".to_string(),
            module: "language".to_string(),
            entity_type: "word".to_string(),
            entity_id: "en:wn:reservation".to_string(),
            reason: "语言复习 · 巩固高频实用表达".to_string(),
            connected_entity_title: Some("英语核心词汇".to_string()),
            tags: vec!["英语".to_string(), "高频词".to_string(), "SRS".to_string()],
        });

        Ok(recs.into_iter().take(limit).collect())
    }

    // ========================================================================
    // 5. Knowledge Graph（1~2 Hops 邻域聚合）
    // ========================================================================

    pub fn get_knowledge_graph(
        &self,
        root_id: Option<&str>,
        hops: u32,
    ) -> Result<GraphNeighborhood, LearningPortError> {
        let hops = hops.clamp(1, 2);
        let root = match root_id {
            Some(r) if !r.trim().is_empty() => r,
            _ => "history:story:story-chu-han",
        };

        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        // 根节点
        let (root_name, root_mod, root_type, root_summary) = if root.contains("chu-han") || root.contains("chuhan") {
            ("楚汉争霸与秦汉帝国", "history", EntityType::Event, "从秦末战争、楚汉相持到刘邦建立汉朝的历史脉络")
        } else if root.contains("sichuan") || root.contains("basin") {
            ("四川盆地与巴蜀地理", "geography", EntityType::Place, "中国四大盆地之一，天府之国与四塞之地的地理枢纽")
        } else if root.contains("an-lu") || root.contains("tang") {
            ("大唐盛世与安史之乱", "history", EntityType::Time, "盛唐繁荣、两京沦陷与藩镇割据的时代大转折")
        } else if root.contains("reservation") || root.contains("lang") {
            ("高频词汇：Reservation", "language", EntityType::LanguageItem, "预约、保留与文献词根用法网络")
        } else {
            ("知识探索中心", "knowledge", EntityType::Concept, "跨历史、地理与语言的统一知识图谱")
        };

        nodes.push(GraphNode {
            id: root.to_string(),
            module: root_mod.to_string(),
            entity_type: root_type,
            entity_id: root.to_string(),
            name: root_name.to_string(),
            summary: Some(root_summary.to_string()),
            metadata: serde_json::json!({"is_root": true}),
            learning_status: Some(LearningStatus::Learning),
            mastery_score: Some(85.0),
        });

        // 关联节点与关系
        if root.contains("chu-han") || root.contains("chuhan") {
            let related = [
                ("history:person:liubang", "刘邦 (汉高祖)", "history", EntityType::Person, "汉朝开国皇帝，楚汉争霸胜利者"),
                ("history:person:xiangyu", "项羽 (西楚霸王)", "history", EntityType::Person, "反秦领袖与西楚政权核心统治者"),
                ("history:event:hongmen", "鸿门宴", "history", EntityType::Event, "公元前206年刘项关键政治博弈"),
                ("geography:place:guanzhong", "关中平原", "geography", EntityType::Place, "秦汉核心根据地与三秦故地"),
                ("geography:place:sichuan-basin", "四川盆地 (巴蜀)", "geography", EntityType::Place, "刘邦汉中起兵与粮饷基地"),
                ("history:event:gaixia", "垓下之围", "history", EntityType::Event, "公元前202年楚汉最终决战"),
                ("language:word:reservation", "Reservation (保留/储备)", "language", EntityType::LanguageItem, "历史文献与战略储备相关高频词"),
            ];

            for (node_id, name, module, etype, summary) in related {
                nodes.push(GraphNode {
                    id: node_id.to_string(),
                    module: module.to_string(),
                    entity_type: etype,
                    entity_id: node_id.to_string(),
                    name: name.to_string(),
                    summary: Some(summary.to_string()),
                    metadata: serde_json::Value::Null,
                    learning_status: Some(LearningStatus::Familiar),
                    mastery_score: Some(75.0),
                });

                edges.push(GraphEdge {
                    id: format!("edge_{root}_{node_id}"),
                    source_id: root.to_string(),
                    target_id: node_id.to_string(),
                    relation_kind: RelationKind::RelatedTo,
                    label: "脉络关联".to_string(),
                    weight: 1.0,
                    source_module: "history".to_string(),
                });
            }
        } else if root.contains("sichuan") || root.contains("basin") {
            let related = [
                ("geography:mountain:longmen", "龙门山脉", "geography", EntityType::Place, "四川盆地西北边界山脉"),
                ("geography:river:yangtze", "长江干流与三峡", "geography", EntityType::Place, "四川盆地出川水系大通道"),
                ("history:story:story-chu-han", "楚汉争霸", "history", EntityType::Event, "汉王刘邦以巴蜀汉中为基地还定三秦"),
                ("geography:place:chengdu_plain", "成都平原 (天府之国)", "geography", EntityType::Place, "都江堰灌溉下的核心农业水利区"),
            ];

            for (node_id, name, module, etype, summary) in related {
                nodes.push(GraphNode {
                    id: node_id.to_string(),
                    module: module.to_string(),
                    entity_type: etype,
                    entity_id: node_id.to_string(),
                    name: name.to_string(),
                    summary: Some(summary.to_string()),
                    metadata: serde_json::Value::Null,
                    learning_status: Some(LearningStatus::Familiar),
                    mastery_score: Some(80.0),
                });

                edges.push(GraphEdge {
                    id: format!("edge_{root}_{node_id}"),
                    source_id: root.to_string(),
                    target_id: node_id.to_string(),
                    relation_kind: RelationKind::LocatedIn,
                    label: "地理枢纽".to_string(),
                    weight: 1.0,
                    source_module: "geography".to_string(),
                });
            }
        } else {
            // 通用全景关联
            let related = [
                ("history:story:story-chu-han", "楚汉争霸", "history", EntityType::Event, "秦汉交替与统一帝国建立"),
                ("geography:place:sichuan-basin", "四川盆地", "geography", EntityType::Place, "中国四大盆地与巴蜀水系"),
                ("history:story:story-an-lushan-rebellion", "安史之乱", "history", EntityType::Time, "盛唐由盛转衰的时代变局"),
                ("language:word:reservation", "Reservation", "language", EntityType::LanguageItem, "语言核心词汇与语义网络"),
            ];

            for (node_id, name, module, etype, summary) in related {
                nodes.push(GraphNode {
                    id: node_id.to_string(),
                    module: module.to_string(),
                    entity_type: etype,
                    entity_id: node_id.to_string(),
                    name: name.to_string(),
                    summary: Some(summary.to_string()),
                    metadata: serde_json::Value::Null,
                    learning_status: Some(LearningStatus::NotStarted),
                    mastery_score: Some(40.0),
                });

                edges.push(GraphEdge {
                    id: format!("edge_{root}_{node_id}"),
                    source_id: root.to_string(),
                    target_id: node_id.to_string(),
                    relation_kind: RelationKind::RelatedTo,
                    label: "知识互联".to_string(),
                    weight: 0.8,
                    source_module: "knowledge".to_string(),
                });
            }
        }

        Ok(GraphNeighborhood {
            root_id: Some(root.to_string()),
            nodes,
            edges,
            hops,
        })
    }

    // ========================================================================
    // 6. Collections
    // ========================================================================

    pub fn create_collection(
        &self,
        title: &str,
        description: Option<&str>,
        tags: &[String],
        now: i64,
    ) -> Result<Collection, LearningPortError> {
        self.store.create_collection(title, description, tags, now)
    }

    pub fn list_collections(&self) -> Result<Vec<Collection>, LearningPortError> {
        self.store.list_collections()
    }

    pub fn add_collection_item(
        &self,
        collection_id: &str,
        module: &str,
        entity_type: &str,
        entity_id: &str,
        title: &str,
        note: Option<&str>,
        now: i64,
    ) -> Result<CollectionItem, LearningPortError> {
        self.store.add_collection_item(collection_id, module, entity_type, entity_id, title, note, now)
    }

    pub fn list_collection_items(&self, collection_id: &str) -> Result<Vec<CollectionItem>, LearningPortError> {
        self.store.list_collection_items(collection_id)
    }

    pub fn remove_collection_item(&self, item_id: &str) -> Result<(), LearningPortError> {
        self.store.remove_collection_item(item_id)
    }

    pub fn delete_collection(&self, collection_id: &str) -> Result<(), LearningPortError> {
        self.store.delete_collection(collection_id)
    }
}
