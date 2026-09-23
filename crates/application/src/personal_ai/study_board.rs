//! Study Board 模块适配器（V11 §107-§114）。
//!
//! `study-board` 是标准 Personal AI 模块：descriptor + 4 工具 + ContextProvider。
//! **PersonalAgent 不含任何 study-board 业务分支**：新增能力只改本文件。
//!
//! 四条铁律对应 V11 Plan：
//! 1. **后端不执行绘画**（§108）：工具只有 `list` / `get` / `save` / `snapshot`；
//!    绘图永远是前端 canvas 行为，工具只登记或回传引用。
//! 2. **后端不解释笔迹**（§109）：`strokes` 是不透明 JSON，原样存取；任何面向
//!    模型的摘要走 [`devtoolbox_core::study_board::strokes_summary`]（有界、无坐标）。
//! 3. **风险分级**（§110）：`list`/`get`/`snapshot` 为 `Read`；`save` 为
//!    `SafeWrite`（白名单由 `registry::allowed_risk` 强制，本模块不绕过）。
//! 4. **隐私铁律**（§113/§114）：板与快照是用户私有数据 —— **不得**自动提升为
//!    Personal Memory（工具路径永不调 memory 服务），**不得**进入日志 / 遥测；
//!    错误文本只含稳定 reason，不含笔迹正文。

use std::sync::Arc;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::study_board::{
    STUDY_BOARD_MAX_TITLE_CHARS, StudyBoard, StudyBoardSnapshot, StudyBoardSummary,
    is_valid_board_id, strokes_summary,
};
use devtoolbox_core::{AgentError, ModuleDescriptor, ToolResult, ToolRisk, ToolSpec};

use crate::personal_ai::args::{optional_string, require_string, tool_error, usize_arg};
use crate::personal_ai::context::{ContextBudget, ContextBundle, ModuleContextProvider};
use crate::personal_ai::registry::{
    ModuleRegistration, ModuleRegistry, ToolExecutor, ToolRegistry,
};
use crate::study_board::ports::{StudyBoardStoreError, StudyBoardStorePort};

/// 模块 id（descriptor id、ContextProvider id、工具名前缀必须一致）。
pub const STUDY_BOARD_MODULE_ID: &str = "study-board";

const TOOL_LIST: &str = "study-board.list";
const TOOL_GET: &str = "study-board.get";
const TOOL_SAVE: &str = "study-board.save";
const TOOL_SNAPSHOT: &str = "study-board.snapshot";

const TOOL_NAMES: [&str; 4] = [TOOL_LIST, TOOL_GET, TOOL_SAVE, TOOL_SNAPSHOT];

/// 四个工具的契约（声明期已知 → `LazyLock`；spec 与 dispatch 同序）。
static TOOL_SPECS: LazyLock<[ToolSpec; 4]> = LazyLock::new(|| {
    [
        spec_for(TOOL_LIST),
        spec_for(TOOL_GET),
        spec_for(TOOL_SAVE),
        spec_for(TOOL_SNAPSHOT),
    ]
});

/// 工具名清单（组合根与测试引用）。
#[must_use]
pub fn study_board_tool_names() -> [&'static str; 4] {
    TOOL_NAMES
}

/// Study Board 工具集（一个结构体、四个身份；dispatch 属模块内部实现细节）。
pub struct StudyBoardTools {
    store: Arc<dyn StudyBoardStorePort>,
}

impl StudyBoardTools {
    #[must_use]
    pub fn new(store: Arc<dyn StudyBoardStorePort>) -> Self {
        Self { store }
    }

    /// `study-board.list`：学习板元数据列表（不含笔迹正文）。
    fn list(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let limit = usize_arg(arguments, "limit").unwrap_or(20).min(200);
        let items = self
            .store
            .list_boards(limit)
            .map_err(|error| tool_error(store_failure("board_list_failed", &error)))?;
        let list: Vec<serde_json::Value> = items.iter().map(summary_json).collect();
        let count = list.len();
        Ok(ToolResult::ok_with_metadata(
            serde_json::json!({"items": list, "count": count}),
            serde_json::json!({
                "ui_hint": {"ui_blocks": [entity_list_block("学习板", &list)]}
            }),
        ))
    }

    /// `study-board.get`：单块板的元数据 + 有界笔迹摘要（不回传坐标）。
    fn get(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let board_id = board_id_argument(arguments)?;
        let board = self
            .store
            .get_board(&board_id)
            .map_err(|error| tool_error(store_failure("board_read_failed", &error)))?;
        let Some(board) = board else {
            return Ok(ToolResult::fail(format!(
                "学习板「{board_id}」不存在；可先用 study-board.list 查看现有学习板"
            )));
        };
        // 最近快照引用（只读登记，不生成图像）。
        let snapshot_id = self
            .store
            .latest_snapshot(&board.id)
            .map_err(|error| tool_error(store_failure("snapshot_read_failed", &error)))?
            .map(|snapshot| snapshot.id);
        Ok(ToolResult::ok_with_metadata(
            serde_json::json!({
                "board_id": board.id,
                "title": board.title,
                "module_origin": board.module_origin,
                "created_at": board.created_at,
                "updated_at": board.updated_at,
                "stroke_count": board.stroke_count(),
                "strokes_summary": strokes_summary(&board.strokes),
                "latest_snapshot_id": snapshot_id,
            }),
            serde_json::json!({
                "ui_hint": {"ui_blocks": [key_value_block(&format!("学习板 · {}", board.title), &[
                    ("标题", board.title.clone()),
                    ("笔画", board.stroke_count().to_string()),
                    ("来源", if board.module_origin.is_empty() { "—".to_string() } else { board.module_origin.clone() }),
                ])]}
            }),
        ))
    }

    /// `study-board.save`：创建/更新学习板（SafeWrite；幂等 upsert）。
    ///
    /// 不校验 / 不解释 `strokes` 内容（§109：笔迹由前端 canvas 产出后原样保存）。
    fn save(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let board_id = board_id_argument(arguments)?;
        let title = optional_string(arguments, "title");
        let strokes = match arguments.get("strokes") {
            Some(value) if value.is_null() => None,
            Some(value) => Some(value.clone()),
            None => None,
        };
        let module_origin = optional_string(arguments, "module_origin");
        let now = now_unix();

        if title.is_none() && strokes.is_none() {
            return Err(AgentError::tool_invalid_argument(
                "study-board.save: 至少提供 title 或 strokes 之一",
            ));
        }
        if let Some(title) = &title
            && title.chars().count() > STUDY_BOARD_MAX_TITLE_CHARS
        {
            return Err(AgentError::tool_invalid_argument(format!(
                "study-board.save: title 超过 {STUDY_BOARD_MAX_TITLE_CHARS} 字符"
            )));
        }

        let existing = self
            .store
            .get_board(&board_id)
            .map_err(|error| tool_error(store_failure("board_read_failed", &error)))?;
        let (board, created) = match existing {
            Some(mut board) => {
                board.update(title, strokes, now);
                if let Some(origin) = module_origin {
                    board.module_origin = origin;
                }
                (board, false)
            }
            None => {
                // 新板必须有标题（不能创建无标题板）。
                let Some(title) = title else {
                    return Err(AgentError::tool_invalid_argument(format!(
                        "study-board.save: 新学习板「{board_id}」必须提供 title"
                    )));
                };
                let strokes = strokes.unwrap_or_else(|| serde_json::json!({"strokes": []}));
                (
                    StudyBoard::new(
                        board_id,
                        title,
                        strokes,
                        now,
                        module_origin.unwrap_or_default(),
                    ),
                    true,
                )
            }
        };

        self.store
            .upsert_board(&board)
            .map_err(|error| tool_error(store_failure("board_save_failed", &error)))?;
        Ok(ToolResult::ok_with_metadata(
            serde_json::json!({
                "board_id": board.id,
                "title": board.title,
                "created": created,
                "updated_at": board.updated_at,
                "stroke_count": board.stroke_count(),
                "note": "已保存学习板（笔迹内容不进入个人记忆与日志）",
            }),
            serde_json::json!({
                "ui_hint": {"ui_blocks": [key_value_block(
                    &format!("已保存学习板 · {}", board.title),
                    &[
                        ("标题", board.title.clone()),
                        ("笔画", board.stroke_count().to_string()),
                    ],
                )]}
            }),
        ))
    }

    /// `study-board.snapshot`：登记/返回快照引用（Read；后端不生成图像）。
    ///
    /// 前端渲染 PNG 后以 `png_base64` 传入登记；未传则只登记有界摘要。
    /// 返回的 `data.kind == "board_snapshot"` 就是快照内容部件引用。
    fn snapshot(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let board_id = board_id_argument(arguments)?;
        let board = self
            .store
            .get_board(&board_id)
            .map_err(|error| tool_error(store_failure("board_read_failed", &error)))?;
        let Some(board) = board else {
            return Ok(ToolResult::fail(format!(
                "学习板「{board_id}」不存在，无法登记快照"
            )));
        };
        let png_base64 = optional_string(arguments, "png_base64");
        let summary = strokes_summary(&board.strokes);
        let snapshot = StudyBoardSnapshot::new(
            new_id("snap"),
            &board.id,
            &board.title,
            png_base64,
            summary.clone(),
            now_unix(),
        );
        self.store
            .upsert_snapshot(&snapshot)
            .map_err(|error| tool_error(store_failure("snapshot_save_failed", &error)))?;
        Ok(ToolResult::ok_with_metadata(
            serde_json::json!({
                "kind": "board_snapshot",
                "snapshot_id": snapshot.id,
                "board_id": board.id,
                "title": board.title,
                "strokes_summary": summary,
                "created_at": snapshot.created_at,
                "has_png": snapshot.png_base64.is_some(),
            }),
            serde_json::json!({
                "ui_hint": {"ui_blocks": [key_value_block(
                    &format!("学习板快照 · {}", board.title),
                    &[("板", board.title.clone()), ("摘要", summary)],
                )]}
            }),
        ))
    }
}

// ---------------------------------------------------------------------------
// 工具契约 / 薄执行器
// ---------------------------------------------------------------------------

fn spec_for(name: &str) -> ToolSpec {
    let (description, input_schema, risk) = match name {
        TOOL_LIST => (
            "列出用户的学习板（标题 / 来源 / 更新时间；不含笔迹内容）。",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200}
                }
            }),
            ToolRisk::Read,
        ),
        TOOL_GET => (
            "读取一块学习板的元数据与有界笔迹摘要（不含坐标；需要看图请用快照）。",
            serde_json::json!({
                "type": "object",
                "required": ["board_id"],
                "properties": {"board_id": {"type": "string", "description": "学习板 id（稳定标识）"}}
            }),
            ToolRisk::Read,
        ),
        TOOL_SAVE => (
            "创建或更新一块学习板（幂等 upsert）。后端不执行绘画、不解释笔迹内容；\
             笔迹由前端 canvas 产出后原样保存。",
            serde_json::json!({
                "type": "object",
                "required": ["board_id"],
                "properties": {
                    "board_id": {"type": "string", "description": "学习板 id（新建或覆盖目标）"},
                    "title": {"type": "string"},
                    "strokes": {"description": "矢量笔画数据（后端不透明）"},
                    "module_origin": {"type": "string", "description": "来源模块（如 history / language）"}
                }
            }),
            ToolRisk::SafeWrite,
        ),
        _ => (
            "登记一块学习板的快照引用（前端渲染 PNG 后传入 base64；不传则只登记摘要）。\
             返回 board_snapshot 引用，不生成图像。",
            serde_json::json!({
                "type": "object",
                "required": ["board_id"],
                "properties": {
                    "board_id": {"type": "string", "description": "学习板 id"},
                    "png_base64": {"type": "string", "description": "前端渲染的 PNG（base64；可选）"}
                }
            }),
            ToolRisk::Read,
        ),
    };
    ToolSpec {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
        risk,
        module: STUDY_BOARD_MODULE_ID.to_string(),
    }
}

struct ToolImpl {
    name: &'static str,
    tools: Arc<StudyBoardTools>,
}

#[async_trait::async_trait]
impl ToolExecutor for ToolImpl {
    fn spec(&self) -> &ToolSpec {
        let index = TOOL_NAMES
            .iter()
            .position(|name| *name == self.name)
            .expect("tool name registered");
        &TOOL_SPECS[index]
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError> {
        match self.name {
            TOOL_LIST => self.tools.list(&arguments),
            TOOL_GET => self.tools.get(&arguments),
            TOOL_SAVE => self.tools.save(&arguments),
            _ => self.tools.snapshot(&arguments),
        }
    }
}

// ---------------------------------------------------------------------------
// ContextProvider
// ---------------------------------------------------------------------------

/// Study Board 模块上下文提供方：AppContext{module,page,entity} → 当前板标题/id。
pub struct StudyBoardProviderOwned {
    tools: Arc<StudyBoardTools>,
}

impl ModuleContextProvider for StudyBoardProviderOwned {
    fn module_id(&self) -> &str {
        STUDY_BOARD_MODULE_ID
    }

    fn build_context(
        &self,
        app_context: &AppContext,
        _budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        // 只从「我在哪」取当前板：`entity{kind:"board", id}`（前端提供）。
        let current = app_context
            .entity
            .as_ref()
            .filter(|entity| entity.kind == "board")
            .map(|entity| entity.id.trim().to_string())
            .filter(|id| !id.is_empty());
        let Some(board_id) = current else {
            return Ok(ContextBundle {
                module: STUDY_BOARD_MODULE_ID.to_string(),
                headline: "学习板".to_string(),
                summary: serde_json::json!({
                    "module": STUDY_BOARD_MODULE_ID,
                    "note": "当前未打开具体学习板；可用 study-board.list 查看已有学习板",
                }),
            });
        };
        let page = app_context.page.clone().unwrap_or_default();
        let loaded = self.tools.store.get_board(&board_id).map_err(|error| {
            AgentError::context(store_failure("board_read_failed", &error).to_string())
        })?;
        match loaded {
            Some(board) => Ok(ContextBundle {
                module: STUDY_BOARD_MODULE_ID.to_string(),
                headline: format!("学习板 · {}", board.title),
                summary: serde_json::json!({
                    "module": STUDY_BOARD_MODULE_ID,
                    "page": page,
                    "entity": {"kind": "board", "id": board.id, "label": board.title},
                    "title": board.title,
                    "stroke_count": board.stroke_count(),
                    "strokes_summary": strokes_summary(&board.strokes),
                    "note": "当前学习板的元信息；笔画坐标由前端渲染，不进入上下文",
                }),
            }),
            // 上下文里的板不存在不是致命错误：给「未知板」摘要，让模型去 list。
            None => Ok(ContextBundle {
                module: STUDY_BOARD_MODULE_ID.to_string(),
                headline: format!("学习板 · {board_id}（未找到）"),
                summary: serde_json::json!({
                    "module": STUDY_BOARD_MODULE_ID,
                    "page": page,
                    "entity": {"kind": "board", "id": board_id},
                    "note": "当前学习板在存储中不存在；可提示用户用 study-board.list 或新建",
                }),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// 注册
// ---------------------------------------------------------------------------

/// 注册 Study Board 模块（descriptor + 4 工具 + context provider）。
///
/// `list`/`get`/`snapshot` 为 Read，`save` 为 SafeWrite（§110）：SafeWrite 白名单
/// 由 [`crate::personal_ai::registry::allowed_risk`] 强制。
pub fn register_study_board(
    modules: &mut ModuleRegistry,
    tools: &mut ToolRegistry,
    store: Arc<dyn StudyBoardStorePort>,
) -> Result<(), AgentError> {
    modules.register(ModuleRegistration {
        descriptor: ModuleDescriptor {
            id: STUDY_BOARD_MODULE_ID.into(),
            display_name: "学习板".into(),
            description: "学习板：手写/草图板的列表、读取、保存与快照登记；后端不执行绘画、不解释笔迹"
                .into(),
            capabilities: vec!["board".into(), "snapshot".into()],
            tools: TOOL_NAMES.iter().map(|name| (*name).to_string()).collect(),
        },
        context_provider: Some(Arc::new(StudyBoardProviderOwned {
            tools: Arc::new(StudyBoardTools::new(Arc::clone(&store))),
        })),
    })?;
    let shared = Arc::new(StudyBoardTools::new(store));
    for name in TOOL_NAMES {
        tools.register(Arc::new(ToolImpl {
            name,
            tools: Arc::clone(&shared),
        }))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// 板 id 参数：缺失或注入形态 → 参数错误（模型只能传稳定 id，V11 §110）。
fn board_id_argument(arguments: &serde_json::Value) -> Result<String, AgentError> {
    let board_id = require_string(arguments, "board_id")?;
    if !is_valid_board_id(&board_id) {
        return Err(AgentError::tool_invalid_argument(format!(
            "invalid board_id `{board_id}`（只允许小写英数字与 . _ -，1-64 字符）"
        )));
    }
    Ok(board_id)
}

/// 存储失败 → 应用层错误。`reason` 稳定；文本不含笔迹正文。
///
/// 复用 `ApplicationError::Infrastructure`（最接近的既有变体；不新增变体）。
fn store_failure(reason: &str, error: &StudyBoardStoreError) -> crate::error::ApplicationError {
    crate::error::ApplicationError::Infrastructure {
        path: std::path::PathBuf::from("study_board"),
        message: format!("{reason}: {}", error.0),
    }
}

/// 板元数据的 JSON 形状（列表用；不含笔迹正文）。
fn summary_json(item: &StudyBoardSummary) -> serde_json::Value {
    serde_json::json!({
        "id": item.id,
        "title": item.title,
        "module_origin": item.module_origin,
        "created_at": item.created_at,
        "updated_at": item.updated_at,
        "stroke_count": item.stroke_count,
    })
}

fn key_value_block(title: &str, items: &[(&str, String)]) -> serde_json::Value {
    serde_json::json!({
        "kind": "key_value",
        "title": title,
        "data": {"items": items.iter().map(|(label, value)| serde_json::json!({
            "label": label,
            "value": value,
        })).collect::<Vec<_>>()},
    })
}

fn entity_list_block(title: &str, items: &[serde_json::Value]) -> serde_json::Value {
    serde_json::json!({
        "kind": "entity_list",
        "title": title,
        "data": {"items": items},
    })
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

/// 时间后缀 id（无 crypto 依赖；与 agent 会话 id 同手法）。
fn new_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{:016x}", nanos % u128::from(u64::MAX))
}

#[cfg(test)]
mod study_board_tests;
