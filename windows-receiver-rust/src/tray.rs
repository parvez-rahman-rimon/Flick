use crate::config::{get_config, get_lan_ip, is_autostart_enabled, set_autostart_enabled, set_clipboard_mode};
use crate::gui;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicPtr, Ordering};

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::HBRUSH;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext;
use windows_sys::Win32::UI::Shell::{
    ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP,
    NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW,
    PostMessageW, PostQuitMessage, RegisterClassExW, SetForegroundWindow,
    TrackPopupMenu, TranslateMessage, HICON, HMENU, MF_CHECKED, MF_DISABLED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, SW_SHOWNORMAL,
    TPM_BOTTOMALIGN, TPM_RIGHTBUTTON, WM_COMMAND, WM_LBUTTONDBLCLK,
    WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WM_USER, WNDCLASSEXW, WS_OVERLAPPED,
};

pub const WM_TRAY_CALLBACK: u32 = WM_USER + 1;
pub const WM_IPC_OPEN_DASHBOARD: u32 = WM_USER + 101;
pub const WM_IPC_OPEN_PAIR: u32 = WM_USER + 102;

const ID_TRAY_OPEN_DASHBOARD: usize = 1001;
const ID_TRAY_PAIR_QR: usize = 1002;
const ID_TRAY_IP_LABEL: usize = 1003;
const ID_TRAY_MODE_MULTI: usize = 1010;
const ID_TRAY_MODE_PATH: usize = 1011;
const ID_TRAY_MODE_IMAGE: usize = 1012;
const ID_TRAY_MODE_FILE: usize = 1013;
const ID_TRAY_OPEN_DOWNLOADS: usize = 1020;
const ID_TRAY_CHANGE_DOWNLOADS: usize = 1021;
const ID_TRAY_TOGGLE_AUTOSTART: usize = 1022;
const ID_TRAY_QUIT: usize = 1030;

static TRAY_HWND: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

pub fn get_tray_hwnd() -> HWND {
    TRAY_HWND.load(Ordering::SeqCst) as HWND
}

pub fn show_tray_notification(title: &str, message: &str) {
    let hwnd = get_tray_hwnd();
    if hwnd.is_null() {
        return;
    }

    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = NIIF_INFO | 0x10; // 0x10 is NIIF_NOSOUND (silent toast)

    let info_wide: Vec<u16> = OsStr::new(message).encode_wide().chain(Some(0)).collect();
    let title_wide: Vec<u16> = OsStr::new(title).encode_wide().chain(Some(0)).collect();

    let copy_len_info = info_wide.len().min(nid.szInfo.len() - 1);
    nid.szInfo[..copy_len_info].copy_from_slice(&info_wide[..copy_len_info]);

    let copy_len_title = title_wide.len().min(nid.szInfoTitle.len() - 1);
    nid.szInfoTitle[..copy_len_title].copy_from_slice(&title_wide[..copy_len_title]);

    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

pub fn run_tray_message_loop(start_minimized: bool, start_pair: bool) {
    unsafe {
        // Enable Per-Monitor v2 DPI awareness
        SetProcessDpiAwarenessContext(-4 as isize as _);

        let hinstance = GetModuleHandleW(std::ptr::null());
        let class_name: Vec<u16> = OsStr::new("FlickTrayWindowClass")
            .encode_wide()
            .chain(Some(0))
            .collect();

        // 1. Load application icon from embedded PE resource (or standard app icon)
        let icon: HICON = LoadIconW(hinstance, 1 as *const u16);
        let app_icon = if !icon.is_null() {
            icon
        } else {
            LoadIconW(0 as HINSTANCE, 32512 as *const u16) // IDI_APPLICATION
        };

        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(tray_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: app_icon,
            hCursor: 0 as _,
            hbrBackground: 0 as HBRUSH,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: app_icon,
        };

        RegisterClassExW(&wnd_class);

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            0 as HWND,
            0 as HMENU,
            hinstance,
            std::ptr::null(),
        );

        if hwnd.is_null() {
            eprintln!("[Tray] Failed to create tray message window");
            return;
        }

        TRAY_HWND.store(hwnd as _, Ordering::SeqCst);

        // 2. Add System Tray Icon
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_CALLBACK;
        nid.hIcon = app_icon;

        let tip_wide: Vec<u16> = OsStr::new("Flick Receiver")
            .encode_wide()
            .chain(Some(0))
            .collect();
        let copy_tip_len = tip_wide.len().min(nid.szTip.len() - 1);
        nid.szTip[..copy_tip_len].copy_from_slice(&tip_wide[..copy_tip_len]);

        Shell_NotifyIconW(NIM_ADD, &nid);

        // Initial boot welcome toast
        show_tray_notification(
            "Flick Active",
            "Flick Windows Receiver active. Double-click tray icon to open Dashboard.",
        );

        if !start_minimized {
            gui::open_or_show_dashboard(start_pair);
        }

        // 4. Standard Win32 Message Pump
        let mut msg = std::mem::zeroed();
        while GetMessageW(&mut msg, 0 as HWND, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Clean up tray icon on exit
        Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY_CALLBACK => {
            let event = lparam as u32;
            if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
                gui::open_or_show_dashboard(false);
            } else if event == WM_RBUTTONUP {
                show_tray_menu(hwnd);
            }
            0
        }

        WM_IPC_OPEN_DASHBOARD => {
            gui::open_or_show_dashboard(false);
            0
        }

        WM_IPC_OPEN_PAIR => {
            gui::open_or_show_dashboard(true);
            0
        }

        WM_COMMAND => {
            let cmd = (wparam & 0xFFFF) as usize;
            match cmd {
                ID_TRAY_OPEN_DASHBOARD => {
                    gui::open_or_show_dashboard(false);
                }
                ID_TRAY_PAIR_QR => {
                    gui::open_or_show_dashboard(true);
                }
                ID_TRAY_MODE_MULTI => {
                    set_clipboard_mode("SMART_MULTI");
                }
                ID_TRAY_MODE_PATH => {
                    set_clipboard_mode("FORCE_PATH");
                }
                ID_TRAY_MODE_IMAGE => {
                    set_clipboard_mode("FORCE_IMAGE");
                }
                ID_TRAY_MODE_FILE => {
                    set_clipboard_mode("FORCE_FILE");
                }
                ID_TRAY_OPEN_DOWNLOADS => {
                    let cfg = get_config();
                    let path_wide: Vec<u16> = OsStr::new(&cfg.save_directory)
                        .encode_wide()
                        .chain(Some(0))
                        .collect();
                    let op_wide: Vec<u16> = OsStr::new("open")
                        .encode_wide()
                        .chain(Some(0))
                        .collect();
                    ShellExecuteW(
                        0 as HWND,
                        op_wide.as_ptr(),
                        path_wide.as_ptr(),
                        std::ptr::null(),
                        std::ptr::null(),
                        SW_SHOWNORMAL,
                    );
                }
                ID_TRAY_CHANGE_DOWNLOADS => {
                    gui::launch_folder_picker();
                }
                ID_TRAY_TOGGLE_AUTOSTART => {
                    let current = is_autostart_enabled();
                    set_autostart_enabled(!current);
                    let dash = gui::get_dashboard_hwnd();
                    if !dash.is_null() {
                        windows_sys::Win32::Graphics::Gdi::InvalidateRect(dash, std::ptr::null(), 0);
                    }
                }
                ID_TRAY_QUIT => {
                    gui::close_dashboard_if_open();
                    PostQuitMessage(0);
                }
                _ => {}
            }
            0
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn show_tray_menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    let mode_menu = CreatePopupMenu();
    let cfg = get_config();
    let lan_ip = get_lan_ip();

    let open_dash_str: Vec<u16> = OsStr::new("⚡ Open Dashboard")
        .encode_wide()
        .chain(Some(0))
        .collect();
    AppendMenuW(menu, MF_STRING, ID_TRAY_OPEN_DASHBOARD, open_dash_str.as_ptr());

    let pair_qr_str: Vec<u16> = OsStr::new("Pair New Phone (Show QR)...")
        .encode_wide()
        .chain(Some(0))
        .collect();
    AppendMenuW(menu, MF_STRING, ID_TRAY_PAIR_QR, pair_qr_str.as_ptr());

    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

    let ip_label = format!("Flick Receiver • {}:{}", lan_ip, cfg.http_port);
    let ip_label_wide: Vec<u16> = OsStr::new(&ip_label)
        .encode_wide()
        .chain(Some(0))
        .collect();
    AppendMenuW(menu, MF_STRING | MF_GRAYED | MF_DISABLED, ID_TRAY_IP_LABEL, ip_label_wide.as_ptr());

    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

    // Clipboard Submenu
    let modes = [
        ("Smart Multi (File + Text + DIB)", "SMART_MULTI", ID_TRAY_MODE_MULTI),
        ("Force Path Only", "FORCE_PATH", ID_TRAY_MODE_PATH),
        ("Force Image Only", "FORCE_IMAGE", ID_TRAY_MODE_IMAGE),
        ("Force File Drop Only", "FORCE_FILE", ID_TRAY_MODE_FILE),
    ];

    for (label, mode_code, id) in modes {
        let is_active = cfg.clipboard_mode == mode_code;
        let flags = MF_STRING | if is_active { MF_CHECKED } else { MF_UNCHECKED };
        let label_wide: Vec<u16> = OsStr::new(label).encode_wide().chain(Some(0)).collect();
        AppendMenuW(mode_menu, flags, id, label_wide.as_ptr());
    }

    let clip_str: Vec<u16> = OsStr::new("Clipboard Mode")
        .encode_wide()
        .chain(Some(0))
        .collect();
    AppendMenuW(menu, MF_POPUP, mode_menu as usize, clip_str.as_ptr());

    let open_dl_str: Vec<u16> = OsStr::new("Open Saved Files Folder")
        .encode_wide()
        .chain(Some(0))
        .collect();
    AppendMenuW(menu, MF_STRING, ID_TRAY_OPEN_DOWNLOADS, open_dl_str.as_ptr());

    let change_dl_str: Vec<u16> = OsStr::new("Change Save Location...")
        .encode_wide()
        .chain(Some(0))
        .collect();
    AppendMenuW(menu, MF_STRING, ID_TRAY_CHANGE_DOWNLOADS, change_dl_str.as_ptr());

    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

    let autostart = is_autostart_enabled();
    let autostart_flags = MF_STRING | if autostart { MF_CHECKED } else { MF_UNCHECKED };
    let autostart_str: Vec<u16> = OsStr::new("Start with Windows")
        .encode_wide()
        .chain(Some(0))
        .collect();
    AppendMenuW(menu, autostart_flags, ID_TRAY_TOGGLE_AUTOSTART, autostart_str.as_ptr());

    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

    let quit_str: Vec<u16> = OsStr::new("Quit Flick")
        .encode_wide()
        .chain(Some(0))
        .collect();
    AppendMenuW(menu, MF_STRING, ID_TRAY_QUIT, quit_str.as_ptr());

    let mut cursor_pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut cursor_pt);

    SetForegroundWindow(hwnd);
    TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
        cursor_pt.x,
        cursor_pt.y,
        0,
        hwnd,
        std::ptr::null(),
    );
    PostMessageW(hwnd, WM_NULL, 0, 0);

    DestroyMenu(menu);
}
