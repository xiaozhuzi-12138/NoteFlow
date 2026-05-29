//! 闹钟铃声播放。
//!
//! 这里用 ffmpeg 工具链里的 ffplay 播放音频。播放进程在后台循环运行，
//! 提醒弹窗关闭后由调用方停止。

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

pub struct AlarmAudio {
    child: Option<Child>,
}

impl AlarmAudio {
    pub fn start_looping() -> Self {
        let child = find_audio_file()
            .and_then(|audio_path| spawn_ffplay_loop(&audio_path).ok());

        Self { child }
    }
}

impl Drop for AlarmAudio {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn find_audio_file() -> Option<PathBuf> {
    let file_name = "test.mp3";

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let candidate = exe_dir.join(file_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    std::env::current_dir()
        .ok()
        .map(|dir| dir.join(file_name))
        .filter(|path| path.is_file())
}

fn spawn_ffplay_loop(audio_path: &PathBuf) -> std::io::Result<Child> {
    let mut command = Command::new("ffplay");
    command
        .arg("-nodisp")
        .arg("-autoexit")
        .arg("-loop")
        .arg("0")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg(audio_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command.spawn()
}
