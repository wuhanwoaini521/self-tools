//! Language Learning Hub 的 infrastructure 适配器：SQLite 存储 + 数据集解析。
//!
//! 解析器全部为纯函数（bytes → `ImportedItem`），可在无网络环境单测；
//! 许可证 Gate 在 `import::gate_license`（未知/非商业 → 拒绝导入）。

pub mod import;
pub mod store;

pub use import::{
    ImportError, ImportReport, ImportedExample, ImportedItem, ImportedMeaning,
    ImportedPronunciation, ImportedRelation, LanguageDatasetImporter, gate_license, import_into,
    sources,
};
pub use store::{ItemDetailRows, LanguageStore, SearchHit};
