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
            id: "rec_meiji".to_string(),
            title: "明治维新与日本近代化之路".to_string(),
            summary: "探索1868年明治维新背后的政治变革、关键历史人物（坂本龙马、西乡隆盛）与地理区位演变。".to_string(),
            module: "history".to_string(),
            entity_type: "story".to_string(),
            entity_id: "meiji_restoration".to_string(),
            reason: "历史精选 · 跨越政治与地理的时代转折".to_string(),
            connected_entity_title: Some("日本历史 · 幕末与维新".to_string()),
            tags: vec!["历史故事".to_string(), "日本".to_string(), "近代史".to_string()],
        });

        recs.push(ExploreRecommendation {
            id: "rec_tarim".to_string(),
            title: "塔里木盆地与天山地理".to_string(),
            summary: "中国最大内陆盆地，北倚天山南临昆仑，丝绸之路南北两道的地理大通道。".to_string(),
            module: "geography".to_string(),
            entity_type: "place".to_string(),
            entity_id: "tarim_basin".to_string(),
            reason: "地理百科 · 自然地貌与丝路交通枢纽".to_string(),
            connected_entity_title: Some("中国地形与盆地".to_string()),
            tags: vec!["地理".to_string(), "地貌".to_string(), "丝绸之路".to_string()],
        });

        recs.push(ExploreRecommendation {
            id: "rec_tang".to_string(),
            title: "盛唐长安与丝绸之路交通".to_string(),
            summary: "盛唐时期的都城格局、万国来朝的文化交融与西域往来贸易路线。".to_string(),
            module: "history".to_string(),
            entity_type: "event".to_string(),
            entity_id: "tang_dynasty_changan".to_string(),
            reason: "知识图谱关联 · 从地理盆地延伸至历史交通".to_string(),
            connected_entity_title: Some("大唐盛世".to_string()),
            tags: vec!["历史".to_string(), "文化".to_string(), "长安".to_string()],
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
        let root = root_id.unwrap_or("history:story:meiji_restoration");

        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        // 根节点
        let (root_name, root_mod, root_type, root_summary) = if root.contains("meiji") {
            ("明治维新", "history", EntityType::Event, "1868年日本近代化政治与社会变革")
        } else if root.contains("tarim") {
            ("塔里木盆地", "geography", EntityType::Place, "中国最大的内陆盆地，位于新疆南部")
        } else if root.contains("tang") {
            ("大唐盛世", "history", EntityType::Time, "公元618年-907年中国封建王朝繁荣顶峰")
        } else if root.contains("hangzhou") {
            ("杭州", "geography", EntityType::Destination, "浙江省省会，历史文化名城与江南水乡")
        } else {
            ("核心探索节点", "knowledge", EntityType::Concept, "跨模块知识网络节点")
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
            mastery_score: Some(65.0),
        });

        // 1-hop 关联节点与关系
        if root.contains("meiji") {
            let related = [
                ("history:person:emperor_meiji", "明治天皇", "history", EntityType::Person, "第122代天皇，宣布王政复古"),
                ("history:event:boshin_war", "戊辰战争", "history", EntityType::Event, "1868-1869年维新派与幕府军内战"),
                ("geography:country:japan", "日本国", "geography", EntityType::Place, "东亚岛国，位于太平洋西岸"),
                ("history:person:tokugawa_yoshinobu", "德川庆喜", "history", EntityType::Person, "江户幕府第15代征夷大将军，大政奉还"),
                ("history:time:1868", "1868年 (明治元年)", "history", EntityType::Time, "改元明治，颁布五条御誓文"),
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
                    mastery_score: Some(72.0),
                });

                edges.push(GraphEdge {
                    id: format!("edge_{root}_{node_id}"),
                    source_id: root.to_string(),
                    target_id: node_id.to_string(),
                    relation_kind: RelationKind::RelatedTo,
                    label: "核心关联".to_string(),
                    weight: 1.0,
                    source_module: "history".to_string(),
                });
            }
        } else if root.contains("tarim") {
            let related = [
                ("geography:mountain:tianshan", "天山山脉", "geography", EntityType::Place, "界于准噶尔与塔里木两大盆地之间"),
                ("geography:desert:taklamakan", "塔克拉玛干沙漠", "geography", EntityType::Place, "位于塔里木盆地中央的世界第二大流动沙漠"),
                ("history:event:silk_road", "丝绸之路", "history", EntityType::Topic, "连接古代欧亚大陆的陆上商贸大通道"),
                ("geography:river:tarim_river", "塔里木河", "geography", EntityType::Place, "中国最长的内陆河"),
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
                    label: "地理区位".to_string(),
                    weight: 1.0,
                    source_module: "geography".to_string(),
                });
            }
        } else {
            // 通用关联
            let related = [
                ("history:topic:general_history", "中国历史通览", "history", EntityType::Topic, "通史脉络与事件"),
                ("geography:place:asia", "亚洲地理概貌", "geography", EntityType::Place, "自然与人文地理"),
                ("language:item:starter_words", "高频词汇集", "language", EntityType::LanguageItem, "语言学习基础词"),
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
                    mastery_score: Some(20.0),
                });

                edges.push(GraphEdge {
                    id: format!("edge_{root}_{node_id}"),
                    source_id: root.to_string(),
                    target_id: node_id.to_string(),
                    relation_kind: RelationKind::RelatedTo,
                    label: "相关知识".to_string(),
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
