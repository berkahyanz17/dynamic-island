//! Overlay window behaviour: pinned top-centre, click-through except over the
//! pill, hidden while a fullscreen app is running, hideable from the tray.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, WebviewWindow};

/// Logical size of the transparent canvas. Keep in sync with `CANVAS` in src/layout.ts.
const CANVAS_W: f64 = 480.0;
const CANVAS_H: f64 = 220.0;

/// Cursor check interval: fast when the cursor is near the pill, slow when it
/// is far away, slower still when there is nothing to hover (no media / hidden).
const ACTIVE_TICK: Duration = Duration::from_millis(16);
const FAR_TICK: Duration = Duration::from_millis(60);
const OFF_TICK: Duration = Duration::from_millis(250);
/// "Near" means within this many logical px of the pill's hit rectangle.
const NEAR_PX: f64 = 120.0;

const REPOSITION_EVERY: Duration = Duration::from_secs(2);
const FULLSCREEN_CHECK_EVERY: Duration = Duration::from_secs(1);

/// Region of the window (logical px) that should receive mouse input.
#[derive(Clone, Copy, Default)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Rect {
    fn contains(&self, px: f64, py: f64) -> bool {
        self.near(px, py, 0.0)
    }

    /// Inside the rectangle grown by `margin` on every side. An empty rect is never near.
    fn near(&self, px: f64, py: f64, margin: f64) -> bool {
        self.w > 0.0
            && px >= self.x - margin
            && px <= self.x + self.w + margin
            && py >= self.y - margin
            && py <= self.y + self.h + margin
    }
}

#[derive(Default)]
pub struct OverlayState {
    hit: Mutex<Rect>,
    user_hidden: AtomicBool,
}

#[tauri::command]
pub fn set_hit_rect(state: State<'_, OverlayState>, x: f64, y: f64, w: f64, h: f64) {
    if let Ok(mut r) = state.hit.lock() {
        *r = Rect { x, y, w, h };
    }
}

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    app.manage(OverlayState::default());

    let window = app
        .get_webview_window("main")
        .expect("main window missing from tauri.conf.json");

    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        crate::win32::apply_overlay_styles(hwnd.0 as isize);
    }

    // Start fully click-through; the watcher thread opens it up over the pill.
    let _ = window.set_ignore_cursor_events(true);
    place(&window);
    let _ = window.show();

    build_tray(app)?;
    spawn_watcher(app.handle().clone(), window);
    Ok(())
}

/// Centre the canvas on the top edge of the primary monitor.
fn place(window: &WebviewWindow) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    let width_px = (CANVAS_W * scale).round() as i32;
    let x = monitor.position().x + (monitor.size().width as i32 - width_px) / 2;
    let y = monitor.position().y;

    let want_size = LogicalSize::new(CANVAS_W, CANVAS_H);
    let size_ok = window
        .inner_size()
        .map(|s| s.width as i32 == width_px && s.height as i32 == (CANVAS_H * scale).round() as i32)
        .unwrap_or(false);
    if !size_ok {
        let _ = window.set_size(want_size);
    }
    let pos_ok = window
        .outer_position()
        .map(|p| p.x == x && p.y == y)
        .unwrap_or(false);
    if !pos_ok {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

/// Cursor position in the window's logical coordinates.
///
/// On Windows this is plain Win32 calls. The Tauri equivalents (`outer_position`,
/// `scale_factor`, ...) each make a round trip to the UI thread, which is far too
/// expensive to do dozens of times a second.
#[cfg(windows)]
fn cursor_local(_app: &AppHandle, _window: &WebviewWindow, hwnd: Option<isize>) -> Option<(f64, f64)> {
    crate::win32::cursor_in_window(hwnd?)
}

#[cfg(not(windows))]
fn cursor_local(app: &AppHandle, window: &WebviewWindow, _hwnd: Option<isize>) -> Option<(f64, f64)> {
    let cursor = app.cursor_position().ok()?;
    let origin = window.outer_position().ok()?;
    let scale = window.scale_factor().ok()?;
    Some((
        (cursor.x - origin.x as f64) / scale,
        (cursor.y - origin.y as f64) / scale,
    ))
}

fn spawn_watcher(app: AppHandle, window: WebviewWindow) {
    #[cfg(windows)]
    let hwnd = window.hwnd().ok().map(|h| h.0 as isize);
    #[cfg(not(windows))]
    let hwnd: Option<isize> = None;

    std::thread::Builder::new()
        .name("overlay".into())
        .spawn(move || {
            let mut ignoring = true;
            let mut visible = true;
            let mut fullscreen = false;
            let mut last_place = Instant::now();
            let mut last_fs = Instant::now()
                .checked_sub(FULLSCREEN_CHECK_EVERY)
                .unwrap_or_else(Instant::now);

            loop {
                let state = app.state::<OverlayState>();
                let mut pause = FAR_TICK;

                if last_place.elapsed() >= REPOSITION_EVERY {
                    last_place = Instant::now();
                    place(&window); // handles monitor / DPI / resolution changes
                }

                #[cfg(windows)]
                if last_fs.elapsed() >= FULLSCREEN_CHECK_EVERY {
                    last_fs = Instant::now();
                    fullscreen = crate::win32::fullscreen_app_running();
                }
                #[cfg(not(windows))]
                {
                    let _ = (&mut last_fs, &mut fullscreen);
                }

                let want_visible = !fullscreen && !state.user_hidden.load(Ordering::Relaxed);
                if want_visible != visible {
                    visible = want_visible;
                    let _ = if visible { window.show() } else { window.hide() };
                }

                if visible {
                    // Click-through everywhere except over the pill.
                    let hit = state.hit.lock().map(|r| *r).unwrap_or_default();
                    if hit.w <= 0.0 {
                        pause = OFF_TICK; // nothing to hover
                    }
                    if let Some((lx, ly)) = cursor_local(&app, &window, hwnd) {
                        let over_pill = hit.contains(lx, ly);
                        if over_pill == ignoring {
                            ignoring = !over_pill;
                            let _ = window.set_ignore_cursor_events(ignoring);
                            // The webview can't see the cursor while click-through, so
                            // it gets hover state from here.
                            let _ = app.emit("island://over", over_pill);
                        }
                        if hit.near(lx, ly, NEAR_PX) {
                            pause = ACTIVE_TICK;
                        }
                    }
                } else {
                    pause = OFF_TICK;
                }

                std::thread::sleep(pause);
            }
        })
        .expect("failed to spawn overlay thread");
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Show / hide island", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &quit])?;

    let mut tray = TrayIconBuilder::new()
        .tooltip("Island")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => {
                let state = app.state::<OverlayState>();
                let hidden = state.user_hidden.load(Ordering::Relaxed);
                state.user_hidden.store(!hidden, Ordering::Relaxed);
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
