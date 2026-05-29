//! 应用启动编排层。
//!
//! 这个模块负责把 UI、数据、托盘、后台任务组装在一起。具体业务行为
//! 分散在 handlers/timers/window 等模块里，避免入口函数继续膨胀。

use crate::{handlers, storage, timers, tray, ui_sync, window, AppWindow};
use slint::{ComponentHandle, Timer};
use std::cell::RefCell;
use std::rc::Rc;

/// 全局共享的应用数据句柄。
///
/// Slint 回调都运行在 UI 线程中，当前用 Rc<RefCell<_>> 足够简单直接。
pub type SharedAppData = Rc<RefCell<storage::AppData>>;

/// 启动桌面便签应用。
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let app_data: SharedAppData = Rc::new(RefCell::new(storage::AppData::load()));
    let ui = AppWindow::new()?;

    // Timer 需要持有句柄才会持续工作，集中存放可以避免后台任务被提前释放。
    let mut background_timers: Vec<Timer> = Vec::new();

    ui_sync::sync_all(&ui, &app_data.borrow());
    handlers::bind_all(&ui, app_data.clone());

    let (_tray_manager, tray_rx) = match tray::TrayManager::create() {
        Ok((manager, rx)) => (Some(manager), Some(rx)),
        Err(e) => {
            eprintln!("系统托盘创建失败: {}", e);
            (None, None)
        }
    };

    timers::start_alarm_checker(&ui, app_data.clone(), &mut background_timers);
    timers::start_tray_listener(&ui, app_data.clone(), tray_rx, &mut background_timers);
    timers::start_auto_save(&ui, app_data.clone(), &mut background_timers);

    window::restore_window_state(&ui, &app_data.borrow());

    ui.run()?;
    Ok(())
}
