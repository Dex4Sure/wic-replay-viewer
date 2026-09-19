#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
fn configure_webkit_rendering() {
    const SHARED_MEMORY_SETTING: &str = "WEBKIT_DMABUF_RENDERER_FORCE_SHM";
    const VIEWER_BACKEND_SETTING: &str = "WIC_REPLAY_VIEWER_GDK_BACKEND";
    let viewer_backend =
        std::env::var_os(VIEWER_BACKEND_SETTING).unwrap_or_else(|| "wayland".into());
    // SAFETY: this is the first operation in the process, before Tauri, WebKitGTK,
    // or any application worker threads have been initialized.
    unsafe {
        // Use native Wayland by default, including for direct binary launches.
        // The viewer-specific setting keeps X11 available as an explicit fallback.
        std::env::set_var("GDK_BACKEND", viewer_backend);
        if std::env::var_os(SHARED_MEMORY_SETTING).is_none() {
            std::env::set_var(SHARED_MEMORY_SETTING, "1");
        }
    }
}

fn main() {
    #[cfg(target_os = "linux")]
    configure_webkit_rendering();
    wic_replay_viewer_app::run();
}
