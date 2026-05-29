//! Slint UI 与 Rust 数据模型之间的映射。
//!
//! UI 层使用多个数组属性展示列表，这里把 AppData 转换成 Slint 可绑定的
//! ModelRc，业务模块只需要在数据变化后调用同步函数。

use crate::{storage, AppWindow};
use slint::{ModelRc, SharedString, VecModel};

/// 同步所有可视数据到界面。
pub fn sync_all(ui: &AppWindow, data: &storage::AppData) {
    sync_notes(ui, data);
    sync_alarms(ui, data);
}

/// 同步便签列表。
pub fn sync_notes(ui: &AppWindow, data: &storage::AppData) {
    let ids: Vec<SharedString> = data.notes.iter().map(|n| n.id.clone().into()).collect();
    let contents: Vec<SharedString> = data
        .notes
        .iter()
        .map(|n| n.content.clone().into())
        .collect();

    ui.set_note_ids(ModelRc::new(VecModel::from(ids)));
    ui.set_note_contents(ModelRc::new(VecModel::from(contents)));
}

/// 同步闹钟列表。
pub fn sync_alarms(ui: &AppWindow, data: &storage::AppData) {
    let ids: Vec<SharedString> = data.alarms.iter().map(|a| a.id.clone().into()).collect();
    let times: Vec<SharedString> = data
        .alarms
        .iter()
        .map(|a| format!("{:02}:{:02}", a.hour, a.minute).into())
        .collect();
    let memos: Vec<SharedString> = data
        .alarms
        .iter()
        .map(|a| a.memo.clone().into())
        .collect();
    let enables: Vec<bool> = data.alarms.iter().map(|a| a.enabled).collect();

    ui.set_alarm_ids(ModelRc::new(VecModel::from(ids)));
    ui.set_alarm_times(ModelRc::new(VecModel::from(times)));
    ui.set_alarm_memos(ModelRc::new(VecModel::from(memos)));
    ui.set_alarm_enables(ModelRc::new(VecModel::from(enables)));
}
