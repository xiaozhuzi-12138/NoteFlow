//! Windows system tray integration.
//!
//! This module owns the hidden message window, tray icon, and tray menu.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
    Arc,
};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_CONTROL};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_TRAY_ICON: u32 = WM_USER + 1;
const HOTKEY_ID_TOGGLE_CLICK_THROUGH: i32 = 3001;
const HOTKEY_ID_TOGGLE_PINNED: i32 = 3002;
const TRAY_UID: u32 = 1001;
const CMD_ENABLE_CLICK_THROUGH: usize = 2001;
const CMD_DISABLE_CLICK_THROUGH: usize = 2002;
const CMD_QUIT: usize = 2003;

#[derive(Debug, Clone, Copy)]
pub enum TrayEvent {
    Quit,
    ShowWindow,
    SetClickThrough(bool),
    ToggleClickThrough,
    TogglePinned,
}

pub struct TrayManager {
    _hwnd: HWND,
    _hicon: HICON,
    click_through_enabled: Arc<AtomicBool>,
}

impl TrayManager {
    pub fn create(
        initial_click_through: bool,
    ) -> Result<(Self, mpsc::Receiver<TrayEvent>), Box<dyn std::error::Error>> {
        let hicon = make_tray_hicon()?;
        let (tx, rx) = mpsc::channel();
        let click_through_enabled = Arc::new(AtomicBool::new(initial_click_through));

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

        let tray_context = Box::new(TrayContext {
            tx,
            click_through_enabled: click_through_enabled.clone(),
        });
        let tray_context_ptr = Box::into_raw(tray_context);

        let create_result = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                PCWSTR(class_name.as_ptr()),
                windows::core::w!(""),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                hinstance,
                Some(tray_context_ptr as *const _ as _),
            )
        };

        let hwnd = match create_result {
            Ok(h) if !h.0.is_null() => h,
            _ => {
                unsafe { drop(Box::from_raw(tray_context_ptr)) };
                return Err("CreateWindowExW failed".into());
            }
        };

        add_tray_icon(hwnd, hicon)?;
        register_hotkeys(hwnd)?;

        Ok((
            Self {
                _hwnd: hwnd,
                _hicon: hicon,
                click_through_enabled,
            },
            rx,
        ))
    }

    pub fn clone_state_handle(&self) -> Arc<AtomicBool> {
        self.click_through_enabled.clone()
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

struct TrayContext {
    tx: mpsc::Sender<TrayEvent>,
    click_through_enabled: Arc<AtomicBool>,
}

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
            let context_ptr = tray_context_ptr(hwnd);
            if !context_ptr.is_null() {
                drop(Box::from_raw(context_ptr));
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_TRAY_ICON => {
            let ev = lparam.0 as u32 & 0xFFFF;
            if ev == WM_RBUTTONUP {
                show_tray_menu(hwnd);
            } else if ev == WM_LBUTTONDBLCLK {
                send_tray_event(hwnd, TrayEvent::ShowWindow);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let command_id = wparam.0 as usize & 0xFFFF;
            match command_id {
                CMD_ENABLE_CLICK_THROUGH => send_tray_event(hwnd, TrayEvent::SetClickThrough(true)),
                CMD_DISABLE_CLICK_THROUGH => {
                    send_tray_event(hwnd, TrayEvent::SetClickThrough(false))
                }
                CMD_QUIT => send_tray_event(hwnd, TrayEvent::Quit),
                _ => {}
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            if wparam.0 as i32 == HOTKEY_ID_TOGGLE_CLICK_THROUGH {
                send_tray_event(hwnd, TrayEvent::ToggleClickThrough);
            } else if wparam.0 as i32 == HOTKEY_ID_TOGGLE_PINNED {
                send_tray_event(hwnd, TrayEvent::TogglePinned);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn register_hotkeys(hwnd: HWND) -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        RegisterHotKey(
            hwnd,
            HOTKEY_ID_TOGGLE_CLICK_THROUGH,
            MOD_CONTROL,
            u32::from(b'K'),
        )?;
        RegisterHotKey(
            hwnd,
            HOTKEY_ID_TOGGLE_PINNED,
            MOD_CONTROL,
            u32::from(b'L'),
        )?;
    }
    Ok(())
}

unsafe fn tray_context_ptr(hwnd: HWND) -> *mut TrayContext {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayContext
}

unsafe fn send_tray_event(hwnd: HWND, event: TrayEvent) {
    let context_ptr = tray_context_ptr(hwnd);
    if !context_ptr.is_null() {
        let _ = (*context_ptr).tx.send(event);
    }
}

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
    let context_ptr = tray_context_ptr(hwnd);
    let click_through_enabled = !context_ptr.is_null()
        && (*context_ptr)
            .click_through_enabled
            .load(Ordering::Relaxed);

    let enable_text = encode_wide("启动鼠标穿透");
    let disable_text = encode_wide("关闭鼠标穿透");
    let quit_text = encode_wide("退出应用");

    let enable_flags = if click_through_enabled {
        MF_STRING | MF_GRAYED
    } else {
        MF_STRING
    };
    let disable_flags = if click_through_enabled {
        MF_STRING
    } else {
        MF_STRING | MF_GRAYED
    };

    let _ = AppendMenuW(
        hmenu,
        enable_flags,
        CMD_ENABLE_CLICK_THROUGH,
        PCWSTR(enable_text.as_ptr()),
    );
    let _ = AppendMenuW(
        hmenu,
        disable_flags,
        CMD_DISABLE_CLICK_THROUGH,
        PCWSTR(disable_text.as_ptr()),
    );
    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
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

fn encode_wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn make_tray_hicon() -> Result<HICON, Box<dyn std::error::Error>> {
    if let Some(hicon) = try_load_embedded_app_icon() {
        return Ok(hicon);
    }

    if let Some(hicon) = try_load_icon_from_assets() {
        return Ok(hicon);
    }

    // Fall back to the old procedural icon so tray creation never blocks startup.
    make_fallback_tray_hicon()
}

fn try_load_embedded_app_icon() -> Option<HICON> {
    let hinstance: windows::Win32::Foundation::HINSTANCE =
        unsafe { GetModuleHandleW(None).ok()? }.into();
    let handle = unsafe {
        LoadImageW(
            hinstance,
            PCWSTR(1 as *const u16),
            IMAGE_ICON,
            32,
            32,
            LR_DEFAULTCOLOR,
        )
    }
    .ok()?;

    let hicon = HICON(handle.0);
    if hicon.0.is_null() {
        None
    } else {
        Some(hicon)
    }
}

fn try_load_icon_from_assets() -> Option<HICON> {
    let icon_path = std::env::current_dir().ok()?.join("assets").join("icon.ico");
    let icon_path_wide = encode_wide(icon_path.to_str()?);
    let handle = unsafe {
        LoadImageW(
            windows::Win32::Foundation::HINSTANCE::default(),
            PCWSTR(icon_path_wide.as_ptr()),
            IMAGE_ICON,
            32,
            32,
            LR_DEFAULTCOLOR | LR_LOADFROMFILE,
        )
    }
    .ok()?;

    let hicon = HICON(handle.0);
    if hicon.0.is_null() {
        None
    } else {
        Some(hicon)
    }
}

fn make_fallback_tray_hicon() -> Result<HICON, Box<dyn std::error::Error>> {
    let w = 32i32;
    let h = 32i32;
    let margin = 4;
    let r = 5;
    let fold_sz = 8;
    let body_w = w - margin * 2;
    let body_h = h - margin * 2;

    let paper_body = [228u8, 226, 218, 255];
    let paper_fold = [208u8, 205, 195, 255];
    let paper_crease = [188u8, 184, 174, 255];

    let mut pixels: Vec<u32> = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let lx = x - margin;
            let ly = y - margin;
            let in_body =
                lx >= 0 && lx < body_w && ly >= 0 && ly < body_h && inside_rounded_rect32(lx, ly, body_w, body_h, r);

            let (r8, g8, b8, a8) = if !in_body {
                (45u8, 45u8, 45u8, 255u8)
            } else {
                let fx = lx;
                let in_fold = inside_fold32(fx, ly, body_w, fold_sz);
                let in_crease = {
                    let diag = fx - (body_w - fold_sz) + ly;
                    diag >= fold_sz - 1
                        && diag <= fold_sz + 1
                        && fx >= body_w - fold_sz
                        && ly <= fold_sz
                };

                if in_crease {
                    (paper_crease[0], paper_crease[1], paper_crease[2], 255)
                } else if in_fold {
                    let t = ((fx - (body_w - fold_sz)) as f32 + ly as f32) / (fold_sz as f32 * 2.0);
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

            pixels.push((a8 as u32) << 24 | (r8 as u32) << 16 | (g8 as u32) << 8 | b8 as u32);
        }
    }

    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = w;
    bmi.bmiHeader.biHeight = -h;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB.0 as u32;

    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let hdc_screen = unsafe { GetDC(None) };
    let create_result = unsafe { CreateDIBSection(hdc_screen, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) };
    unsafe { ReleaseDC(None, hdc_screen) };

    let hbmp = match create_result {
        Ok(h) if !h.0.is_null() && !bits.is_null() => h,
        _ => return Err("CreateDIBSection failed".into()),
    };

    let pixel_count = (w * h) as usize;
    let dst = unsafe { std::slice::from_raw_parts_mut(bits as *mut u32, pixel_count) };
    dst.copy_from_slice(&pixels);

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

    if fx >= fr && fx <= fw - fr && fy >= fr && fy <= fh - fr {
        return true;
    }
    if fy >= fr && fy <= fh - fr {
        return true;
    }
    if fx >= fr && fx <= fw - fr {
        return true;
    }

    let corners = [(fr, fr), (fw - fr, fr), (fr, fh - fr), (fw - fr, fh - fr)];
    for &(cx, cy) in &corners {
        let dx = fx - cx;
        let dy = fy - cy;
        if dx * dx + dy * dy <= fr * fr {
            return true;
        }
    }

    false
}

fn inside_fold32(x: i32, y: i32, w: i32, fold_sz: i32) -> bool {
    x >= w - fold_sz && y <= fold_sz && (x - (w - fold_sz)) + y <= fold_sz
}
