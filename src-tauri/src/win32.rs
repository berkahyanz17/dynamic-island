//! Small Win32 helpers for the overlay window. No Tauri types in here.

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::SHQueryUserNotificationState;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowLongPtrW, GetWindowRect, SetWindowLongPtrW, GWL_EXSTYLE,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

/// Cursor position relative to the window's top-left corner, in logical px.
/// Read straight from Win32, so the overlay thread never waits on the UI thread.
pub fn cursor_in_window(hwnd: isize) -> Option<(f64, f64)> {
    unsafe {
        let hwnd = HWND(hwnd as _);
        let mut cursor = POINT::default();
        GetCursorPos(&mut cursor).ok()?;
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).ok()?;
        let dpi = GetDpiForWindow(hwnd);
        let scale = if dpi == 0 { 1.0 } else { f64::from(dpi) / 96.0 };
        Some((
            f64::from(cursor.x - rect.left) / scale,
            f64::from(cursor.y - rect.top) / scale,
        ))
    }
}

/// Make the window a non-activating tool window: clicking it never steals focus
/// from the game or editor underneath, and it stays out of Alt-Tab.
pub fn apply_overlay_styles(hwnd: isize) {
    unsafe {
        let hwnd = HWND(hwnd as _);
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let add = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, current | add);
    }
}

/// True while a fullscreen app, a Direct3D fullscreen game or presentation mode
/// is in the foreground.
pub fn fullscreen_app_running() -> bool {
    unsafe {
        match SHQueryUserNotificationState() {
            // QUNS_BUSY = 2, QUNS_RUNNING_D3D_FULL_SCREEN = 3, QUNS_PRESENTATION_MODE = 4
            Ok(state) => matches!(state.0, 2 | 3 | 4),
            Err(_) => false,
        }
    }
}
