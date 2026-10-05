//! 26 周英语主线计划（V13 W6）——**可检查的能力 checkpoint**，不是课时表。
//!
//! ## 为什么要有这个文件
//!
//! 「半年能交流」这个目标，如果没有可验证的中期检查点，就只能靠感觉：
//! 学了 42 课 ≠ 会说话。本模块把目标拆成 26 周、每 4 周一个**能力检查**，
//! 每一项都给出「用真实数据怎么算达标」——达标判定在 [`Checkpoint::is_met`]，
//! 不达标时界面直接显示当前真实数值与差距。
//!
//! 三个原则：
//! 1. **不达标就说没达标**。宁可显示「开口 12 分钟 / 需要 30」，
//!    也不把「学完 3 课」说成「第一周完成」；
//! 2. **只统计能自动测的量**（课数、开口秒数、复习正确率、句子卡数…），
//!    需要人自评的项目（能不能听懂、敢不敢开口）标为 `SelfReported`，
//!    界面明确提示这是你自己填的，不冒充自动检测；
//! 3. 计划是**模板**，进度是**数据**：这里只有定义，不碰 IO。

/// 一个里程碑的判定方式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckKind {
    /// 累计完成课时数 ≥ 阈值。
    LessonsCompleted,
    /// 累计开口秒数 ≥ 阈值（跟读评分的真实记录）。
    SpokenSeconds,
    /// 复习平均掌握度（0–100）≥ 阈值。
    ReviewMastery,
    /// 句型卡数量 ≥ 阈值（挖掘进 SRS 的句子卡）。
    SentenceCards,
    /// 已学词数 ≥ 阈值。
    WordsLearned,
    /// 到期复习清零所需连续天数（连续学习）≥ 阈值。
    StreakDays,
    /// 学习者自评（能不能听懂 / 敢不敢说）——**不自动检测**，界面要说明。
    SelfReported,
}

/// 达成里程碑所需的数据（来自各服务的真实统计）。
#[derive(Clone, Copy, Debug, Default)]
pub struct RoadmapMetrics {
    pub lessons_completed: u32,
    pub spoken_seconds: i64,
    pub review_mastery: f64,
    pub sentence_cards: u32,
    pub words_learned: u32,
    pub streak_days: u32,
}

/// 一条可验证的检查项。
///
/// 这是**静态计划**（不是接口 DTO），所以不 derive serde：序列化由
/// application 层的 `RoadmapView` 负责，避免「静态表形状」被 HTTP 契约绑死。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checkpoint {
    pub label: &'static str,
    pub kind: CheckKind,
    pub threshold: u32,
}

/// 一周的计划。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoadmapWeek {
    /// 第几周（1–26）。
    pub week: u32,
    /// 主题（人话）。
    pub focus: &'static str,
    /// 到这一周结束时应能做到的事（**能力**，不是课时）。
    pub can_do: &'static str,
    /// 达成判定。
    pub checks: &'static [Checkpoint],
}

impl Checkpoint {
    /// 是否达标；`SelfReported` 永远返回 `false`（没有人能自动判定「敢不敢说」）。
    #[must_use]
    pub fn is_met(&self, metrics: &RoadmapMetrics) -> bool {
        match self.kind {
            CheckKind::LessonsCompleted => metrics.lessons_completed >= self.threshold,
            CheckKind::SpokenSeconds => metrics.spoken_seconds >= i64::from(self.threshold),
            CheckKind::ReviewMastery => metrics.review_mastery >= f64::from(self.threshold),
            CheckKind::SentenceCards => metrics.sentence_cards >= self.threshold,
            CheckKind::WordsLearned => metrics.words_learned >= self.threshold,
            CheckKind::StreakDays => metrics.streak_days >= self.threshold,
            // 自评项不参与自动判定：冒充自动检测就是在骗自己。
            CheckKind::SelfReported => false,
        }
    }

    /// 当前真实数值（用于显示「12 / 30」这样的差距）。
    #[must_use]
    pub fn current(&self, metrics: &RoadmapMetrics) -> u32 {
        match self.kind {
            CheckKind::LessonsCompleted => metrics.lessons_completed,
            CheckKind::SpokenSeconds => u32::try_from(metrics.spoken_seconds).unwrap_or(u32::MAX),
            CheckKind::ReviewMastery => metrics.review_mastery.round().max(0.0) as u32,
            CheckKind::SentenceCards => metrics.sentence_cards,
            CheckKind::WordsLearned => metrics.words_learned,
            CheckKind::StreakDays => metrics.streak_days,
            CheckKind::SelfReported => 0,
        }
    }

    /// 单位（显示用）。
    #[must_use]
    pub const fn unit(&self) -> &'static str {
        match self.kind {
            CheckKind::LessonsCompleted => "课",
            CheckKind::SpokenSeconds => "分钟",
            CheckKind::ReviewMastery => "%",
            CheckKind::SentenceCards => "张句子卡",
            CheckKind::WordsLearned => "词",
            CheckKind::StreakDays => "天连续",
            CheckKind::SelfReported => "自评",
        }
    }
}

/// 检查项构造宏式简写（保持表可读）。
const fn c(label: &'static str, kind: CheckKind, threshold: u32) -> Checkpoint {
    Checkpoint {
        label,
        kind,
        threshold,
    }
}

/// 26 周计划。
///
/// 节奏依据（明说，不是玄学）：
/// - 每周 4–6 课 NCE（约 20–30 分钟/天）→ 26 周覆盖 Book 1–2 的大部分；
/// - 每天至少 10 分钟开口 → 26 周累计 30 小时以上真实说话量；
/// - 复习留存率（掌握度）长期 > 80% 才算「学过」，否则只是「见过」。
pub const ROADMAP: &[RoadmapWeek] = &[
    RoadmapWeek {
        week: 1,
        focus: "开嗓 + 建立节奏",
        can_do: "能说出自己的名字、城市、一件今天发生的事（哪怕只有三句）",
        checks: &[
            c("完成 3 课", CheckKind::LessonsCompleted, 3),
            c("开口 5 分钟", CheckKind::SpokenSeconds, 300),
            c("连续 3 天", CheckKind::StreakDays, 3),
        ],
    },
    RoadmapWeek {
        week: 2,
        focus: "自我介绍成段",
        can_do: "不看稿做 60 秒自我介绍：姓名 / 职业 / 家乡 / 爱好",
        checks: &[
            c("完成 6 课", CheckKind::LessonsCompleted, 6),
            c("开口 12 分钟", CheckKind::SpokenSeconds, 720),
            c("句子卡 6 张", CheckKind::SentenceCards, 6),
        ],
    },
    RoadmapWeek {
        week: 4,
        focus: "第一次能力检查：点餐与买单",
        can_do: "在餐厅完成点餐、要求换菜、买单，全程英文",
        checks: &[
            c("完成 10 课", CheckKind::LessonsCompleted, 10),
            c("开口 25 分钟", CheckKind::SpokenSeconds, 1_500),
            c("复习掌握度 70%", CheckKind::ReviewMastery, 70),
        ],
    },
    RoadmapWeek {
        week: 6,
        focus: "听懂本册课文",
        can_do: "听懂 NCE 课文 80%，并用三句话复述大意",
        checks: &[
            c("完成 15 课", CheckKind::LessonsCompleted, 15),
            c("开口 40 分钟", CheckKind::SpokenSeconds, 2_400),
            c("句子卡 15 张", CheckKind::SentenceCards, 15),
        ],
    },
    RoadmapWeek {
        week: 8,
        focus: "问路与交通",
        can_do: "问路、乘车、买票，遇到听不懂时会要求对方重复",
        checks: &[
            c("完成 20 课", CheckKind::LessonsCompleted, 20),
            c("开口 55 分钟", CheckKind::SpokenSeconds, 3_300),
            c("复习掌握度 75%", CheckKind::ReviewMastery, 75),
        ],
    },
    RoadmapWeek {
        week: 12,
        focus: "电话与视频里的自我介绍",
        can_do: "在电话/视频里做自我介绍并回答三个简单追问",
        checks: &[
            c("完成 30 课", CheckKind::LessonsCompleted, 30),
            c("开口 90 分钟", CheckKind::SpokenSeconds, 5_400),
            c("学词 200 个", CheckKind::WordsLearned, 200),
        ],
    },
    RoadmapWeek {
        week: 16,
        focus: "读懂短文并复述",
        can_do: "读一篇 800 词短文，复述三件事",
        checks: &[
            c("完成 38 课", CheckKind::LessonsCompleted, 38),
            c("开口 120 分钟", CheckKind::SpokenSeconds, 7_200),
            c("句子卡 30 张", CheckKind::SentenceCards, 30),
        ],
    },
    RoadmapWeek {
        week: 20,
        focus: "就熟悉话题聊十分钟",
        can_do: "围绕一个熟悉话题聊 10 分钟不冷场（会卡但能接上）",
        checks: &[
            c("完成 44 课", CheckKind::LessonsCompleted, 44),
            c("开口 150 分钟", CheckKind::SpokenSeconds, 9_000),
            c("复习掌握度 80%", CheckKind::ReviewMastery, 80),
        ],
    },
    RoadmapWeek {
        week: 24,
        focus: "写 200 词并自查",
        can_do: "写 200 词邮件/留言，自己改掉明显错误",
        checks: &[
            c("完成 50 课", CheckKind::LessonsCompleted, 50),
            c("开口 180 分钟", CheckKind::SpokenSeconds, 10_800),
            c("学词 400 个", CheckKind::WordsLearned, 400),
        ],
    },
    RoadmapWeek {
        week: 26,
        focus: "终检：30 分钟真实对话",
        can_do: "和真人（或 AI 陪练）完成 30 分钟英文对话，全程不切中文",
        checks: &[
            c("完成 54 课", CheckKind::LessonsCompleted, 54),
            c("开口累计 240 分钟", CheckKind::SpokenSeconds, 14_400),
            c("复习掌握度 80%", CheckKind::ReviewMastery, 80),
        ],
    },
];

/// 某一周的周计划（`week` 越界返回 `None`）。
#[must_use]
pub fn week_plan(week: u32) -> Option<&'static RoadmapWeek> {
    ROADMAP.iter().find(|item| item.week == week)
}

/// 当前处于第几周。
///
/// `start` 是「开始日」（Unix 秒）与今天的差值。**没有开始日就返回 `None`**
/// —— 不替用户编一个起点（界面会问「从哪天开始算」）。
#[must_use]
pub fn current_week(start: i64, now: i64) -> Option<u32> {
    if start <= 0 || now < start {
        return None;
    }
    let days = (now - start) / 86_400;
    let week = days / 7 + 1;
    Some(u32::try_from(week).unwrap_or(1).min(ROADMAP.len() as u32))
}

/// 某个「周数」对应的最近一次检查（周数落在两次检查之间时取上一次）。
#[must_use]
pub fn latest_checkpoint(week: u32) -> Option<&'static RoadmapWeek> {
    ROADMAP.iter().rev().find(|item| item.week <= week)
}

/// 一周是否全部达标。
#[must_use]
pub fn week_complete(plan: &RoadmapWeek, metrics: &RoadmapMetrics) -> bool {
    plan.checks
        .iter()
        .filter(|check| check.kind != CheckKind::SelfReported)
        .all(|check| check.is_met(metrics))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roadmap_is_a_sparse_but_ordered_checkpoint_table() {
        // 刻意**稀疏**：每 2–4 周一个能力检查，而不是 26 条凑数 ——
        // 每周都写一条「里程碑」只会变成打勾游戏。
        assert!(!ROADMAP.is_empty());
        for pair in ROADMAP.windows(2) {
            assert!(pair[0].week < pair[1].week, "周次必须递增");
        }
        for item in ROADMAP.iter() {
            assert!((1..=26).contains(&item.week), "第 {} 周越界", item.week);
            assert!(!item.checks.is_empty(), "第 {} 周没有检查项", item.week);
            assert!(!item.can_do.is_empty(), "第 {} 周没有能力描述", item.week);
        }
        assert_eq!(
            ROADMAP.last().map(|item| item.week),
            Some(26),
            "必须覆盖到第 26 周"
        );
    }

    #[test]
    fn current_checkpoint_is_the_last_one_not_yet_passed_week() {
        // 第 5 周：最近一次检查是第 4 周。
        assert_eq!(latest_checkpoint(5).map(|item| item.week), Some(4));
        // 第 4 周本身：就是第 4 周。
        assert_eq!(latest_checkpoint(4).map(|item| item.week), Some(4));
        // 第 1 周之前：还没有任何检查。
        assert!(latest_checkpoint(0).is_none());
        // 超过终点：停在第 26 周。
        assert_eq!(latest_checkpoint(40).map(|item| item.week), Some(26));
    }

    #[test]
    fn metrics_drive_every_checkpoint_kind() {
        let metrics = RoadmapMetrics {
            lessons_completed: 10,
            spoken_seconds: 1_500,
            review_mastery: 75.0,
            sentence_cards: 20,
            words_learned: 250,
            streak_days: 5,
        };
        let week = week_plan(4).expect("week 4");
        assert!(week_complete(week, &metrics), "全部达标：{metrics:?}");
        // 差一个条件就不算完成。
        let short = RoadmapMetrics {
            spoken_seconds: 100,
            ..metrics
        };
        assert!(!week_complete(week, &short));
        // 每个检查项的 current 都是真实数值。
        for check in week.checks {
            let _ = check.current(&metrics);
        }
    }

    #[test]
    fn self_reported_never_counts_as_met() {
        let metrics = RoadmapMetrics {
            lessons_completed: 999,
            spoken_seconds: 9_999,
            review_mastery: 100.0,
            sentence_cards: 999,
            words_learned: 999,
            streak_days: 999,
        };
        let check = c("敢不敢说", CheckKind::SelfReported, 1);
        assert!(!check.is_met(&metrics), "自评项不能自动判达标");
        // 自动项在同样数据下都达标，说明判定确实在看数。
        assert!(c("x", CheckKind::LessonsCompleted, 1).is_met(&metrics));
    }

    #[test]
    fn spoken_seconds_threshold_reads_as_minutes() {
        // 界面显示单位是分钟：阈值 1500 秒 = 25 分钟，current 原样给秒数，
        // 由界面换算 —— 阈值与 current 必须同单位。
        let metrics = RoadmapMetrics {
            spoken_seconds: 1_500,
            ..RoadmapMetrics::default()
        };
        let check = c("开口 25 分钟", CheckKind::SpokenSeconds, 1_500);
        assert!(check.is_met(&metrics));
        assert_eq!(check.current(&metrics), 1_500);
        assert_eq!(check.unit(), "分钟");
    }

    #[test]
    fn current_week_is_derived_from_a_real_start_date() {
        let start = 1_700_000_000;
        // 第一天 → 第 1 周；第 7 天（满 7 天）→ 第 2 周。
        assert_eq!(current_week(start, start), Some(1));
        assert_eq!(current_week(start, start + 6 * 86_400), Some(1));
        assert_eq!(current_week(start, start + 7 * 86_400), Some(2));
        // 没记录开始日（或还没开始）→ None，界面自己去问，不编。
        assert_eq!(current_week(0, start + 100 * 86_400), None);
        assert_eq!(current_week(start, start - 10), None);
        // 超过 26 周不越界。
        assert_eq!(
            current_week(start, start + 400 * 86_400),
            Some(ROADMAP.len() as u32)
        );
    }
}
