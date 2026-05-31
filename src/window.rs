//! 窗口尺寸、位置和 Windows 平台窗口能力。
//!
//! Slint 负责绘制无边框窗口，这个模块集中处理窗口状态保存、恢复、
//! 拖拽缩放以及 Windows 置顶等平台相关代码。

use crate::{alarm_alert, app::SharedAppData, storage, AppWindow};
use slint::ComponentHandle;

pub const APP_TITLE: &str = "便签";

const DEFAULT_WINDOW_W: i32 = 320;
const DEFAULT_WINDOW_H: i32 = 480;
const MIN_WINDOW_W: i32 = 280;
const MIN_WINDOW_H: i32 = 320;
const MAX_WINDOW_W: i32 = 600;
const MAX_WINDOW_H: i32 = 900;

/// 限制窗口尺寸，避免拖拽缩放导致窗口过小或过大。
pub fn clamp_window_size(width: i32, height: i32) -> (i32, i32) {
    (
        width.max(MIN_WINDOW_W).min(MAX_WINDOW_W),
        height.max(MIN_WINDOW_H).min(MAX_WINDOW_H),
    )
}

/// 应用窗口尺寸，并同步给 Slint 的窗口属性。
pub fn apply_window_size(ui: &AppWindow, width: i32, height: i32) {
    let (width, height) = clamp_window_size(width, height);

    ui.set_window_w(width as f32);
    ui.set_window_h(height as f32);
    ui.window()
        .set_size(slint::PhysicalSize::new(width as u32, height as u32));
}

/// 把当前窗口位置和尺寸写回持久化数据。
pub fn save_window_state(ui: &AppWindow, data: &SharedAppData) {
    let pos = ui.window().position();
    let size = ui.window().size();
    let is_pinned = ui.get_is_pinned();
    let mut data = data.borrow_mut();

    data.window_x = pos.x;
    data.window_y = pos.y;
    data.window_w = size.width as i32;
    data.window_h = size.height as i32;
    data.is_pinned = is_pinned;
    data.save();
}

pub fn set_click_through(_ui: &AppWindow, data: &SharedAppData, enabled: bool) {
    apply_click_through(APP_TITLE, enabled);
    _ui.set_is_click_through(enabled);

    let mut data = data.borrow_mut();
    data.is_click_through = enabled;
    data.save();
}

pub fn toggle_click_through(ui: &AppWindow, data: &SharedAppData) -> bool {
    let enabled = !data.borrow().is_click_through;
    set_click_through(ui, data, enabled);
    enabled
}

pub fn set_pinned(ui: &AppWindow, data: &SharedAppData, pinned: bool) {
    set_topmost(APP_TITLE, pinned);
    ui.set_is_pinned(pinned);

    let mut data = data.borrow_mut();
    data.is_pinned = pinned;
    data.save();
}

pub fn toggle_pinned(ui: &AppWindow, data: &SharedAppData) -> bool {
    let pinned = !ui.get_is_pinned();
    set_pinned(ui, data, pinned);
    pinned
}

/// 从上次保存的位置恢复窗口；没有记录时默认放到屏幕右上角。
pub fn restore_window_state(ui: &AppWindow, data: &storage::AppData) {
    let (ww, wh) = if data.window_w > 0 && data.window_h > 0 {
        clamp_window_size(data.window_w, data.window_h)
    } else {
        (DEFAULT_WINDOW_W, DEFAULT_WINDOW_H)
    };

    apply_window_size(ui, ww, wh);

    if data.window_x >= 0 && data.window_y >= 0 {
        ui.window()
            .set_position(slint::PhysicalPosition::new(data.window_x, data.window_y));
    } else {
        let (x, y) = calc_top_right(ww, wh);
        ui.window().set_position(slint::PhysicalPosition::new(x, y));
    }

    let is_pinned = data.is_pinned;
    let is_click_through = data.is_click_through;
    ui.set_is_pinned(is_pinned);

    // 延迟设置置顶，确保窗口已完全创建，FindWindowW 能找到它
    let ui_weak = ui.as_weak();
    slint::Timer::single_shot(std::time::Duration::from_millis(100), move || {
        if let Some(ui) = ui_weak.upgrade() {
            configure_tray_only_window(APP_TITLE);
            set_topmost(APP_TITLE, ui.get_is_pinned());
            apply_click_through(APP_TITLE, is_click_through);
        }
    });
}

/// 显示窗口并尽量把它激活到最前面。
pub fn show_and_focus(ui: &AppWindow) {
    let _ = ui.window().show();
    focus_window(APP_TITLE, ui.get_is_pinned());
    alarm_alert::refocus_active_alert();
}

/// 根据拖拽偏移移动无边框窗口。
pub fn move_window(ui: &AppWindow, offset_x: f32, offset_y: f32) {
    let window = ui.window();
    let current = window.position();
    let scale = window.scale_factor();
    let dx = (offset_x * scale).round() as i32;
    let dy = (offset_y * scale).round() as i32;

    window.set_position(slint::PhysicalPosition::new(current.x + dx, current.y + dy));
}

/// 根据拖拽偏移调整窗口尺寸，并立即保存尺寸。
pub fn resize_window(ui: &AppWindow, data: &SharedAppData, dx: f32, dy: f32) {
    let window = ui.window();
    let size = window.size();
    let scale = window.scale_factor();
    let dw = (dx * scale).round() as i32;
    let dh = (dy * scale).round() as i32;
    let (new_w, new_h) = clamp_window_size(size.width as i32 + dw, size.height as i32 + dh);

    apply_window_size(ui, new_w, new_h);

    let mut data = data.borrow_mut();
    data.window_w = new_w;
    data.window_h = new_h;
    data.save();
}

#[cfg(target_os = "windows")]
fn to_wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// 设置窗口是否置顶。
#[cfg(target_os = "windows")]
pub fn set_topmost(title: &str, topmost: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE,
        SWP_SHOWWINDOW,
    };

    let title_wide = to_wide(title);
    unsafe {
        if let Ok(hwnd) = FindWindowW(None, windows::core::PCWSTR(title_wide.as_ptr())) {
            let insert_after = if topmost { HWND_TOPMOST } else { HWND_NOTOPMOST };
            let _ = SetWindowPos(
                hwnd,
                insert_after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            alarm_alert::refocus_active_alert();
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_topmost(_title: &str, _topmost: bool) {}

#[cfg(target_os = "windows")]
pub fn apply_click_through(title: &str, enabled: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE,
        SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_LAYERED, WS_EX_TRANSPARENT,
    };

    let title_wide = to_wide(title);
    unsafe {
        if let Ok(hwnd) = FindWindowW(None, windows::core::PCWSTR(title_wide.as_ptr())) {
            let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let mask = WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0;
            if enabled {
                ex_style |= mask;
            } else {
                ex_style &= !mask;
            }

            let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn apply_click_through(_title: &str, _enabled: bool) {}

#[cfg(target_os = "windows")]
pub fn configure_tray_only_window(title: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE,
        SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_APPWINDOW,
        WS_EX_TOOLWINDOW,
    };

    let title_wide = to_wide(title);
    unsafe {
        if let Ok(hwnd) = FindWindowW(None, windows::core::PCWSTR(title_wide.as_ptr())) {
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let tray_only_style = (ex_style | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0;

            if tray_only_style != ex_style {
                let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, tray_only_style as isize);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
                );
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn configure_tray_only_window(_title: &str) {}

#[cfg(target_os = "windows")]
fn focus_window(title: &str, topmost: bool) {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, FindWindowW, GetForegroundWindow, GetWindowThreadProcessId,
        SetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOP, HWND_TOPMOST,
        SW_RESTORE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };

    let title_wide = to_wide(title);
    unsafe {
        if let Ok(hwnd) = FindWindowW(None, windows::core::PCWSTR(title_wide.as_ptr())) {
            let _ = ShowWindow(hwnd, SW_RESTORE);

            let current_thread = GetCurrentThreadId();
            let foreground = GetForegroundWindow();
            let foreground_thread = if !foreground.0.is_null() {
                GetWindowThreadProcessId(foreground, None)
            } else {
                0
            };
            let attached = foreground_thread != 0
                && foreground_thread != current_thread
                && AttachThreadInput(current_thread, foreground_thread, true).as_bool();

            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);

            if attached {
                let _ = AttachThreadInput(current_thread, foreground_thread, false);
            }

            let insert_after = if topmost { HWND_TOPMOST } else { HWND_TOP };
            let _ = SetWindowPos(
                hwnd,
                insert_after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn focus_window(_title: &str, _topmost: bool) {}

/// 计算窗口默认右上角位置。
#[cfg(target_os = "windows")]
fn calc_top_right(window_width: i32, _window_height: i32) -> (i32, i32) {
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
    };
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

    let title_wide = to_wide(APP_TITLE);
    unsafe {
        if let Ok(hwnd) = FindWindowW(None, windows::core::PCWSTR(title_wide.as_ptr())) {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(monitor, &mut info).as_bool() {
                return (info.rcWork.right - window_width - 30, 30);
            }
        }
    }

    (100, 100)
}

#[cfg(not(target_os = "windows"))]
fn calc_top_right(_window_width: i32, _window_height: i32) -> (i32, i32) {
    (100, 100)
}
