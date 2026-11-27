mod media;
mod model;
mod overlay;
mod vlc;

#[cfg(not(windows))]
mod demo;
#[cfg(windows)]
mod gsmtc;
#[cfg(windows)]
mod win32;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            media::start(app.handle());
            overlay::setup(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            media::get_state,
            media::media_toggle,
            media::media_next,
            media::media_prev,
            media::media_seek,
            overlay::set_hit_rect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the island app");
}
