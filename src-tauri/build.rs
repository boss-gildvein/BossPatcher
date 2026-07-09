fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "app_exit",
            "call_alias",
            "get_status",
            "patch_files",
        ]),
    ))
    .expect("failed to build Tauri app");
}
