//! Background timers.
//!
//! Keep `Timer` handles alive in `app.rs`; otherwise the timers stop when
//! dropped.

use crate::{alarm_alert, app::SharedAppData, tray, window, AppWindow};
use chrono::{Datelike, Timelike};
use slint::{ComponentHandle, Timer, TimerMode};
use std::sync::mpsc;

/// Check enabled alarms every 30 seconds.
pub fn start_alarm_checker(ui: &AppWindow, data: SharedAppData, timers: &mut Vec<Timer>) {
    let ui_weak = ui.as_weak();
    let mut last_triggered_minute: Option<(i32, u32, u32, u32)> = None;
    let alarm_timer = Timer::default();
    alarm_timer.start(
        TimerMode::Repeated,
        std::time::Duration::from_secs(30),
        move || {
            let now = chrono::Local::now();
            let current_key = (now.year(), now.ordinal(), now.hour(), now.minute());
            if last_triggered_minute == Some(current_key) {
                return;
            }

            let current_hour = now.hour();
            let current_minute = now.minute();

            let data_ref = data.borrow();
            let mut triggered = false;
            for alarm in &data_ref.alarms {
                if alarm.enabled && alarm.hour == current_hour && alarm.minute == current_minute {
                    triggered = true;
                    alarm_alert::show_alarm_alert(
                        alarm.hour,
                        alarm.minute,
                        &alarm.memo,
                        &alarm.id,
                        data.clone(),
                        ui_weak.clone(),
                    );
                }
            }

            if triggered {
                last_triggered_minute = Some(current_key);
            }
        },
    );
    timers.push(alarm_timer);
}

/// Listen to tray events: quit app / show window.
pub fn start_tray_listener(
    ui: &AppWindow,
    data: SharedAppData,
    tray_rx: Option<mpsc::Receiver<tray::TrayEvent>>,
    timers: &mut Vec<Timer>,
) {
    let Some(rx) = tray_rx else {
        return;
    };

    let ui_weak = ui.as_weak();
    let tray_timer = Timer::default();
    tray_timer.start(
        TimerMode::Repeated,
        std::time::Duration::from_millis(200),
        move || {
            if let Ok(ev) = rx.try_recv() {
                match ev {
                    tray::TrayEvent::Quit => {
                        if let Some(ui) = ui_weak.upgrade() {
                            window::save_window_state(&ui, &data);
                        }
                        std::process::exit(0);
                    }
                    tray::TrayEvent::ShowWindow => {
                        if let Some(ui) = ui_weak.upgrade() {
                            window::show_and_focus(&ui);
                        }
                    }
                }
            }
        },
    );
    timers.push(tray_timer);
}

/// Save window state periodically.
pub fn start_auto_save(ui: &AppWindow, data: SharedAppData, timers: &mut Vec<Timer>) {
    let ui_weak = ui.as_weak();
    let save_timer = Timer::default();
    save_timer.start(
        TimerMode::Repeated,
        std::time::Duration::from_secs(3),
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                window::save_window_state(&ui, &data);
            }
        },
    );
    timers.push(save_timer);
}
