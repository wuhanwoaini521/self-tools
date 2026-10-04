//! Personal Knowledge 运行时（已下沉到 infrastructure，两端共用）。
//!
//! 实现见 `devtoolbox_infrastructure::knowledge_runtime`；这里只做 re-export，
//! 让桌面命令的历史引用路径继续可用。

pub use devtoolbox_infrastructure::knowledge_runtime::*;
