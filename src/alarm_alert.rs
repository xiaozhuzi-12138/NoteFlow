//! Alarm alert popup management.
//!
//! This module keeps Slint alert windows and alarm audio alive until the user
//! explicitly stops an alert.

use crate::{alarm_audio, app::SharedAppData, ui_sync, window, AlarmAlertWindow, AppWindow};
use slint::{CloseRequestResponse, ComponentHandle, SharedString, Timer, TimerMode};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, SetWindowPos, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW,
};

struct ActiveAlarmAlert {
    id: u64,
    window: AlarmAlertWindow,
    audio: Rc<RefCell<Option<alarm_audio::AlarmAudio>>>,
    /// 闹钟在 AppData 中的存储 ID，用于关闭时删除。
    alarm_storage_id: String,
    /// 共享数据句柄，用于关闭时删除闹钟并同步 UI。
    data: SharedAppData,
    /// 主窗口弱引用，用于同步 UI。
    ui_weak: slint::Weak<AppWindow>,
}

thread_local! {
    static ACTIVE_ALERTS: RefCell<Vec<ActiveAlarmAlert>> = const { RefCell::new(Vec::new()) };
}

#[cfg(target_os = "windows")]
thread_local! {
    static MODAL_OWNER: RefCell<Option<isize>> = const { RefCell::new(None) };
}

static NEXT_ALERT_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static KEEP_TOP_TIMER: RefCell<Option<Timer>> = const { RefCell::new(None) };
}

fn ensure_keep_top_timer() {
    KEEP_TOP_TIMER.with(|t| {
        if t.borrow().is_some() {
            return;
        }
        let timer = Timer::default();
        timer.start(TimerMode::Repeated, std::time::Duration::from_secs(1), || {
            let has_alert = ACTIVE_ALERTS.with(|alerts| !alerts.borrow().is_empty());
            if has_alert {
                focus_topmost_alert();
            }
        });
        *t.borrow_mut() = Some(timer);
    });
}

pub fn show_alarm_alert(
    hour: u32,
    minute: u32,
    memo: &str,
    alarm_id: &str,
    data: SharedAppData,
    ui_weak: slint::Weak<AppWindow>,
) {
    let alert = match AlarmAlertWindow::new() {
        Ok(alert) => alert,
        Err(err) => {
            eprintln!("failed to create alarm alert window: {err}");
            return;
        }
    };

    let alert_id = NEXT_ALERT_ID.fetch_add(1, Ordering::Relaxed);
    alert.set_alarm_time(SharedString::from(format!("{:02}:{:02}", hour, minute)));
    alert.set_memo(SharedString::from(memo.trim()));

    let audio = Rc::new(RefCell::new(Some(alarm_audio::AlarmAudio::start_looping())));

    // Make alert modal: disable the main window while at least one alert is active.
    let modal_acquired = acquire_modal_owner();

    {
        let audio_for_stop = audio.clone();
        let alarm_storage_id = alarm_id.to_string();
        let data_for_stop = data.clone();
        let ui_weak_for_stop = ui_weak.clone();
        alert.on_stop_alarm(move || {
            dismiss_alert(alert_id, &audio_for_stop, &alarm_storage_id, &data_for_stop, &ui_weak_for_stop);
        });
    }

    {
        let audio_for_close = audio.clone();
        let alarm_storage_id = alarm_id.to_string();
        let data_for_close = data.clone();
        let ui_weak_for_close = ui_weak.clone();
        alert.window().on_close_requested(move || {
            dismiss_alert(alert_id, &audio_for_close, &alarm_storage_id, &data_for_close, &ui_weak_for_close);
            CloseRequestResponse::HideWindow
        });
    }

    // 先定位再显示，避免窗口闪烁
    position_alert_centered_on_app(&alert);

    if let Err(err) = alert.show() {
        eprintln!("failed to show alarm alert window: {err}");
        let _ = audio.borrow_mut().take();
        if modal_acquired {
            release_modal_owner_if_no_alerts();
        }
        return;
    }

    focus_topmost_alert();

    // Ensure the keep-top timer is running while alerts are active.
    ensure_keep_top_timer();

    ACTIVE_ALERTS.with(|alerts| {
        alerts.borrow_mut().push(ActiveAlarmAlert {
            id: alert_id,
            window: alert,
            audio,
            alarm_storage_id: alarm_id.to_string(),
            data,
            ui_weak,
        });
    });
}

pub fn refocus_active_alert() {
    let has_alert = ACTIVE_ALERTS.with(|alerts| !alerts.borrow().is_empty());
    if has_alert {
        focus_topmost_alert();
    }
}

fn dismiss_alert(
    alert_id: u64,
    audio: &Rc<RefCell<Option<alarm_audio::AlarmAudio>>>,
    alarm_storage_id: &str,
    data: &SharedAppData,
    ui_weak: &slint::Weak<AppWindow>,
) {
    let _ = audio.borrow_mut().take();

    let (removed, became_empty) = ACTIVE_ALERTS.with(|alerts| {
        let mut alerts = alerts.borrow_mut();
        if let Some(pos) = alerts.iter().position(|entry| entry.id == alert_id) {
            let entry = alerts.remove(pos);
            let _ = entry.window.hide();
            let _ = entry.audio.borrow_mut().take();
            (true, alerts.is_empty())
        } else {
            (false, alerts.is_empty())
        }
    });

    if removed {
        // 删除闹钟数据并同步 UI
        {
            let mut data = data.borrow_mut();
            data.delete_alarm(alarm_storage_id);
        }
        if let Some(ui) = ui_weak.upgrade() {
            ui_sync::sync_alarms(&ui, &data.borrow());
        }
    }

    if removed && became_empty {
        release_modal_owner();
    }
}

fn release_modal_owner_if_no_alerts() {
    let has_active_alert = ACTIVE_ALERTS.with(|alerts| !alerts.borrow().is_empty());
    if !has_active_alert {
        release_modal_owner();
    }
}

#[cfg(target_os = "windows")]
fn to_wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(target_os = "windows")]
fn find_window_by_title(title: &str) -> Option<HWND> {
    let title_wide = to_wide(title);
    unsafe { FindWindowW(None, windows::core::PCWSTR(title_wide.as_ptr())).ok() }
}

#[cfg(target_os = "windows")]
fn find_main_window() -> Option<HWND> {
    find_window_by_title(window::APP_TITLE)
}

#[cfg(target_os = "windows")]
fn acquire_modal_owner() -> bool {
    let Some(main_hwnd) = find_main_window() else {
        return false;
    };

    MODAL_OWNER.with(|owner| {
        let mut owner = owner.borrow_mut();
        if owner.is_none() {
            unsafe {
                let _ = EnableWindow(main_hwnd, false);
            }
            *owner = Some(main_hwnd.0 as isize);
            true
        } else {
            false
        }
    })
}

#[cfg(not(target_os = "windows"))]
fn acquire_modal_owner() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn release_modal_owner() {
    MODAL_OWNER.with(|owner| {
        let raw = owner.borrow_mut().take();
        if let Some(raw_hwnd) = raw {
            let hwnd = HWND(raw_hwnd as _);
            unsafe {
                let _ = EnableWindow(hwnd, true);
                let _ = SetForegroundWindow(hwnd);
            }
        }
    });
}

#[cfg(not(target_os = "windows"))]
fn release_modal_owner() {}

#[cfg(target_os = "windows")]
fn position_alert_centered_on_app(alert: &AlarmAlertWindow) {
    let Some(main_hwnd) = find_main_window() else {
        return;
    };

    let mut app_rect = RECT::default();
    unsafe {
        if windows::Win32::UI::WindowsAndMessaging::GetWindowRect(main_hwnd, &mut app_rect).is_err() {
            return;
        }
    }

    // 使用窗口物理尺寸，若尚未显示则使用 Slint 定义的 preferred 尺寸
    let alert_size = alert.window().size();
    let alert_w = if alert_size.width > 0 { alert_size.width as i32 } else { 360 };
    let alert_h = if alert_size.height > 0 { alert_size.height as i32 } else { 260 };

    let app_width = app_rect.right - app_rect.left;
    let app_height = app_rect.bottom - app_rect.top;
    let mut x = app_rect.left + (app_width - alert_w) / 2;
    let mut y = app_rect.top + (app_height - alert_h) / 2;

    let monitor = unsafe { MonitorFromWindow(main_hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    unsafe {
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            let work = info.rcWork;

            let max_x = work.right - alert_w;
            x = if max_x >= work.left {
                x.clamp(work.left, max_x)
            } else {
                work.left
            };

            let max_y = work.bottom - alert_h;
            y = if max_y >= work.top {
                y.clamp(work.top, max_y)
            } else {
                work.top
            };
        }
    }

    alert.window().set_position(slint::PhysicalPosition::new(x, y));
}

#[cfg(not(target_os = "windows"))]
fn position_alert_centered_on_app(_alert: &AlarmAlertWindow) {}

#[cfg(target_os = "windows")]
fn focus_topmost_alert() {
    ACTIVE_ALERTS.with(|alerts| {
        if let Some(alert) = alerts.borrow().last() {
            let _ = alert.window.window().show();
            let _ = alert.window.window().set_position(alert.window.window().position());
        }
    });

    if let Some(alert_hwnd) = find_window_by_title("便签闹钟") {
        unsafe {
            let _ = SetWindowPos(
                alert_hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            let _ = SetForegroundWindow(alert_hwnd);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn focus_topmost_alert() {}
