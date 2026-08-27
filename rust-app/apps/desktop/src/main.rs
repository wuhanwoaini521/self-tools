// 这些依赖由同一 package 的 library target 使用；显式引用让 binary target 的
// workspace 级 `unused_crate_dependencies` lint 保持有效。
use devtoolbox_application as _;
use devtoolbox_infrastructure as _;
use serde as _;
use tauri as _;
use tauri_plugin_dialog as _;

fn main() {
    devtoolbox_desktop::run();
}
