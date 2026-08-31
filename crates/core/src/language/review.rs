//! 复习调度（#60/#61/#67-#68 的学习状态）。
//!
//! V1 采用简单 SRS（SM-2 变体）：rating 0–3，interval 按 ease 增长；
//! 接口隔离在纯函数 `schedule` 上，未来可整体替换为 FSRS（#60 架构预留）。

use serde::{Deserialize, Serialize};

/// 学习状态（#59：与词典数据分离）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningStateKind {
    New,
    Learning,
    Review,
    Mastered,
}

impl LearningStateKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::New => "新词",
            Self::Learning => "学习中",
            Self::Review => "复习中",
            Self::Mastered => "已掌握",
        }
    }
}

/// 复习评分（简单 SRS）。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewRating {
    /// 忘了（重新学）。
    Again = 0,
    /// 困难。
    Hard = 1,
    /// 记得。
    Good = 2,
    /// 轻松。
    Easy = 3,
}

impl ReviewRating {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Again => "忘记",
            Self::Hard => "困难",
            Self::Good => "记得",
            Self::Easy => "轻松",
        }
    }

    /// 等级 → SM-2 quality（0..=5）。
    #[must_use]
    pub const fn quality(self) -> f64 {
        match self {
            Self::Again => 0.0,
            Self::Hard => 2.5,
            Self::Good => 4.0,
            Self::Easy => 5.0,
        }
    }
}

/// 一条学习记录（词典无关，按 item_id 关联）。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LearningState {
    pub item_id: String,
    pub state: LearningStateKind,
    pub interval_days: f64,
    pub ease: f64,
    /// 下次复习时间（unix 秒），到期即进入 Today 队列。
    pub due_at: i64,
    pub review_count: i64,
    pub lapses: i64,
    pub started_at: i64,
    pub updated_at: i64,
}

impl LearningState {
    #[must_use]
    pub fn new(item_id: impl Into<String>, now: i64) -> Self {
        Self {
            item_id: item_id.into(),
            state: LearningStateKind::New,
            interval_days: 0.0,
            ease: 2.5,
            due_at: now,
            review_count: 0,
            lapses: 0,
            started_at: now,
            updated_at: now,
        }
    }
}

/// 复习打分结果（#68 的存储依据）。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReviewOutcome {
    pub state: LearningStateKind,
    pub interval_days: f64,
    pub ease: f64,
    pub due_at: i64,
    pub lapses: i64,
}

/// 简单 SRS 调度器（纯函数；`now` 由调用方注入保证可测）。
#[derive(Clone, Copy, Debug, Default)]
pub struct ReviewScheduler;

impl ReviewScheduler {
    /// 对当前学习记录应用一次评分，返回更新（不动原记录）。
    #[must_use]
    pub fn schedule(current: &LearningState, rating: ReviewRating, now: i64) -> ReviewOutcome {
        let quality = rating.quality();
        let ease =
            (current.ease + (0.1 - (5.0 - quality) * (0.08 + (5.0 - quality) * 0.02))).max(1.3);
        let (interval_days, lapses, state) = match rating {
            ReviewRating::Again => {
                let lapses = current.lapses + 1;
                let interval = if current.review_count == 0 || current.interval_days < 1.0 {
                    0.0 // 首次再次失败：当天重学
                } else {
                    1.0
                };
                let state = if current.state == LearningStateKind::Mastered {
                    LearningStateKind::Review
                } else {
                    LearningStateKind::Learning
                };
                (interval, lapses, state)
            }
            ReviewRating::Hard => {
                let interval = if current.review_count == 0 {
                    1.0
                } else {
                    (current.interval_days * ease * 0.8).max(1.0)
                };
                (interval, current.lapses, LearningStateKind::Review)
            }
            ReviewRating::Good => {
                let interval = if current.review_count == 0 {
                    1.0
                } else {
                    (current.interval_days * ease).max(1.0)
                };
                (interval, current.lapses, LearningStateKind::Review)
            }
            ReviewRating::Easy => {
                let interval = if current.review_count == 0 {
                    3.0
                } else {
                    (current.interval_days * ease * 1.3).max(1.0)
                };
                (interval, current.lapses, LearningStateKind::Review)
            }
        };
        let state = if interval_days >= 21.0 && rating != ReviewRating::Again {
            LearningStateKind::Mastered
        } else {
            state
        };
        let due_at = now + (interval_days * 86_400.0).round() as i64;
        ReviewOutcome {
            state,
            interval_days,
            ease,
            due_at,
            lapses,
        }
    }

    /// 到期未复习超过 N 天的记录进入「复习中」，重新待复习（简化：由上层轮询拉取）。
    #[must_use]
    pub fn is_due(record: &LearningState, now: i64) -> bool {
        record.due_at <= now
    }
}

/// 每日学习计划（Today，#61）。
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TodayPlan {
    pub due_reviews: i64,
    pub new_words: i64,
    pub sentences: i64,
    pub listening: i64,
    pub speaking: i64,
    pub total: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn record() -> LearningState {
        LearningState::new("en:wn:reservation", NOW)
    }

    #[test]
    fn new_word_good_schedules_one_day() {
        let outcome = ReviewScheduler::schedule(&record(), ReviewRating::Good, NOW);
        assert_eq!(outcome.interval_days, 1.0);
        assert_eq!(outcome.due_at, NOW + 86_400);
        assert_eq!(outcome.state, LearningStateKind::Review);
    }

    #[test]
    fn again_resets_and_lapses() {
        let mut current = record();
        current.review_count = 5;
        current.interval_days = 10.0;
        let outcome = ReviewScheduler::schedule(&current, ReviewRating::Again, NOW);
        assert_eq!(outcome.lapses, 1);
        assert_eq!(outcome.interval_days, 1.0);
        assert_eq!(outcome.state, LearningStateKind::Learning);
    }

    #[test]
    fn mastered_after_long_interval() {
        let current = LearningState {
            interval_days: 18.0,
            ease: 2.5,
            review_count: 9,
            ..record()
        };
        let outcome = ReviewScheduler::schedule(&current, ReviewRating::Good, NOW);
        assert_eq!(outcome.state, LearningStateKind::Mastered);
        assert!(outcome.interval_days > 21.0);
    }

    #[test]
    fn due_by_time() {
        let current = record();
        assert!(ReviewScheduler::is_due(&current, NOW));
        let future = LearningState {
            due_at: NOW + 86400,
            ..record()
        };
        assert!(!ReviewScheduler::is_due(&future, NOW));
    }

    #[test]
    fn easy_first_review_three_days() {
        let outcome = ReviewScheduler::schedule(&record(), ReviewRating::Easy, NOW);
        assert_eq!(outcome.interval_days, 3.0);
    }
}
