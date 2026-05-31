//! UI 回调绑定。
//!
//! Slint 通过 callback 把用户操作交给 Rust。这里统一注册所有回调，
//! 每个小函数负责一个功能区，避免启动流程里堆满闭包。

use crate::{app::SharedAppData, ui_sync, window, AppWindow};
use chrono::{Datelike, Local, NaiveDate, Timelike};
use slint::{ComponentHandle, SharedString};

/// 绑定应用所有 UI 回调。
pub fn bind_all(ui: &AppWindow, data: SharedAppData) {
    bind_note_handlers(ui, data.clone());
    bind_alarm_handlers(ui, data.clone());
    bind_window_handlers(ui, data);
}

fn bind_note_handlers(ui: &AppWindow, data: SharedAppData) {
    {
        let ui_weak = ui.as_weak();
        let data = data.clone();
        ui.on_save_note(move |id, content| {
            let id = id.to_string();
            let content = content.trim().to_string();
            if content.is_empty() {
                return;
            }

            let mut data = data.borrow_mut();
            if id.is_empty() {
                data.add_note(content);
            } else {
                data.update_note(&id, content);
            }

            if let Some(ui) = ui_weak.upgrade() {
                ui_sync::sync_notes(&ui, &data);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let data = data.clone();
        ui.on_delete_note(move |id| {
            let mut data = data.borrow_mut();
            data.delete_note(&id.to_string());
            if let Some(ui) = ui_weak.upgrade() {
                ui_sync::sync_notes(&ui, &data);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_load_note_for_edit(move |id| {
            let id = id.to_string();
            let data = data.borrow();
            if let Some(note) = data.get_note(&id) {
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_note_editor_id(SharedString::from(&note.id));
                    ui.set_note_editor_content(SharedString::from(&note.content));
                    ui.set_note_editor_open(true);
                }
            }
        });
    }
}

fn bind_alarm_handlers(ui: &AppWindow, data: SharedAppData) {
    {
        let ui_weak = ui.as_weak();
        let data = data.clone();
        ui.on_save_alarm(move |id, year_str, month_str, day_str, hour_str, minute_str, memo| {
            let id = id.to_string();
            let memo = memo.trim().to_string();

            if let Some((year, month, day, hour, minute)) =
                parse_alarm_datetime_str(&year_str, &month_str, &day_str, &hour_str, &minute_str)
            {
                let mut data = data.borrow_mut();
                if id.is_empty() {
                    data.add_alarm(year, month, day, hour, minute, memo);
                } else {
                    data.update_alarm(&id, year, month, day, hour, minute, memo);
                }

                if let Some(ui) = ui_weak.upgrade() {
                    ui_sync::sync_alarms(&ui, &data);
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let data = data.clone();
        ui.on_delete_alarm(move |id| {
            let mut data = data.borrow_mut();
            data.delete_alarm(&id.to_string());
            if let Some(ui) = ui_weak.upgrade() {
                ui_sync::sync_alarms(&ui, &data);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let data = data.clone();
        ui.on_toggle_alarm(move |id| {
            let mut data = data.borrow_mut();
            data.toggle_alarm(&id.to_string());
            if let Some(ui) = ui_weak.upgrade() {
                ui_sync::sync_alarms(&ui, &data);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_prepare_new_alarm(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let now = Local::now();
                ui.set_alarm_editor_year(SharedString::from(now.year().to_string()));
                ui.set_alarm_editor_month(SharedString::from(now.month().to_string()));
                ui.set_alarm_editor_day(SharedString::from(now.day().to_string()));
                ui.set_alarm_editor_hour(SharedString::from(now.hour().to_string()));
                ui.set_alarm_editor_minute(SharedString::from(now.minute().to_string()));
                ui.set_alarm_editor_memo(SharedString::from(""));
                ui.set_alarm_editor_id(SharedString::from(""));
                ui.set_alarm_editor_open(true);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_adjust_alarm_field(move |field, delta| {
            if let Some(ui) = ui_weak.upgrade() {
                let field = field.to_string();
                match field.as_str() {
                    "year" => adjust_editor_field(&ui, "year", delta, 2000, 2099),
                    "month" => adjust_editor_field(&ui, "month", delta, 1, 12),
                    "day" => adjust_editor_field(&ui, "day", delta, 1, 31),
                    "hour" => adjust_editor_field(&ui, "hour", delta, 0, 23),
                    "minute" => adjust_editor_field(&ui, "minute", delta, 0, 59),
                    _ => {}
                }
            }
        });
    }
}

fn bind_window_handlers(ui: &AppWindow, data: SharedAppData) {
    {
        let ui_weak = ui.as_weak();
        let data = data.clone();
        ui.on_request_close(move || {
            if let Some(ui) = ui_weak.upgrade() {
                window::save_window_state(&ui, &data);
                let _ = ui.window().hide();
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let data = data.clone();
        ui.on_request_minimize(move || {
            if let Some(ui) = ui_weak.upgrade() {
                window::save_window_state(&ui, &data);
                let _ = ui.window().hide();
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let data = data.clone();
        ui.on_toggle_pin(move |pinned| {
            if let Some(ui) = ui_weak.upgrade() {
                window::set_pinned(&ui, &data, pinned);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let data = data.clone();
        ui.on_toggle_click_through(move |enabled| {
            if let Some(ui) = ui_weak.upgrade() {
                window::set_click_through(&ui, &data, enabled);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_move_window(move |offset_x, offset_y| {
            if let Some(ui) = ui_weak.upgrade() {
                window::move_window(&ui, offset_x, offset_y);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_resize_window(move |dx, dy| {
            if let Some(ui) = ui_weak.upgrade() {
                window::resize_window(&ui, &data, dx, dy);
            }
        });
    }
}

/// 解析闹钟输入，统一限制年月日时分范围。
fn parse_alarm_datetime(
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
) -> Option<(i32, u32, u32, u32, u32)> {
    if year < 2000 {
        return None;
    }
    let month = u32::try_from(month).ok()?;
    let day = u32::try_from(day).ok()?;
    let hour = u32::try_from(hour).ok()?;
    let minute = u32::try_from(minute).ok()?;

    if !(1..=12).contains(&month) || hour >= 24 || minute >= 60 {
        return None;
    }

    if NaiveDate::from_ymd_opt(year, month, day).is_some() {
        Some((year, month, day, hour, minute))
    } else {
        None
    }
}

fn parse_alarm_datetime_str(
    year: &str,
    month: &str,
    day: &str,
    hour: &str,
    minute: &str,
) -> Option<(i32, u32, u32, u32, u32)> {
    let year = year.trim().parse::<i32>().ok()?;
    let month = month.trim().parse::<i32>().ok()?;
    let day = day.trim().parse::<i32>().ok()?;
    let hour = hour.trim().parse::<i32>().ok()?;
    let minute = minute.trim().parse::<i32>().ok()?;
    parse_alarm_datetime(year, month, day, hour, minute)
}

fn adjust_editor_field(ui: &AppWindow, field: &str, delta: i32, min: i32, max: i32) {
    let current = match field {
        "year" => ui.get_alarm_editor_year().to_string(),
        "month" => ui.get_alarm_editor_month().to_string(),
        "day" => ui.get_alarm_editor_day().to_string(),
        "hour" => ui.get_alarm_editor_hour().to_string(),
        "minute" => ui.get_alarm_editor_minute().to_string(),
        _ => return,
    };

    let parsed = current.trim().parse::<i32>().unwrap_or(min);
    let next = (parsed + delta).clamp(min, max).to_string();
    let next = SharedString::from(next);

    match field {
        "year" => ui.set_alarm_editor_year(next),
        "month" => ui.set_alarm_editor_month(next),
        "day" => ui.set_alarm_editor_day(next),
        "hour" => ui.set_alarm_editor_hour(next),
        "minute" => ui.set_alarm_editor_minute(next),
        _ => {}
    }
}
