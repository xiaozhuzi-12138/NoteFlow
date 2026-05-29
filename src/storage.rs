//! 数据模型与持久化存储。
//!
//! 当前应用的数据量很小，使用 JSON 文件存储更直观，也方便以后排查用户
//! 本地数据问题。所有会修改数据的方法都会主动保存一次。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ==================== 数据模型 ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// 便签唯一 ID，用于编辑和删除。
    pub id: String,
    /// 便签正文。
    pub content: String,
    /// 创建时间，直接存储格式化后的展示文本。
    pub created_at: String,
}

/// 闹钟数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alarm {
    /// 闹钟唯一 ID。
    pub id: String,
    /// 24 小时制小时。
    pub hour: u32,
    /// 分钟。
    pub minute: u32,
    /// 闹钟提醒内容。
    pub memo: String,
    /// 是否启用。
    pub enabled: bool,
}

/// 应用完整持久化状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppData {
    /// 所有便签。
    pub notes: Vec<Note>,
    /// 所有闹钟。
    pub alarms: Vec<Alarm>,
    /// 上次关闭时的窗口位置 X（-1 = 无记录，下次自动右上角）
    pub window_x: i32,
    /// 上次关闭时的窗口位置 Y
    pub window_y: i32,
    /// 上次关闭时的窗口宽度（-1 = 默认 320）
    pub window_w: i32,
    /// 上次关闭时的窗口高度（-1 = 默认 480）
    pub window_h: i32,
    /// 上次关闭时是否置顶
    pub is_pinned: bool,
}

// ==================== 存储路径 ====================

impl AppData {
    fn data_dir() -> std::path::PathBuf {
        // Windows: %APPDATA%/便签
        // 其他: ~/.sticky-note/
        #[cfg(target_os = "windows")]
        {
            if let Ok(appdata) = std::env::var("APPDATA") {
                let dir = std::path::PathBuf::from(appdata).join("便签");
                std::fs::create_dir_all(&dir).ok();
                return dir;
            }
        }
        // 回退
        let dir = dirs_fallback();
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    fn data_file() -> std::path::PathBuf {
        Self::data_dir().join("data.json")
    }
}

fn dirs_fallback() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        std::path::PathBuf::from(home).join(".sticky-note")
    } else {
        std::path::PathBuf::from(".")
    }
}

// ==================== 加载/保存 ====================

impl AppData {
    /// 从文件加载数据，文件不存在时返回空数据
    pub fn load() -> Self {
        let path = Self::data_file();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    serde_json::from_str(&content).unwrap_or_default()
                }
                Err(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }

    /// 保存数据到文件
    pub fn save(&self) {
        let path = Self::data_file();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            std::fs::write(&path, json).ok();
        }
    }
}

impl Default for AppData {
    fn default() -> Self {
        Self {
            notes: Vec::new(),
            alarms: Vec::new(),
            window_x: -1,
            window_y: -1,
            window_w: -1,
            window_h: -1,
            is_pinned: true,
        }
    }
}

// ==================== 便签操作 ====================

impl AppData {
    /// 新增便签并返回生成的 ID。
    pub fn add_note(&mut self, content: String) -> String {
        let now = chrono::Local::now();
        let note = Note {
            id: Uuid::new_v4().to_string(),
            content,
            created_at: now.format("%Y-%m-%d %H:%M").to_string(),
        };
        let id = note.id.clone();
        self.notes.push(note);
        self.save();
        id
    }

    /// 更新指定便签内容，返回是否找到目标。
    pub fn update_note(&mut self, id: &str, content: String) -> bool {
        if let Some(note) = self.notes.iter_mut().find(|n| n.id == id) {
            note.content = content;
            self.save();
            true
        } else {
            false
        }
    }

    /// 删除指定便签，返回是否实际删除。
    pub fn delete_note(&mut self, id: &str) -> bool {
        let len_before = self.notes.len();
        self.notes.retain(|n| n.id != id);
        if self.notes.len() != len_before {
            self.save();
            true
        } else {
            false
        }
    }

    /// 根据 ID 查找便签。
    pub fn get_note(&self, id: &str) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }
}

// ==================== 闹钟操作 ====================

impl AppData {
    /// 新增闹钟并返回生成的 ID。
    pub fn add_alarm(&mut self, hour: u32, minute: u32, memo: String) -> String {
        let alarm = Alarm {
            id: Uuid::new_v4().to_string(),
            hour,
            minute,
            memo,
            enabled: true,
        };
        let id = alarm.id.clone();
        self.alarms.push(alarm);
        self.save();
        id
    }

    /// 更新指定闹钟，返回是否找到目标。
    pub fn update_alarm(&mut self, id: &str, hour: u32, minute: u32, memo: String) -> bool {
        if let Some(alarm) = self.alarms.iter_mut().find(|a| a.id == id) {
            alarm.hour = hour;
            alarm.minute = minute;
            alarm.memo = memo;
            self.save();
            true
        } else {
            false
        }
    }

    /// 删除指定闹钟，返回是否实际删除。
    pub fn delete_alarm(&mut self, id: &str) -> bool {
        let len_before = self.alarms.len();
        self.alarms.retain(|a| a.id != id);
        if self.alarms.len() != len_before {
            self.save();
            true
        } else {
            false
        }
    }

    /// 切换闹钟启用状态，返回切换后的状态。
    pub fn toggle_alarm(&mut self, id: &str) -> bool {
        if let Some(idx) = self.alarms.iter().position(|a| a.id == id) {
            self.alarms[idx].enabled = !self.alarms[idx].enabled;
            let result = self.alarms[idx].enabled;
            self.save();
            result
        } else {
            false
        }
    }
}
