//! 暴露策略（V8 §17-§19/§36-§40/§67-§77）。
//!
//! **Tool discovery 也必须授权**（§18）：未授权 client 不能通过 `tools/list`
//! 看见 private services / files / server actions / personal memory。
//!
//! 本模块是**纯函数表**：tool 名 → exposure group → 必需 scope → remote 可见性。
//! 默认表集中在这里（§67），`settings.mcp` 可覆盖（§47）。

use crate::personal_ai::ToolRisk;

/// 暴露分组（§67）。默认 remote catalog = `BasicRead`（§68）。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum ExposureGroup {
    /// 只读基础能力（server read / apps list 之类无个人数据面）。
    #[default]
    BasicRead,
    /// 个人知识只读（memory / documents / files）。
    KnowledgeRead,
    /// 业务模块只读（history / travel / geography / language）。
    ModuleRead,
    /// 安全写入（受 registry risk 门禁约束）。
    SafeWrite,
    /// 系统操作（必须 SafeAction + 确认，§38）。
    SystemAction,
}

impl ExposureGroup {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ExposureGroup::BasicRead => "basic_read",
            ExposureGroup::KnowledgeRead => "knowledge_read",
            ExposureGroup::ModuleRead => "module_read",
            ExposureGroup::SafeWrite => "safe_write",
            ExposureGroup::SystemAction => "system_action",
        }
    }
}

/// 单个工具的暴露策略。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolExposure {
    pub group: ExposureGroup,
    /// 执行所需 scope（§37：risk → scope）。
    pub required_scope: &'static str,
    /// 远程是否可见（§77：remote 默认只有基础 read）。
    pub remote_visible: bool,
}

/// 默认 scope 常量（与 `McpScope` 对齐，avoid 循环依赖字符串）。
const SCOPE_READ: &str = "selftools.read";
const SCOPE_MEMORY_READ: &str = "memory.read";
const SCOPE_DOCUMENTS_READ: &str = "documents.read";
const SCOPE_FILES_READ: &str = "files.read";
const SCOPE_SERVER_READ: &str = "server.read";
const SCOPE_HISTORY_ENRICH: &str = "history.enrich";
const SCOPE_MEMORY_WRITE: &str = "memory.write";
const SCOPE_SERVER_ACTION: &str = "server.action";

/// 默认暴露表（§67/§70/§75-§77）。
///
/// **未列出的工具一律不暴露**（fail-closed，§19）：MCP 面是一个受控子集，
/// 不是 ToolRegistry 的镜像。
#[must_use]
pub fn default_exposure(tool_name: &str) -> Option<ToolExposure> {
    let (group, scope, remote) = match tool_name {
        // --- 家庭服务器：基础只读（remote 可见，§77） ---
        "server.get_status" | "server.get_cpu" | "server.get_memory" | "server.get_storage"
        | "server.get_health" => (ExposureGroup::BasicRead, SCOPE_SERVER_READ, true),
        "services.list" | "services.get" | "services.get_status" => {
            (ExposureGroup::BasicRead, SCOPE_SERVER_READ, true)
        }
        "apps.list" | "apps.get" | "apps.get_status" => {
            (ExposureGroup::BasicRead, SCOPE_SERVER_READ, true)
        }

        // --- 个人知识：只读；remote 可见但需各自 scope（§70） ---
        "memory.search" | "memory.list" | "memory.get" => {
            (ExposureGroup::KnowledgeRead, SCOPE_MEMORY_READ, true)
        }
        "documents.search"
        | "documents.get"
        | "documents.read"
        | "documents.get_context"
        | "documents.list_recent" => (ExposureGroup::KnowledgeRead, SCOPE_DOCUMENTS_READ, true),
        "files.search" | "files.get_metadata" | "files.read_text" => {
            (ExposureGroup::KnowledgeRead, SCOPE_FILES_READ, true)
        }
        "knowledge.search" => (ExposureGroup::KnowledgeRead, SCOPE_READ, true),

        // --- 业务模块：只读 ---
        "history.search" | "history.get_event" | "history.get_person" | "history.get_context" => {
            (ExposureGroup::ModuleRead, SCOPE_READ, true)
        }
        "travel.search" | "travel.guide" | "travel.snapshot" | "travel.cache" => {
            (ExposureGroup::ModuleRead, SCOPE_READ, true)
        }
        "geography.search" | "geography.entity" => (ExposureGroup::ModuleRead, SCOPE_READ, true),
        "language.search" | "language.today" | "language.review" | "language.explain" => {
            (ExposureGroup::ModuleRead, SCOPE_READ, true)
        }

        // --- 安全写入 ---
        "memory.save" => (ExposureGroup::SafeWrite, SCOPE_MEMORY_WRITE, true),
        "history.ensure_enrichment" => (ExposureGroup::SafeWrite, SCOPE_HISTORY_ENRICH, true),
        // §73：`files.open` 不回传本地路径；外部只拿元数据 → 归 KnowledgeRead。
        "files.open" => (ExposureGroup::KnowledgeRead, SCOPE_FILES_READ, true),

        // --- 系统操作：remote 不可见（§77），本地也需确认（§38） ---
        "services.restart" => (ExposureGroup::SystemAction, SCOPE_SERVER_ACTION, false),
        "services.get_logs" => (ExposureGroup::BasicRead, SCOPE_SERVER_READ, true),
        "apps.open" => (ExposureGroup::BasicRead, SCOPE_SERVER_READ, true),

        // 未列出 → 不暴露
        _ => return None,
    };
    Some(ToolExposure {
        group,
        required_scope: scope,
        remote_visible: remote,
    })
}

/// tool 名 → 必需 scope 的字符串（未列出 → `None`）。
#[must_use]
pub fn required_scope_for(tool_name: &str) -> Option<&'static str> {
    default_exposure(tool_name).map(|exposure| exposure.required_scope)
}

/// tool 是否对远程可见（未列出 → false）。
#[must_use]
pub fn is_remote_visible(tool_name: &str) -> bool {
    default_exposure(tool_name).is_some_and(|exposure| exposure.remote_visible)
}

/// tool risk → scope 映射（§37；与 `default_exposure` 的 required_scope 一致）。
#[must_use]
pub fn scope_for_risk(risk: ToolRisk, module: &str) -> &'static str {
    match risk {
        // READ：模块化 read scope 优先，未知名模块回落到宽 read。
        ToolRisk::Read => match module {
            "memory" => SCOPE_MEMORY_READ,
            "documents" => SCOPE_DOCUMENTS_READ,
            "files" => SCOPE_FILES_READ,
            "knowledge" => SCOPE_READ,
            _ => SCOPE_READ,
        },
        ToolRisk::SafeWrite => match module {
            "memory" => SCOPE_MEMORY_WRITE,
            "history" => SCOPE_HISTORY_ENRICH,
            _ => SCOPE_READ,
        },
        // SensitiveWrite / System：registry 门禁拒绝 SensitiveWrite；
        // System 必须有 server.action（但仍需确认，§38）。
        ToolRisk::SensitiveWrite | ToolRisk::System => SCOPE_SERVER_ACTION,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::principal::McpScope;

    #[test]
    fn private_tool_names_are_not_exposed() {
        // §19：未列出 = 不暴露（fail-closed）。
        for hidden in [
            "files.write",
            "shell.exec",
            "db.query",
            "http.request",
            "server.reboot",
            "documents.scan",
        ] {
            assert!(default_exposure(hidden).is_none(), "不得暴露: {hidden}");
        }
    }

    #[test]
    fn system_actions_are_hidden_from_remote() {
        let exposure = default_exposure("services.restart").expect("restart");
        assert_eq!(exposure.group, ExposureGroup::SystemAction);
        assert!(!exposure.remote_visible, "§77：SYSTEM 对远程隐藏");
        assert_eq!(exposure.required_scope, "server.action");
        assert!(!is_remote_visible("services.restart"));
    }

    #[test]
    fn knowledge_tools_need_module_scopes() {
        assert_eq!(required_scope_for("memory.search"), Some("memory.read"));
        assert_eq!(required_scope_for("files.read_text"), Some("files.read"));
        assert_eq!(
            required_scope_for("documents.get_context"),
            Some("documents.read")
        );
        // §70：memory 工具必须 memory.read，不是宽 selftools.read 之外的东西。
        assert!(is_remote_visible("memory.search"));
    }

    #[test]
    fn basic_server_reads_are_remote_visible() {
        for tool in [
            "server.get_status",
            "server.get_storage",
            "services.list",
            "apps.list",
        ] {
            let exposure = default_exposure(tool).expect("tool");
            assert_eq!(exposure.group, ExposureGroup::BasicRead);
            assert!(exposure.remote_visible, "{tool} 应远程可见");
            assert_eq!(exposure.required_scope, "server.read");
        }
    }

    #[test]
    fn risk_to_scope_mapping_matches_table() {
        assert_eq!(
            scope_for_risk(ToolRisk::Read, "memory"),
            required_scope_for("memory.search").expect("scope")
        );
        assert_eq!(
            scope_for_risk(ToolRisk::Read, "files"),
            required_scope_for("files.search").expect("scope")
        );
        assert_eq!(scope_for_risk(ToolRisk::System, "server"), "server.action");
        assert_eq!(
            scope_for_risk(ToolRisk::SafeWrite, "memory"),
            "memory.write"
        );
    }

    #[test]
    fn scope_string_round_trips_through_mcp_scope() {
        for tool in ["memory.search", "files.read_text", "services.restart"] {
            let scope = required_scope_for(tool).expect("scope");
            assert!(McpScope::parse(scope).is_some(), "{tool} 的 scope 必须已知");
        }
    }
}
