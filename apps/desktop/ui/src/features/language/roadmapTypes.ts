/**
 * 26 周能力路线图的前后端契约（V13 W6）。
 *
 * 与 `crates/application/src/language/course.rs` 的 `RoadmapView` 对应。
 * 数字**全部来自真实统计**：课程进度、跟读记录、平台复习、句子卡数量。
 */

/** 一条检查项。`auto = false` 表示这是学习者自评，系统不自动判定。 */
export interface RoadmapCheck {
  label: string;
  /** 稳定分类：lessons_completed / spoken_seconds / review_mastery / sentence_cards / words_learned / streak_days / self_reported。 */
  kind: string;
  current: number;
  threshold: number;
  unit: string;
  auto: boolean;
  met: boolean;
}

/** 一次能力检查（第 N 周）。 */
export interface RoadmapCheckpoint {
  week: number;
  focus: string;
  /** 到这一周应能做到的事（能力，不是课时）。 */
  can_do: string;
  checks: RoadmapCheck[];
  /** 全部**自动**项达标才算完成（自评项不参与）。 */
  complete: boolean;
}

/** 真实指标。 */
export interface RoadmapMetrics {
  lessons_completed: number;
  spoken_minutes: number;
  review_mastery: number;
  sentence_cards: number;
  words_learned: number;
  streak_days: number;
}

export interface RoadmapView {
  /** 开始日（Unix 秒）；没有学习记录时为 null —— 此时不显示「第几周」。 */
  started_at: number | null;
  current_week: number | null;
  /** 距下一个检查点还有几天。 */
  days_to_next: number | null;
  current_checkpoint: RoadmapCheckpoint | null;
  checkpoints: RoadmapCheckpoint[];
  metrics: RoadmapMetrics;
}
