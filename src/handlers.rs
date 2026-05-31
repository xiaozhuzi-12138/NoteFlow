//! UI 回调绑定。
//!
//! Slint 通过 callback 把用户操作交给 Rust。这里统一注册所有回调，
//! 每个小函数负责一个功能区，避免启动流程里堆满闭包。

use crate::{app::SharedAppData, ui_sync, window, AppWindow};
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
        ui.on_save_alarm(move |id, hour_str, minute_str, memo| {
            let id = id.to_string();
            let memo = memo.trim().to_string();

            if let Some((hour, minute)) = parse_time(&hour_str, &minute_str) {
                let mut data = data.borrow_mut();
                if id.is_empty() {
                    data.add_alarm(hour, minute, memo);
                } else {
                    data.update_alarm(&id, hour, minute, memo);
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
        ui.on_toggle_alarm(move |id| {
            let mut data = data.borrow_mut();
            data.toggle_alarm(&id.to_string());
            if let Some(ui) = ui_weak.upgrade() {
                ui_sync::sync_alarms(&ui, &data);
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
        ui.on_toggle_pin(move |pinned| {
            if let Some(ui) = ui_weak.upgrade() {
                window::set_topmost(window::APP_TITLE, pinned);
                ui.set_is_pinned(pinned);
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

/// 解析闹钟输入，统一限制小时和分钟范围。
fn parse_time(hour_str: &str, minute_str: &str) -> Option<(u32, u32)> {
    let hour = hour_str.trim().parse::<u32>().ok()?;
    let minute = minute_str.trim().parse::<u32>().ok()?;
    if hour < 24 && minute < 60 {
        Some((hour, minute))
    } else {
        None
    }
}
