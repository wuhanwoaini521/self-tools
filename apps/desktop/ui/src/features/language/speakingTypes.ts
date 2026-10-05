/**
 * 跟读评分的前后端契约类型（V13 W2）。
 *
 * 与 `crates/core/src/language/course.rs` 的 `ShadowAttempt` / `ShadowStats`
 * 与 `language_write_api::shadow_score` 的响应一一对应（Rust 侧是 snake_case）。
 */

/** 评分请求：只带「哪一课哪一句 + 识别到的转写」。 */
export interface ShadowScoreInput {
  lessonId: string;
  sentenceSeq: number;
  /** 语音识别转写（必须来自真实识别结果；空串服务端会拒绝）。 */
  transcript: string;
  /** 实际开口时长（毫秒）。 */
  durationMs: number;
  /** 目标句参考时长（毫秒；缺省 0 → 服务端只算准确度/完整度相关的部分）。 */
  targetMs?: number;
  /** 超过阈值的长停顿（毫秒数组）。 */
  longPausesMs?: number[];
}

/** 评分响应（含词级差异，界面直接展示「漏了哪个词」）。 */
export interface ShadowScoreResult {
  /** 三项均分（0–100）。 */
  overall: number;
  accuracy: number;
  completeness: number;
  fluency: number;
  duration_ms: number;
  /** 服务端从库里取到的目标句（不采信前端自报）。 */
  target: string;
  transcript: string;
  missing: string[];
  wrong: string[];
  extra: string[];
}

/** 跟读统计（进度页与路线图的真实指标）。 */
export interface ShadowStats {
  attempts: number;
  /** 开口总时长（秒）。 */
  spoken_seconds: number;
  avg_accuracy: number;
  avg_completeness: number;
  avg_fluency: number;
  /** 达到 80 分以上的次数。 */
  strong_attempts: number;
}
