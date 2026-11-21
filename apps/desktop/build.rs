use std::path::Path;

/// `tauri.conf.json` 把 `history-data-pipeline/dist/history.duckdb` 声明为 bundle
/// resource，`tauri_build` 在 `cargo check` 阶段就会校验它存在。而该文件是
/// pipeline 的 gitignored 构建产物：CI 不 checkout submodule，全新克隆也没有。
///
/// 这里在缺失时创建一个 0 字节占位，保证编译能通过；运行时
/// `HistoryDuckDbRepository::open_or_attach` 会把它当作"数据不可用"降级，
/// History 页面显示引导卡片，其余功能不受影响。
fn ensure_history_resource() {
    let resource = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../history-data-pipeline/dist/history.duckdb");
    if resource.exists() {
        return;
    }
    if let Some(parent) = resource.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::write(&resource, b"").is_err() {
        eprintln!(
            "warning: could not create history placeholder at {}",
            resource.display()
        );
    }
}

fn main() {
    ensure_history_resource();
    // 真实产物由 pipeline 重新生成时，重新解析 bundle resource。
    println!("cargo:rerun-if-changed=../../history-data-pipeline/dist/history.duckdb");

    #[cfg(feature = "e2e")]
    let attributes =
        tauri_build::Attributes::new().capabilities_path_pattern("./**/capabilities/**/*.json");
    #[cfg(not(feature = "e2e"))]
    let attributes =
        tauri_build::Attributes::new().capabilities_path_pattern("./capabilities/**/*.json");

    tauri_build::try_build(attributes).expect("failed to build Tauri application context");
}
