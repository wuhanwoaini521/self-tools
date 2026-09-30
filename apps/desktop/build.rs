fn main() {
    #[cfg(feature = "e2e")]
    let attributes =
        tauri_build::Attributes::new().capabilities_path_pattern("./**/capabilities/**/*.json");
    #[cfg(not(feature = "e2e"))]
    let attributes =
        tauri_build::Attributes::new().capabilities_path_pattern("./capabilities/**/*.json");

    tauri_build::try_build(attributes).expect("failed to build Tauri application context");
}
