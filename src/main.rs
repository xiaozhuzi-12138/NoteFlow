#![windows_subsystem = "windows"]

//! 程序入口。
//!
//! 大部分业务代码都拆到了独立模块中，这里只保留 Slint 生成代码的引入
//! 和应用启动调用，方便以后继续扩展桌面端能力。

mod app;
mod alarm_alert;
mod alarm_audio;
mod handlers;
mod storage;
mod timers;
mod tray;
mod ui_sync;
mod window;

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::run()
}
