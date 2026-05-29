//! 系统托盘管理器
//! 使用 Windows API 直接实现：隐藏窗口 + 托盘图标 + 右键菜单
//! 托盘菜单：退出应用
//!
//! 事件流程：
//!   右键托盘 → WM_TRAY_ICON → show_tray_menu(TrackPopupMenu)
//!   点击"退出应用" → WM_COMMAND → mpsc 发送 TrayEvent::Quit
//!   timers.rs 定时器轮询 → save + exit

use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::Graphics::Gdi::*;
use std::sync::mpsc;

const WM_TRAY_ICON: u32 = WM_USER + 1;
const TRAY_UID: u32 = 1001;
const CMD_QUIT: usize = 2001;

/// 托盘事件
#[derive(Debug, Clone, Copy)]
pub enum TrayEvent {
    Quit,
    ShowWindow,
}

/// 系统托盘管理器（持有隐藏窗口和图标句柄）
pub struct TrayManager {
    _hwnd: HWND,
    _hicon: HICON,
}

impl TrayManager {
    pub fn create() -> Result<(Self, mpsc::Receiver<TrayEvent>), Box<dyn std::error::Error>> {
        let hicon = make_tray_hicon()?;

        let (tx, rx) = mpsc::channel();

        // 注册窗口类
        let class_name = encode_wide("StickyNoteTrayClass");
        let hinstance = unsafe { GetModuleHandleW(None)? };

        let wnd_class = WNDCLASSW {
            lpfnWndProc: Some(tray_wnd_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            hInstance: hinstance.into(),
            ..unsafe { std::mem::zeroed() }
        };

        if unsafe { RegisterClassW(&wnd_class) } == 0 {
            return Err("RegisterClassW failed".into());
        }

        // 把 tx 装箱，通过 CREATESTRUCT 传入 WndProc
        let tx_box = Box::new(tx);
        let tx_ptr = Box::into_raw(tx_box);

        let create_result = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                PCWSTR(class_name.as_ptr()),
                windows::core::w!(""),
                WS_OVERLAPPED,
                0, 0, 0, 0,
                None, None, hinstance,
                Some(tx_ptr as *const _ as _),
            )
        };

        let hwnd = match create_result {
            Ok(h) if !h.0.is_null() => h,
            _ => {
                unsafe { drop(Box::from_raw(tx_ptr)); }
                return Err("CreateWindowExW failed".into());
            }
        };

        add_tray_icon(hwnd, hicon)?;

        Ok((Self { _hwnd: hwnd, _hicon: hicon }, rx))
    }
}

impl Drop for TrayManager {
    fn drop(&mut self) {
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = self._hwnd;
            nid.uID = TRAY_UID;
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            let _ = DestroyWindow(self._hwnd);
            let _ = DestroyIcon(self._hicon);
        }
    }
}

// ---- 窗口过程 ----

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            LRESULT(0)
        }

        WM_DESTROY => {
            let tx_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut mpsc::Sender<TrayEvent>;
            if !tx_ptr.is_null() {
                drop(Box::from_raw(tx_ptr));
            }
            PostQuitMessage(0);
            LRESULT(0)
        }

        WM_TRAY_ICON => {
            let ev = lparam.0 as u32 & 0xFFFF;



            if ev == WM_RBUTTONUP {
                show_tray_menu(hwnd);
            } else if ev == WM_LBUTTONDBLCLK {
                let tx_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut mpsc::Sender<TrayEvent>;
                if !tx_ptr.is_null() {
                    let _ = (*tx_ptr).send(TrayEvent::ShowWindow);
                }
            }
            LRESULT(0)
        }

        WM_COMMAND => {
            if (wparam.0 as usize & 0xFFFF) == CMD_QUIT {
                let tx_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut mpsc::Sender<TrayEvent>;
                if !tx_ptr.is_null() {
                    let _ = (*tx_ptr).send(TrayEvent::Quit);
                }
            }
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ---- 托盘图标 ----

fn add_tray_icon(hwnd: HWND, hicon: HICON) -> Result<(), Box<dyn std::error::Error>> {
    let tip = encode_wide("便签 - 桌面助手");
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = TRAY_UID;
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY_ICON;
    nid.hIcon = hicon;
    for (i, &ch) in tip.iter().enumerate().take(127) {
        nid.szTip[i] = ch;
    }

    if unsafe { Shell_NotifyIconW(NIM_ADD, &nid) }.as_bool() {
        Ok(())
    } else {
        Err("Shell_NotifyIconW NIM_ADD failed".into())
    }
}

unsafe fn show_tray_menu(hwnd: HWND) {
    let hmenu = CreatePopupMenu().unwrap_or_default();
    let quit_text = encode_wide("退出应用");
    let _ = AppendMenuW(hmenu, MF_STRING, CMD_QUIT, PCWSTR(quit_text.as_ptr()));

    let mut pt = POINT { x: 0, y: 0 };
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(hwnd);

    let _ = TrackPopupMenu(
        hmenu,
        TPM_BOTTOMALIGN | TPM_LEFTALIGN,
        pt.x,
        pt.y,
        0,
        hwnd,
        None,
    );

    let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(hmenu);
}

// ---- 辅助 ----

fn encode_wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

// ---- 便签形状托盘图标 (32x32) → HICON ----

fn make_tray_hicon() -> Result<HICON, Box<dyn std::error::Error>> {
    let w = 32i32;
    let h = 32i32;
    let margin = 4;
    let r = 5;
    let fold_sz = 8;
    let body_w = w - margin * 2;
    let body_h = h - margin * 2;

    let paper_body   = [228u8, 226, 218, 255];
    let paper_fold   = [208u8, 205, 195, 255];
    let paper_crease = [188u8, 184, 174, 255];

    // 直接在内存中生成 32-bit BGRA 像素数据
    let mut pixels: Vec<u32> = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let lx = x - margin;
            let ly = y - margin;
            let in_body = lx >= 0 && lx < body_w && ly >= 0 && ly < body_h
                && inside_rounded_rect32(lx, ly, body_w, body_h, r);

            let (r8, g8, b8, a8) = if !in_body {
                (45u8, 45u8, 45u8, 255u8)
            } else {
                let fx = lx;
                let in_fold = inside_fold32(fx, ly, body_w, fold_sz);
                let in_crease = {
                    let diag = fx - (body_w - fold_sz) + ly;
                    diag >= fold_sz - 1 && diag <= fold_sz + 1
                        && fx >= body_w - fold_sz && ly <= fold_sz
                };

                if in_crease {
                    (paper_crease[0], paper_crease[1], paper_crease[2], 255)
                } else if in_fold {
                    let t = ((fx - (body_w - fold_sz)) as f32 + ly as f32)
                        / (fold_sz as f32 * 2.0);
                    let t = t.clamp(0.0, 1.0);
                    (
                        lerp(paper_fold[0], paper_body[0], 1.0 - t),
                        lerp(paper_fold[1], paper_body[1], 1.0 - t),
                        lerp(paper_fold[2], paper_body[2], 1.0 - t),
                        255,
                    )
                } else {
                    (paper_body[0], paper_body[1], paper_body[2], 255)
                }
            };

            // BGRA 格式 (Windows 32-bit bitmap)
            pixels.push(
                (a8 as u32) << 24 | (r8 as u32) << 16 | (g8 as u32) << 8 | b8 as u32,
            );
        }
    }

    // 用 CreateDIBSection 创建 HBITMAP
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = w;
    bmi.bmiHeader.biHeight = -h; // 负数 = top-down DIB
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB.0 as u32;

    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let hdc_screen = unsafe { GetDC(None) };
    let create_result = unsafe {
        CreateDIBSection(
            hdc_screen,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        )
    };
    unsafe { ReleaseDC(None, hdc_screen) };

    let hbmp = match create_result {
        Ok(h) if !h.0.is_null() && !bits.is_null() => h,
        _ => return Err("CreateDIBSection failed".into()),
    };

    // 拷贝像素
    let pixel_count = (w * h) as usize;
    let dst = unsafe { std::slice::from_raw_parts_mut(bits as *mut u32, pixel_count) };
    dst.copy_from_slice(&pixels);

    // HBITMAP → HICON
    let icon_info = ICONINFO {
        fIcon: BOOL(1),
        hbmMask: hbmp,
        hbmColor: hbmp,
        ..unsafe { std::mem::zeroed() }
    };

    Ok(unsafe { CreateIconIndirect(&icon_info) }?)
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 * t + b as f32 * (1.0 - t)) as u8
}

fn inside_rounded_rect32(x: i32, y: i32, w: i32, h: i32, r: i32) -> bool {
    let fx = x as f32;
    let fy = y as f32;
    let fw = w as f32 - 1.0;
    let fh = h as f32 - 1.0;
    let fr = r as f32;

    if fx >= fr && fx <= fw - fr && fy >= fr && fy <= fh - fr { return true; }
    if fy >= fr && fy <= fh - fr { return true; }
    if fx >= fr && fx <= fw - fr { return true; }
    let corners = [(fr, fr), (fw - fr, fr), (fr, fh - fr), (fw - fr, fh - fr)];
    for &(cx, cy) in &corners {
        let dx = fx - cx;
        let dy = fy - cy;
        if dx * dx + dy * dy <= fr * fr { return true; }
    }
    false
}

fn inside_fold32(x: i32, y: i32, w: i32, fold_sz: i32) -> bool {
    x >= w - fold_sz && y <= fold_sz && (x - (w - fold_sz)) + y <= fold_sz
}
