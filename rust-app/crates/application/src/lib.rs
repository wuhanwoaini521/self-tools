//! UI 无关的文档、工作区与任务编辑用例。

pub mod error;
pub mod workflows;

pub use error::ApplicationError;
pub use workflows::{
    DocumentDto, convert_lines_to_tasks, cycle_lines, load_document, load_settings, save_document,
    save_settings, scan_workspace,
};
