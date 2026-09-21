//! Files 模块适配器（V6 Track C，§40-§48）。
//!
//! Files 是标准 Personal AI 模块：注册 descriptor + 4 个 Read 工具 + ContextProvider。
//! **任何文件访问都必须经 `FileService::authorize`**（允许根 + canonicalize +
//! traversal + deny）；模块层不做任何路径判断，也不存在写 / 移动 / 重命名 /
//! 删除 / 执行能力（§42：backend 绝不运行 shell，`files.open` 只产出 Action）。
//!
//! - `files.search(query?, extension?, root_id?, modified_after?, limit?)` → 允许根内文件命中
//! - `files.get_metadata(target)` → 单文件元数据（target = `file_id` 或路径）
//! - `files.read_text(target, max_chars?)` → 安全读取；二进制 / 超大 / 受限 → **只返回元数据**
//! - `files.open(target)` → 打开请求（`OpenFile` Action，由前端执行）
//!
//! 设置为组合根注入的闭包，**每次工具调用 / 上下文构建时读取**，改设置立即生效。

use std::sync::Arc;

use devtoolbox_core::files::{FileAccessDenied, FileMetadata, KnowledgeRoot};
use devtoolbox_core::personal_ai::AppContext;
use devtoolbox_core::settings::KnowledgeSettings;
use devtoolbox_core::{AgentError, ModuleDescriptor, ToolResult, ToolRisk, ToolSpec};

use crate::error::ApplicationError;
use crate::files::{FileQuery, FileService};
use crate::personal_ai::args::{optional_string, require_string, tool_error, usize_arg};
use crate::personal_ai::context::{ContextBudget, ContextBundle, ModuleContextProvider};
use crate::personal_ai::registry::{ModuleRegistration, ModuleRegistry, ToolExecutor, ToolRegistry};

const TOOL_SEARCH: &str = "files.search";
const TOOL_GET_METADATA: &str = "files.get_metadata";
const TOOL_READ_TEXT: &str = "files.read_text";
const TOOL_OPEN: &str = "files.open";

const TOOL_NAMES: [&str; 4] = [TOOL_SEARCH, TOOL_GET_METADATA, TOOL_READ_TEXT, TOOL_OPEN];

/// 工具名清单（组合根与测试引用）。
#[must_use]
pub fn files_tool_names() -> [&'static str; 4] {
    TOOL_NAMES
}

/// Files 工具集（一个结构体、四个身份；dispatch 属模块内部实现细节）。
pub struct FilesTools {
    service: Arc<FileService>,
    settings: Arc<dyn Fn() -> KnowledgeSettings + Send + Sync>,
}

impl FilesTools {
    #[must_use]
    pub fn new(
        service: Arc<FileService>,
        settings: Arc<dyn Fn() -> KnowledgeSettings + Send + Sync>,
    ) -> Self {
        Self { service, settings }
    }

    #[must_use]
    pub fn service(&self) -> &Arc<FileService> {
        &self.service
    }

    /// 当前设置（每次调用读取，组合根改动立即生效）。
    fn settings(&self) -> KnowledgeSettings {
        (self.settings)()
    }
}

fn spec_for(name: &str) -> ToolSpec {
    let (description, input_schema) = match name {
        TOOL_SEARCH => (
            "在允许目录内搜索文件（按文件名/路径关键词、扩展名、修改时间过滤）。\
             只返回允许根内的文件；凭据/密钥类文件默认不返回。",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "文件名/路径关键词（可空 = 只按条件过滤）"},
                    "extension": {"type": "string", "description": "扩展名（不含点，如 md / json）"},
                    "root_id": {"type": "string", "description": "限定允许根 id"},
                    "modified_after": {"type": "integer", "description": "Unix 时间戳（秒），只返回此时间之后修改的文件"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50}
                }
            }),
        ),
        TOOL_GET_METADATA => (
            "取单个文件的元数据（大小 / 修改时间 / 内容类别 / 是否受限）。\
             target 为 file_id（file-…）或允许目录内的路径。",
            serde_json::json!({
                "type": "object",
                "required": ["target"],
                "properties": {"target": {"type": "string", "description": "file_id 或允许根内的文件路径"}}
            }),
        ),
        TOOL_READ_TEXT => (
            "安全读取文本文件内容（只读、受允许根与体积限制）。\
             受限文件、二进制文件、超大文件都不会返回内容，只返回元数据与原因。",
            serde_json::json!({
                "type": "object",
                "required": ["target"],
                "properties": {
                    "target": {"type": "string", "description": "file_id 或允许根内的文件路径"},
                    "max_chars": {"type": "integer", "minimum": 1, "maximum": 200000}
                }
            }),
        ),
        _ => (
            "产出打开文件的请求（前端决定如何打开）。本工具只返回元数据与打开动作，\
             不会执行任何命令。",
            serde_json::json!({
                "type": "object",
                "required": ["target"],
                "properties": {"target": {"type": "string", "description": "file_id 或允许根内的文件路径"}}
            }),
        ),
    };
    ToolSpec {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
        risk: ToolRisk::Read,
        module: "files".to_string(),
    }
}

/// 单个工具的薄执行器（满足 `ToolRegistry` 契约）。
struct ToolImpl {
    name: &'static str,
    tools: Arc<FilesTools>,
}

#[async_trait::async_trait]
impl ToolExecutor for ToolImpl {
    fn spec(&self) -> &ToolSpec {
        static CACHE: std::sync::LazyLock<[ToolSpec; 4]> = std::sync::LazyLock::new(|| {
            [
                spec_for(TOOL_SEARCH),
                spec_for(TOOL_GET_METADATA),
                spec_for(TOOL_READ_TEXT),
                spec_for(TOOL_OPEN),
            ]
        });
        match self.name {
            TOOL_SEARCH => &CACHE[0],
            TOOL_GET_METADATA => &CACHE[1],
            TOOL_READ_TEXT => &CACHE[2],
            _ => &CACHE[3],
        }
    }

    async fn execute(&self, arguments: serde_json::Value) -> Result<ToolResult, AgentError> {
        match self.name {
            TOOL_SEARCH => self.tools.search(&arguments),
            TOOL_GET_METADATA => self.tools.get_metadata(&arguments),
            TOOL_READ_TEXT => self.tools.read_text(&arguments),
            _ => self.tools.open(&arguments),
        }
    }
}

impl FilesTools {
    /// 检索（§48）：服务层做索引 / 实时扫描与授权；未配置允许根 → 受控错误。
    pub fn search(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let spec = FileQuery {
            query: optional_string(arguments, "query").unwrap_or_default(),
            extension: extension_arg(arguments),
            root_id: optional_string(arguments, "root_id"),
            modified_after: arguments
                .get("modified_after")
                .and_then(serde_json::Value::as_i64),
            limit: usize_arg(arguments, "limit").unwrap_or(10),
            // §71：凭据/密钥类文件默认不进入检索结果。
            include_restricted: false,
        };
        let entries = self
            .service
            .search(&self.settings(), &spec)
            .map_err(tool_error)?;
        if entries.is_empty() {
            return Ok(ToolResult::ok(serde_json::json!({
                "items": [],
                "note": "没有找到匹配的文件",
            })));
        }
        let items: Vec<serde_json::Value> = entries.iter().map(file_json).collect();
        Ok(ToolResult::ok_with_metadata(
            serde_json::json!({"items": items, "count": items.len()}),
            serde_json::json!({
                "ui_hint": {"ui_blocks": [file_list_block("文件命中", &items)]}
            }),
        ))
    }

    pub fn get_metadata(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let target = require_string(arguments, "target")?;
        let entry = self
            .service
            .metadata(&self.settings(), &target)
            .map_err(tool_error)?;
        let item = file_json(&entry);
        Ok(ToolResult::ok_with_metadata(
            item.clone(),
            serde_json::json!({
                "ui_hint": {"ui_blocks": [file_list_block("文件", std::slice::from_ref(&item))]}
            }),
        ))
    }

    /// 安全读取（§42/§47）：受限 / 二进制 / 超大 → 只回元数据，**绝不把字节给模型**。
    pub fn read_text(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let target = require_string(arguments, "target")?;
        let settings = self.settings();
        let max_chars = usize_arg(arguments, "max_chars").unwrap_or(settings.max_read_chars);
        match self.service.read_text(&settings, &target, max_chars) {
            Ok(result) => Ok(ToolResult::ok(serde_json::json!({
                "file": file_json(&result.file),
                "text": result.text,
                "truncated": result.truncated,
                "char_count": result.char_count,
            }))),
            // 只有「不是文本」与「过大」降级为元数据；其余拒绝原因如实报错。
            Err(ApplicationError::Files {
                reason: FileAccessDenied::NotText | FileAccessDenied::TooLarge,
                message,
                ..
            }) => {
                let entry = self.service.metadata(&settings, &target).map_err(tool_error)?;
                Ok(ToolResult::ok(serde_json::json!({
                    "file": file_json(&entry),
                    "text": "",
                    "truncated": false,
                    "content_available": false,
                    "note": message,
                })))
            }
            Err(error) => Err(tool_error(error)),
        }
    }

    /// 打开请求（§46）：只产出 Action，绝不执行 shell。
    pub fn open(&self, arguments: &serde_json::Value) -> Result<ToolResult, AgentError> {
        let target = require_string(arguments, "target")?;
        let (entry, action) = self
            .service
            .open_action(&self.settings(), &target)
            .map_err(tool_error)?;
        let item = file_json(&entry);
        let action = serde_json::to_value(&action).unwrap_or(serde_json::Value::Null);
        Ok(ToolResult::ok_with_metadata(
            serde_json::json!({"file": item}),
            serde_json::json!({
                "ui_hint": {
                    "actions": [action],
                    "ui_blocks": [file_list_block("文件", std::slice::from_ref(&item))],
                }
            }),
        ))
    }
}

/// 文件 JSON（§5 `file_list` item 形状；路径为展示形式，不含任何文件内容）。
#[must_use]
pub fn file_json(entry: &FileMetadata) -> serde_json::Value {
    serde_json::json!({
        "file_id": entry.file_id,
        "file_name": entry.file_name,
        "relative_path": entry.relative_path,
        "path": entry.path,
        "extension": entry.extension,
        "size_bytes": entry.size_bytes,
        "modified_at": entry.modified_at,
        "restricted": entry.restricted,
    })
}

fn file_list_block(title: &str, items: &[serde_json::Value]) -> serde_json::Value {
    serde_json::json!({
        "kind": "file_list",
        "title": title,
        "data": {"items": items},
    })
}




/// 扩展名归一化（去掉前导点、转小写；与 `extension_of` 的输出格式一致）。
fn extension_arg(arguments: &serde_json::Value) -> Option<String> {
    let raw = optional_string(arguments, "extension")?;
    let trimmed = raw.trim_start_matches('.').trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_ascii_lowercase())
    }
}

// ---------------------------------------------------------------------------
// ContextProvider
// ---------------------------------------------------------------------------

/// Files 模块上下文提供方：只给元数据，**永不读正文**（§47）。
pub struct FilesProviderOwned {
    tools: FilesTools,
}

impl FilesProviderOwned {
    #[must_use]
    pub fn new(
        service: Arc<FileService>,
        settings: Arc<dyn Fn() -> KnowledgeSettings + Send + Sync>,
    ) -> Self {
        Self {
            tools: FilesTools::new(service, settings),
        }
    }
}

impl ModuleContextProvider for FilesProviderOwned {
    fn module_id(&self) -> &str {
        "files"
    }

    fn build_context(
        &self,
        app_context: &AppContext,
        _budget: &ContextBudget,
    ) -> Result<ContextBundle, AgentError> {
        let settings = self.tools.settings();
        if let Some(entity) = &app_context.entity {
            if entity.kind != "file" {
                return Err(AgentError::context(format!(
                    "files context: unsupported entity kind `{}` (expected file)",
                    entity.kind
                )));
            }
            let entry = self
                .tools
                .service
                .metadata(&settings, &entity.id)
                .map_err(tool_error)?;
            let mut summary = serde_json::json!({
                "module": "files",
                "entity": {"kind": "file", "id": entry.file_id, "label": entry.file_name},
                "file": file_json(&entry),
            });
            if entry.restricted {
                summary["note"] = serde_json::Value::String(
                    "该文件属于受限文件，内容不可读".to_string(),
                );
            }
            return Ok(ContextBundle {
                module: "files".to_string(),
                headline: format!("Files · {}", entry.file_name),
                summary,
            });
        }
        let stats = self.tools.service.stats().map_err(tool_error)?;
        Ok(ContextBundle {
            module: "files".to_string(),
            headline: "Files".to_string(),
            summary: serde_json::json!({
                "module": "files",
                "note": "文件总览",
                "roots": roots_json(&settings.file_roots),
                "stats": {
                    "files": stats.files,
                    "text_files": stats.text_files,
                    "binary_files": stats.binary_files,
                    "restricted": stats.restricted,
                    "failed": stats.failed,
                },
            }),
        })
    }
}

fn roots_json(roots: &[KnowledgeRoot]) -> Vec<serde_json::Value> {
    roots.iter().map(root_json).collect()
}

fn root_json(root: &KnowledgeRoot) -> serde_json::Value {
    serde_json::json!({
        "id": root.id,
        "label": root.label,
        "path": root.path,
        "enabled": root.enabled,
    })
}

/// 注册 Files 模块（descriptor + 4 工具 + context provider）。
pub fn register_files(
    modules: &mut ModuleRegistry,
    tools: &mut ToolRegistry,
    service: Arc<FileService>,
    settings: Arc<dyn Fn() -> KnowledgeSettings + Send + Sync>,
) -> Result<(), AgentError> {
    modules.register(ModuleRegistration {
        descriptor: ModuleDescriptor {
            id: "files".into(),
            display_name: "Files".into(),
            description: "允许目录内的文件：检索 / 元数据 / 安全读取 / 打开（只读，全部经允许根授权）"
                .into(),
            capabilities: vec![
                "search".into(),
                "metadata".into(),
                "read".into(),
                "open".into(),
            ],
            tools: TOOL_NAMES.iter().map(|name| (*name).to_string()).collect(),
        },
        context_provider: Some(Arc::new(FilesProviderOwned::new(
            Arc::clone(&service),
            Arc::clone(&settings),
        ))),
    })?;
    let shared = Arc::new(FilesTools::new(service, settings));
    for name in TOOL_NAMES {
        tools.register(Arc::new(ToolImpl {
            name,
            tools: Arc::clone(&shared),
        }))?;
    }
    Ok(())
}

#[cfg(test)]
mod files_tests;
