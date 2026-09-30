fn main() {
    let windows = tauri_build::WindowsAttributes::new()
        .static_vc_runtime(std::env::var("PROFILE").as_deref() == Ok("release"));
    let attributes = tauri_build::Attributes::new().windows_attributes(windows);
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
