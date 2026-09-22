//! Study Board 数据契约（V11 §107-§114）—— 纯 serde，无 IO、无业务分支。
//!
//! 铁律（与 `memory` 同级别的契约冻结）：
//! - `strokes` 对后端**不透明**：它是矢量笔画数组（前端 canvas 数据），
//!   backend 永不解析、永不解释（只整体存/取）。
//! - 面向模型/日志的任何摘要必须走 [`strokes_summary`]（有界、不含原始坐标），
//!   这是防止笔迹正文被日志或个人记忆拾起的唯一出口。
//! - 全部可选字段 `#[serde(default)]`：前端旧版本缺字段时优雅降级。

use serde::{Deserialize, Serialize};

/// 模块 id（V11 §108：descriptor id 与工具名前缀必须一致）。
pub const STUDY_BOARD_MODULE_ID: &str = "study-board";

/// 标题最大字符数（超过则视为非法输入，由调用方拒绝）。
pub const STUDY_BOARD_MAX_TITLE_CHARS: usize = 200;

/// 笔画摘要最大字符数（V11 §111：有界，防笔迹正文外泄）。
pub const STROKE_SUMMARY_MAX_CHARS: usize = 2000;

/// 学习板。
///
/// - `strokes`：矢量笔画数据，**后端不透明**（`serde_json::Value`）；形状由前端
///   定义（如 `{"strokes":[{"points":[…],"color":…}]}`），backend 只原样存取。
/// - `module_origin`：来源模块（如 `history` / `language` / `study-board`），
///   标识这块板由哪个业务场景创建（用于列表分组，不参与鉴权）。
/// - 时间戳为 Unix 秒。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StudyBoard {
    pub id: String,
    pub title: String,
    /// 矢量笔画数据（不透明；可能为 `Null` / 空数组，未初始化时合法）。
    pub strokes: serde_json::Value,
    pub created_at: i64,
    pub updated_at: i64,
    /// 来源模块 id（历史/语言等业务模块可在保存时标注）。
    pub module_origin: String,
}

impl StudyBoard {
    /// 新板（`created_at == updated_at`；调用方负责生成 id）。
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        strokes: serde_json::Value,
        now: i64,
        module_origin: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            strokes,
            created_at: now,
            updated_at: now,
            module_origin: module_origin.into(),
        }
    }

    /// 覆盖内容并刷新 `updated_at`（不触碰 `created_at`）。
    pub fn update(&mut self, title: Option<String>, strokes: Option<serde_json::Value>, now: i64) {
        if let Some(title) = title {
            self.title = title;
        }
        if let Some(strokes) = strokes {
            self.strokes = strokes;
        }
        self.updated_at = now;
    }

    /// 笔画条数（`strokes` 形状不透明：`{"strokes":[…]}` 或裸数组都支持）。
    #[must_use]
    pub fn stroke_count(&self) -> usize {
        stroke_count(&self.strokes)
    }

    /// 覆盖 `updated_at`（列表排序测试用；生产路径请用 `update`）。
    #[must_use]
    pub fn with_updated_at(mut self, updated_at: i64) -> Self {
        self.updated_at = updated_at;
        self
    }

    /// 列表/上下文用的轻量元数据（**不含**笔迹正文）。
    #[must_use]
    pub fn summary(&self) -> StudyBoardSummary {
        StudyBoardSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            module_origin: self.module_origin.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            stroke_count: self.stroke_count(),
        }
    }
}

/// 学习板快照（V11 §111）。
///
/// `png_base64` 由前端画布渲染后登记（backend 不生成图像）；`strokes_summary`
/// 必须来自 [`strokes_summary`]（有界）。快照与笔迹同样是私有数据：
/// **不自动提升为 Personal Memory、不进日志**（V11 §113/§114）。
///
/// `title` 是登记瞬间的板标题，只随快照记录在内存/接口形状里；SQLite
/// `snapshots` 表不存 title（schema 由 V11 §112 冻结），读取时由调用方补当前板标题。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StudyBoardSnapshot {
    /// 快照 id（`snap-*`）。
    pub id: String,
    pub board_id: String,
    /// 登记时的板标题（冗余便于展示，不要求与当前标题一致）。
    pub title: String,
    /// PNG base64（前端传入；可能为空 = 只存摘要）。
    pub png_base64: Option<String>,
    /// 有界笔画摘要（≤ [`STROKE_SUMMARY_MAX_CHARS`]）。
    pub strokes_summary: String,
    pub created_at: i64,
}

impl StudyBoardSnapshot {
    /// 构造快照（摘要一律经过有界裁剪，防止超大 payload）。
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        board_id: impl Into<String>,
        title: impl Into<String>,
        png_base64: Option<String>,
        strokes_summary: impl Into<String>,
        now: i64,
    ) -> Self {
        Self {
            id: id.into(),
            board_id: board_id.into(),
            title: title.into(),
            png_base64,
            strokes_summary: bounded_strokes_summary(&strokes_summary.into()),
            created_at: now,
        }
    }
}

/// 学习板轻量元数据（列表 / ContextProvider 使用；不含笔迹正文）。
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StudyBoardSummary {
    pub id: String,
    pub title: String,
    pub module_origin: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub stroke_count: usize,
}

/// 板 id 校验（V11 §110：模型只能传稳定 id；封死注入形态）。
///
/// 规则与 `core::server::is_valid_id` 一致：1–64 个字符，首字符小写字母或数字，
/// 其余只允许小写英数字与 `.` / `_` / `-`；前后空白视为非法。
#[must_use]
pub fn is_valid_board_id(raw: &str) -> bool {
    if raw.is_empty() || raw.len() > 64 {
        return false;
    }
    let mut chars = raw.chars();
    let first = chars.next().unwrap_or('_');
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    raw.chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
}

/// 笔画条数（`strokes` 形状不透明：`{"strokes":[…]}` 或裸数组都支持；未知形状为 0）。
#[must_use]
pub fn stroke_count(strokes: &serde_json::Value) -> usize {
    let array = strokes
        .get("strokes")
        .and_then(serde_json::Value::as_array)
        .or_else(|| strokes.as_array());
    array.map_or(0, |items| {
        items
            .iter()
            .filter(|item| {
                !item.is_null() && item.as_array().is_none_or(|points| !points.is_empty())
            })
            .count()
    })
}

/// 笔画 → 有界摘要（V11 §111）：不返回坐标正文，只给模型可读的结构信息。
///
/// 永不 panic；输入任意 JSON 都返回 ≤ [`STROKE_SUMMARY_MAX_CHARS`] 字符的文本。
#[must_use]
pub fn strokes_summary(strokes: &serde_json::Value) -> String {
    let count = stroke_count(strokes);
    let text = match strokes {
        serde_json::Value::Null => "空学习板（无笔画）".to_string(),
        serde_json::Value::Array(_) if count == 0 => "空学习板（无笔画）".to_string(),
        _ => format!("{count} 条矢量笔画（坐标由前端渲染，不在此展示）"),
    };
    bounded_strokes_summary(&text)
}

/// 硬截断到 [`STROKE_SUMMARY_MAX_CHARS`]（UTF-8 边界安全；保留 `…` 后缀）。
#[must_use]
pub fn bounded_strokes_summary(text: &str) -> String {
    if text.chars().count() <= STROKE_SUMMARY_MAX_CHARS {
        return text.to_string();
    }
    let mut out: String = text
        .chars()
        .take(STROKE_SUMMARY_MAX_CHARS.saturating_sub(1))
        .collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn study_board_round_trip_and_defaults() {
        let board = StudyBoard::new(
            "b-1868",
            "遵义会议草图",
            json!({"strokes": [{"points": [[0, 0], [3, 4]], "color": "#000"}]}),
            1_000,
            "history",
        );
        let text = serde_json::to_string(&board).expect("serialize");
        let back: StudyBoard = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(board, back);

        // serde(default)：缺字段的旧 payload 也能解析（模块来源缺省为空串）。
        let legacy: StudyBoard =
            serde_json::from_value(json!({"id": "b-1", "title": "t", "strokes": null}))
                .expect("legacy payload");
        assert_eq!(legacy.module_origin, "");
        assert_eq!(legacy.created_at, 0);
        assert_eq!(legacy.updated_at, 0);
        assert!(legacy.strokes.is_null());
    }

    #[test]
    fn empty_object_deserializes_to_defaults() {
        let board: StudyBoard = serde_json::from_str("{}").expect("empty payload");
        assert_eq!(board, StudyBoard::default());
        assert_eq!(board.stroke_count(), 0);
        assert!(board.id.is_empty());
    }

    #[test]
    fn update_overwrites_content_and_touches_updated_at_only() {
        let mut board = StudyBoard::new("b-1", "旧标题", json!({"strokes": []}), 1_000, "history");
        board.update(
            Some("新标题".into()),
            Some(json!({"strokes": [{"points": [[1, 1]]}]})),
            2_000,
        );
        assert_eq!(board.title, "新标题");
        assert_eq!(board.created_at, 1_000);
        assert_eq!(board.updated_at, 2_000);
        assert_eq!(board.stroke_count(), 1);
        // 只改标题不动笔迹。
        board.update(Some("又改标题".into()), None, 3_000);
        assert_eq!(board.stroke_count(), 1);
        assert_eq!(board.updated_at, 3_000);
    }

    #[test]
    fn strokes_summary_is_bounded_and_leaks_no_geometry() {
        let strokes = json!({"strokes": [{"points": [[0, 0], [9, 9]]}, {"points": [[1, 1]]}]});
        let summary = strokes_summary(&strokes);
        assert_eq!(summary, "2 条矢量笔画（坐标由前端渲染，不在此展示）");
        assert!(!summary.contains("0, 0"), "摘要不得含坐标点");
        assert!(summary.chars().count() <= STROKE_SUMMARY_MAX_CHARS);
        assert_eq!(strokes_summary(&serde_json::Value::Null), "空学习板（无笔画）");
        assert_eq!(strokes_summary(&json!([])), "空学习板（无笔画）");
        // 裸数组笔画也被计数（前端旧形状）。
        assert_eq!(stroke_count(&json!([[[0, 0], [1, 1]]])), 1);

        // 任意超长输入都被裁到上限内。
        let huge = bounded_strokes_summary(&"笔".repeat(20_000));
        assert_eq!(huge.chars().count(), STROKE_SUMMARY_MAX_CHARS);
        assert!(huge.ends_with('…'));
        // 空输入不越界。
        assert_eq!(bounded_strokes_summary(""), "");
    }

    #[test]
    fn snapshot_summary_is_bounded_on_construction() {
        let snapshot = StudyBoardSnapshot::new(
            "snap-1",
            "b-1",
            "标题",
            None,
            "板".repeat(20_000),
            3_000,
        );
        assert_eq!(
            snapshot.strokes_summary.chars().count(),
            STROKE_SUMMARY_MAX_CHARS
        );
        assert!(snapshot.png_base64.is_none());
        let text = serde_json::to_string(&snapshot).expect("serialize");
        assert!(text.contains("\"png_base64\":null"));

        let loaded: StudyBoardSnapshot = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(loaded, snapshot);
    }

    #[test]
    fn snapshot_defaults_when_png_missing() {
        let snapshot: StudyBoardSnapshot = serde_json::from_value(json!({
            "id": "snap-2",
            "board_id": "b-2",
            "title": "t",
            "strokes_summary": "1 条矢量笔画（坐标由前端渲染，不在此展示）",
            "created_at": 1
        }))
        .expect("snapshot without png");
        assert!(snapshot.png_base64.is_none());
        let empty: StudyBoardSnapshot = serde_json::from_str("{}").expect("empty snapshot");
        assert_eq!(empty, StudyBoardSnapshot::default());
    }

    #[test]
    fn board_id_validation_rejects_injection_shapes() {
        assert!(is_valid_board_id("b-1868"));
        assert!(is_valid_board_id("board.history_1"));
        for bad in [
            "",
            " ",
            "b 1",
            "B-UPPER",
            "-leading-dash",
            "板-1",
            "b\n2",
            " board",
            "board ",
            "b/2",
            "b;rm",
            &"a".repeat(65),
        ] {
            assert!(!is_valid_board_id(bad), "应当拒绝: {bad:?}");
        }
    }

    #[test]
    fn summary_metadata_contains_no_strokes() {
        let board = StudyBoard::new(
            "b-3",
            "手写",
            json!({"strokes": [{"points": [[7, 7]]}]}),
            1,
            "language",
        );
        let summary = board.summary();
        assert_eq!(summary.stroke_count, 1);
        let text = serde_json::to_string(&summary).expect("serialize");
        assert!(!text.contains("points"), "摘要不得含水笔坐标");
        assert!(!text.contains("7, 7"));
        assert_eq!(StudyBoardSummary::default().stroke_count, 0);
    }

    #[test]
    fn module_id_is_stable() {
        assert_eq!(STUDY_BOARD_MODULE_ID, "study-board");
    }
}
