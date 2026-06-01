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

// ---------------------------------------------------------------------------
// 可靠的窗口查找：枚举所有顶层窗口 + 进程 ID 验证，不再依赖单一
// FindWindowW 标题匹配（避免误匹配到同名的文件夹窗口）。
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
thread_local! {
    /// 主窗口 HWND 缓存（内部存 isize 以便存入 RefCell）。
    static CACHED_MAIN_HWND: std::cell::RefCell<Option<isize>> = const { std::cell::RefCell::new(None) };
}

/// 枚举所有顶层窗口，找到属于当前进程且标题精确匹配的窗口。
/// 这是对 `FindWindowW` 的安全替代：只匹配自己进程的窗口。
#[cfg(target_os = "windows")]
pub fn find_own_window_by_title(title: &str) -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId,
    };

    struct Ctx {
        pid: u32,
        title: String,
        found: Option<HWND>,
    }

    let our_pid = unsafe { GetCurrentProcessId() };
    let mut ctx = Ctx {
        pid: our_pid,
        title: title.to_string(),
        found: None,
    };

    unsafe {
        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let ctx = &mut *(lparam.0 as *mut Ctx);

            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != ctx.pid {
                return BOOL(1); // 不是我们的进程，继续枚举
            }

            let mut buf = [0u16; 512];
            let len = GetWindowTextW(hwnd, &mut buf);
            if len == 0 {
                return BOOL(1); // 无标题窗口，跳过
            }

            let wintitle = String::from_utf16_lossy(&buf[..len as usize]);
            if wintitle == ctx.title {
                ctx.found = Some(hwnd);
                return BOOL(0); // 找到了，停止枚举
            }

            BOOL(1) // 继续
        }

        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut _ as isize));
    }

    ctx.found
}

/// 获取主窗口 HWND，带缓存。其他模块可调用此函数获取已校验的句柄。
#[cfg(target_os = "windows")]
pub fn get_cached_main_hwnd() -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::IsWindow;

    CACHED_MAIN_HWND.with(|cached| {
        if let Some(raw) = *cached.borrow() {
            let hwnd = HWND(raw as *mut _);
            if unsafe { IsWindow(hwnd).as_bool() } {
                return Some(hwnd);
            }
            // 缓存已失效，清除
            *cached.borrow_mut() = None;
        }

        // 重新查找并缓存
        if let Some(hwnd) = find_own_window_by_title(APP_TITLE) {
            *cached.borrow_mut() = Some(hwnd.0 as isize);
            return Some(hwnd);
        }

        None
    })
}

/// 设置窗口是否置顶。
#[cfg(target_os = "windows")]
pub fn set_topmost(_title: &str, topmost: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };

    if let Some(hwnd) = get_cached_main_hwnd() {
        unsafe {
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
        }
        alarm_alert::refocus_active_alert();
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_topmost(_title: &str, _topmost: bool) {}

#[cfg(target_os = "windows")]
pub fn apply_click_through(_title: &str, enabled: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE,
        SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_LAYERED, WS_EX_TRANSPARENT,
    };

    if let Some(hwnd) = get_cached_main_hwnd() {
        unsafe {
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
pub fn configure_tray_only_window(_title: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE,
        SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_APPWINDOW,
        WS_EX_TOOLWINDOW,
    };

    if let Some(hwnd) = get_cached_main_hwnd() {
        unsafe {
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
fn focus_window(_title: &str, topmost: bool) {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId,
        SetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOP, HWND_TOPMOST,
        SW_RESTORE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };

    if let Some(hwnd) = get_cached_main_hwnd() {
        unsafe {
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

/// 计算窗口默认右上角位置。优先使用主窗口所在显示器，若窗口尚未创建
/// 或查找失败则回退到主显示器，再失败则用硬编码默认值。
#[cfg(target_os = "windows")]
fn calc_top_right(window_width: i32, _window_height: i32) -> (i32, i32) {
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
    };

    let fallback = || -> Option<(i32, i32)> {
        let monitor = unsafe { MonitorFromWindow(None, MONITOR_DEFAULTTOPRIMARY) };
        if monitor.0.is_null() {
            return None;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info).as_bool() } {
            Some((info.rcWork.right - window_width - 30, 30))
        } else {
            None
        }
    };

    // 尝试用缓存的 HWND 获取所在显示器
    if let Some(hwnd) = get_cached_main_hwnd() {
        unsafe {
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

    // 窗口尚未创建时用主显示器
    if let Some(pos) = fallback() {
        return pos;
    }

    (100, 100)
}

#[cfg(not(target_os = "windows"))]
fn calc_top_right(_window_width: i32, _window_height: i32) -> (i32, i32) {
    (100, 100)
}
