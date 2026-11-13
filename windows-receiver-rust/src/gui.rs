use crate::clipboard;
use crate::config::{
    clear_recent_transfers, get_config, get_lan_ip, get_recent_transfers,
    remove_paired_device, set_active_pair_secret, set_clipboard_mode,
    FlickConfig,
};
use qrcode::QrCode;
use rand::RngCore;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW,
    CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect,
    FrameRect, InvalidateRect, SelectObject, SetBkMode, SetTextColor,
    DT_CENTER, DT_LEFT, DT_NOCLIP, DT_SINGLELINE, DT_VCENTER, HBRUSH, HDC,
    HFONT, SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect,
    GetSystemMetrics, KillTimer, LoadCursorW, PostMessageW,
    RegisterClassExW, SetForegroundWindow, SetTimer, ShowWindow,
    CS_HREDRAW, CS_VREDRAW, IDC_ARROW, SM_CXSCREEN, SM_CYSCREEN, SW_RESTORE,
    SW_SHOW, SW_SHOWNORMAL, WM_ACTIVATE, WM_CLOSE, WM_CREATE, WM_DESTROY,
    WM_LBUTTONDOWN, WM_PAINT, WM_TIMER, WNDCLASSEXW, WS_CAPTION,
    WS_SYSMENU, WS_VISIBLE,
};

// ----------------------------------------------------------------------------
// Authentic Teenage Engineering Hardware Design Tokens (MEMORY.md)
// ----------------------------------------------------------------------------
const COLOR_BG: u32 = 0x000E0B0A; // #0A0B0E (BGR: 0E 0B 0A)
const COLOR_SURFACE: u32 = 0x00191513; // #131519
const COLOR_SURFACE_VARIANT: u32 = 0x00231D1A; // #1A1D23
const COLOR_OUTLINE: u32 = 0x002C2522; // #22252C
const COLOR_ORANGE: u32 = 0x000044FF; // #FF4400
const COLOR_ORANGE_LIP: u32 = 0x00001E7A; // #7A1E00
const COLOR_KEYCAP_DARK: u32 = 0x00382E2A; // #2A2E38
const COLOR_KEYCAP_LIP: u32 = 0x000C0908; // #08090C
const COLOR_TEXT_WHITE: u32 = 0x00FFFFFF;
const COLOR_TEXT_DIM: u32 = 0x00887C77; // #777C88
const COLOR_TEXT_GREEN: u32 = 0x0073CC2E; // #2ECC71

static DASHBOARD_HWND: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

struct GuiState {
    current_tab: usize, // 0 = Dashboard, 1 = QR Pairing
    qr_secret: String,
    qr_expiry: f64,
    qr_matrix: Option<Vec<Vec<bool>>>,
}

static GUI_STATE: Mutex<Option<GuiState>> = Mutex::new(None);

#[allow(dead_code)]
pub fn is_dashboard_open() -> bool {
    !DASHBOARD_HWND.load(Ordering::SeqCst).is_null()
}

pub fn get_dashboard_hwnd() -> HWND {
    DASHBOARD_HWND.load(Ordering::SeqCst) as HWND
}

pub fn close_dashboard_if_open() {
    let hwnd = DASHBOARD_HWND.load(Ordering::SeqCst) as HWND;
    if !hwnd.is_null() {
        unsafe {
            DestroyWindow(hwnd);
        }
    }
}

pub fn open_or_show_dashboard(show_qr_tab: bool) {
    let hwnd = DASHBOARD_HWND.load(Ordering::SeqCst) as HWND;
    if !hwnd.is_null() {
        unsafe {
            if show_qr_tab {
                let mut state = GUI_STATE.lock().unwrap();
                if let Some(s) = &mut *state {
                    s.current_tab = 1;
                    if s.qr_matrix.is_none() || SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64() > s.qr_expiry {
                        generate_fresh_qr_state(s);
                    }
                }
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            ShowWindow(hwnd, SW_RESTORE);
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
        return;
    }

    // Initialize state
    {
        let mut state = GUI_STATE.lock().unwrap();
        let mut s = GuiState {
            current_tab: if show_qr_tab { 1 } else { 0 },
            qr_secret: String::new(),
            qr_expiry: 0.0,
            qr_matrix: None,
        };
        if show_qr_tab {
            generate_fresh_qr_state(&mut s);
        }
        *state = Some(s);
    }

    create_dashboard_window();
}

fn generate_fresh_qr_state(state: &mut GuiState) {
    let mut rng_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut rng_bytes);
    let secret = hex::encode(rng_bytes);
    let duration_sec = 300.0; // 5 minutes

    set_active_pair_secret(&secret, duration_sec);

    let cfg = get_config();
    let lan_ip = get_lan_ip();

    let spki_pin = crate::crypto::get_pc_spki_pin();
    let qr_uri = format!(
        "flick://pair?v=2&ip={}&p={}&pin={}&s={}&id={}",
        lan_ip,
        cfg.http_port,
        spki_pin,
        secret,
        cfg.pc_id
    );

    if let Ok(code) = QrCode::new(qr_uri.as_bytes()) {
        let width = code.width();
        let mut matrix = Vec::with_capacity(width);
        for y in 0..width {
            let mut row = Vec::with_capacity(width);
            for x in 0..width {
                row.push(code[(x, y)] == qrcode::Color::Dark);
            }
            matrix.push(row);
        }
        state.qr_matrix = Some(matrix);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    state.qr_secret = secret;
    state.qr_expiry = now + duration_sec;
}

// ----------------------------------------------------------------------------
// Non-Blocking STA Folder Picker & Windhawk Defeat Solid Theme Enforcer
// ----------------------------------------------------------------------------

pub fn launch_folder_picker() {
    std::thread::spawn(|| {
        unsafe {
            use windows_sys::Win32::System::Com::{
                CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED,
            };
            use windows_sys::Win32::UI::Shell::{
                SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE,
                BIF_RETURNONLYFSDIRS, BROWSEINFOW,
            };

            CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED as _);

            let title_wide: Vec<u16> = OsStr::new("Select Folder for Received Flick Files")
                .encode_wide()
                .chain(Some(0))
                .collect();

            let mut bi: BROWSEINFOW = std::mem::zeroed();
            bi.hwndOwner = 0 as HWND; // Desktop owner prevents modal deadlock with parent message pump
            bi.lpszTitle = title_wide.as_ptr();
            bi.ulFlags = BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE;

            let pidl = SHBrowseForFolderW(&bi);
            if !pidl.is_null() {
                let mut path_buf = [0u16; 260];
                let ok = SHGetPathFromIDListW(pidl, path_buf.as_mut_ptr());
                CoTaskMemFree(pidl as _);

                if ok != 0 {
                    let len = path_buf.iter().position(|&c| c == 0).unwrap_or(path_buf.len());
                    let selected = String::from_utf16_lossy(&path_buf[..len]);
                    if !selected.is_empty() {
                        crate::config::set_save_directory(&selected);
                        let hwnd = DASHBOARD_HWND.load(Ordering::SeqCst) as HWND;
                        if !hwnd.is_null() {
                            InvalidateRect(hwnd, std::ptr::null(), 0);
                        }
                    }
                }
            }
            CoUninitialize();
        }
    });
}

pub unsafe fn enforce_solid_dark_theme(hwnd: HWND) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmEnableBlurBehindWindow, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
        DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMSBT_NONE,
        DWM_BLURBEHIND,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, SetWindowLongW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE,
        SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_LAYERED,
        WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_THICKFRAME,
    };

    #[repr(C)]
    struct Margins {
        cx_left_width: i32,
        cx_right_width: i32,
        cy_top_height: i32,
        cy_bottom_height: i32,
    }

    // 1. Defeat Windhawk IsWindowEligible: Strip minimize, maximize, and sizing border
    let mut style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
    style &= !(WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_THICKFRAME);
    SetWindowLongW(hwnd, GWL_STYLE, style as i32);

    // 2. Strip WS_EX_LAYERED so alpha-blended transparency is disallowed
    let mut ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    if (ex_style & WS_EX_LAYERED) != 0 {
        ex_style &= !WS_EX_LAYERED;
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style as i32);
    }

    SetWindowPos(
        hwnd,
        0 as _,
        0,
        0,
        0,
        0,
        SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
    );

    // 3. Reset DWM Glass Frame to {0, 0, 0, 0} (solid client area)
    let zero_margins = Margins {
        cx_left_width: 0,
        cx_right_width: 0,
        cy_top_height: 0,
        cy_bottom_height: 0,
    };
    DwmExtendFrameIntoClientArea(hwnd, &zero_margins as *const _ as _);

    // 4. Explicitly disable DWM Blur Behind
    let bb = DWM_BLURBEHIND {
        dwFlags: 1, // DWM_BB_ENABLE
        fEnable: 0,
        hRgnBlur: 0 as _,
        fTransitionOnMaximized: 0,
    };
    DwmEnableBlurBehindWindow(hwnd, &bb);

    // 5. Turn off SetWindowCompositionAttribute accent policy (ACCENT_STATE_DISABLED = 0)
    #[repr(C)]
    struct AccentPolicy {
        accent_state: i32,
        accent_flags: i32,
        gradient_color: i32,
        animation_id: i32,
    }

    #[repr(C)]
    struct WinCompAttrData {
        attrib: i32,
        pv_data: *mut core::ffi::c_void,
        cb_data: usize,
    }

    let user32_wide: Vec<u16> = OsStr::new("user32.dll").encode_wide().chain(Some(0)).collect();
    let h_user32 = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(user32_wide.as_ptr());
    if !h_user32.is_null() {
        let proc = windows_sys::Win32::System::LibraryLoader::GetProcAddress(
            h_user32,
            b"SetWindowCompositionAttribute\0".as_ptr(),
        );
        if let Some(set_win_comp) = proc {
            let func: unsafe extern "system" fn(HWND, *mut WinCompAttrData) -> windows_sys::Win32::Foundation::BOOL =
                core::mem::transmute(set_win_comp);
            let mut policy = AccentPolicy {
                accent_state: 0, // ACCENT_DISABLED
                accent_flags: 0,
                gradient_color: 0,
                animation_id: 0,
            };
            let mut data = WinCompAttrData {
                attrib: 19, // WCA_ACCENT_POLICY
                pv_data: &mut policy as *mut _ as _,
                cb_data: core::mem::size_of::<AccentPolicy>(),
            };
            func(hwnd, &mut data);
        }
    }

    // 6. Disable Backdrop Material (DWMSBT_NONE = 1)
    let backdrop = DWMSBT_NONE as u32;
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_SYSTEMBACKDROP_TYPE as _,
        &backdrop as *const _ as _,
        core::mem::size_of::<u32>() as u32,
    );

    // 7. Force Win32 Immersive Dark Mode for title bar & borders
    let dark_mode = 1i32;
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE as _,
        &dark_mode as *const _ as _,
        core::mem::size_of::<i32>() as u32,
    );
}

fn create_dashboard_window() {
    unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());
        let class_name: Vec<u16> = OsStr::new("FlickDashboardWindowClass")
            .encode_wide()
            .chain(Some(0))
            .collect();

        let icon = windows_sys::Win32::UI::WindowsAndMessaging::LoadIconW(hinstance, 1 as *const u16);
        let app_icon = if !icon.is_null() {
            icon
        } else {
            windows_sys::Win32::UI::WindowsAndMessaging::LoadIconW(0 as HINSTANCE, 32512 as *const u16)
        };

        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(dashboard_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: app_icon,
            hCursor: LoadCursorW(0 as HINSTANCE, IDC_ARROW as *const u16),
            hbrBackground: 0 as HBRUSH,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: app_icon,
        };

        RegisterClassExW(&wnd_class);

        let win_w = 540;
        let win_h = 680;
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let pos_x = (screen_w - win_w) / 2;
        let pos_y = (screen_h - win_h) / 2;

        let title_wide: Vec<u16> = OsStr::new("FLICK RX-1 DESKTOP RECEIVER")
            .encode_wide()
            .chain(Some(0))
            .collect();

        // WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU without WS_MINIMIZEBOX/THICKFRAME
        // defeats Windhawk's IsWindowEligible and prevents acrylic translucency injection!
        let window_style = windows_sys::Win32::UI::WindowsAndMessaging::WS_OVERLAPPED
            | WS_CAPTION
            | WS_SYSMENU
            | WS_VISIBLE;

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title_wide.as_ptr(),
            window_style,
            pos_x,
            pos_y,
            win_w,
            win_h,
            0 as HWND,
            0 as _,
            hinstance,
            std::ptr::null(),
        );

        if !hwnd.is_null() {
            DASHBOARD_HWND.store(hwnd as _, Ordering::SeqCst);
            enforce_solid_dark_theme(hwnd);
            SetTimer(hwnd, 1, 1000, None);
            SetTimer(hwnd, 2, 80, None); // Re-enforce theme to defeat late Windhawk hooks
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        } else {
            let err = windows_sys::Win32::Foundation::GetLastError();
            eprintln!("[GUI] CreateWindowExW failed with error: {}", err);
        }
    }
}

unsafe extern "system" fn dashboard_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            enforce_solid_dark_theme(hwnd);
            0
        }

        WM_ACTIVATE => {
            enforce_solid_dark_theme(hwnd);
            0
        }

        WM_TIMER => {
            if wparam == 1 {
                InvalidateRect(hwnd, std::ptr::null(), 0);
            } else if wparam == 2 {
                enforce_solid_dark_theme(hwnd);
                KillTimer(hwnd, 2);
            }
            0
        }

        WM_LBUTTONDOWN => {
            let x = (lparam & 0xFFFF) as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i32;
            handle_mouse_click(hwnd, x, y);
            0
        }

        WM_PAINT => {
            render_dashboard_gdi(hwnd);
            0
        }

        WM_CLOSE => {
            KillTimer(hwnd, 1);
            KillTimer(hwnd, 2);
            DestroyWindow(hwnd);
            0
        }

        WM_DESTROY => {
            DASHBOARD_HWND.store(std::ptr::null_mut(), Ordering::SeqCst);
            let mut state = GUI_STATE.lock().unwrap();
            *state = None;

            // Zero-RAM-When-Closed: Purge virtual pages back to OS
            clipboard::trim_working_set();
            0
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ----------------------------------------------------------------------------
// GDI Double-Buffered Paint Engine
// ----------------------------------------------------------------------------

unsafe fn render_dashboard_gdi(hwnd: HWND) {
    let mut ps = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);

    let mut client_rect: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client_rect);
    let w = client_rect.right - client_rect.left;
    let h = client_rect.bottom - client_rect.top;

    let mem_dc = CreateCompatibleDC(hdc);
    let mem_bmp = CreateCompatibleBitmap(hdc, w, h);
    let old_bmp = SelectObject(mem_dc, mem_bmp as _);

    // 1. Fill Chassis Background #0A0B0E
    let brush_bg = CreateSolidBrush(COLOR_BG);
    FillRect(mem_dc, &client_rect, brush_bg);
    DeleteObject(brush_bg as _);

    SetBkMode(mem_dc, TRANSPARENT as _);

    let font_title = create_gdi_font(18, true, false);
    let font_mono_bold = create_gdi_font(13, true, true);
    let font_mono = create_gdi_font(12, false, true);
    let font_sm = create_gdi_font(11, false, false);

    let cfg = get_config();
    let state_guard = GUI_STATE.lock().unwrap();
    let state = state_guard.as_ref();
    let current_tab = state.map(|s| s.current_tab).unwrap_or(0);

    // 2. Render Machined Header Panel (Y: 12 to 74)
    draw_panel_box(mem_dc, 16, 12, w - 32, 62, COLOR_SURFACE, COLOR_OUTLINE);

    let old_font = SelectObject(mem_dc, font_title as _);
    SetTextColor(mem_dc, COLOR_TEXT_WHITE);
    draw_text_left(mem_dc, "⚡ FLICK RX-1", 30, 22, 200, 24);

    SelectObject(mem_dc, font_mono as _);
    SetTextColor(mem_dc, COLOR_TEXT_DIM);
    draw_text_left(mem_dc, "SECURE LOCAL TELEPORT • TLS 1.3", 30, 46, 260, 18);

    // Status Pill (Green dot + LINK: CONNECTED)
    let dot_rect = RECT { left: w - 170, top: 28, right: w - 162, bottom: 36 };
    let dot_brush = CreateSolidBrush(COLOR_TEXT_GREEN);
    FillRect(mem_dc, &dot_rect, dot_brush);
    DeleteObject(dot_brush as _);

    SetTextColor(mem_dc, COLOR_TEXT_GREEN);
    draw_text_left(mem_dc, "LINK: ACTIVE", w - 156, 24, 120, 18);

    SetTextColor(mem_dc, COLOR_TEXT_DIM);
    draw_text_left(mem_dc, "SPEED: IDLE", w - 156, 42, 120, 18);

    // 3. Tab Buttons (Y: 84 to 116)
    let tab0_active = current_tab == 0;
    let tab1_active = current_tab == 1;

    draw_3d_keycap(
        mem_dc,
        16,
        84,
        (w - 36) / 2,
        32,
        "01 DASHBOARD",
        tab0_active,
        font_mono_bold,
    );
    draw_3d_keycap(
        mem_dc,
        16 + (w - 36) / 2 + 4,
        84,
        (w - 36) / 2,
        32,
        "02 PAIR PHONE (QR)",
        tab1_active,
        font_mono_bold,
    );

    // 4. Tab Content Views
    if current_tab == 0 {
        render_tab_dashboard(mem_dc, w, &cfg, font_mono_bold, font_mono, font_sm);
    } else {
        render_tab_qr(mem_dc, w, state, font_mono_bold, font_mono);
    }

    // Copy to screen DC
    BitBlt(hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);

    // Clean up GDI objects
    SelectObject(mem_dc, old_font);
    SelectObject(mem_dc, old_bmp);
    DeleteObject(font_title as _);
    DeleteObject(font_mono_bold as _);
    DeleteObject(font_mono as _);
    DeleteObject(font_sm as _);
    DeleteObject(mem_bmp as _);
    DeleteDC(mem_dc);

    EndPaint(hwnd, &ps);
}

// ----------------------------------------------------------------------------
// Tab 0: Dashboard View
// ----------------------------------------------------------------------------

unsafe fn render_tab_dashboard(
    hdc: HDC,
    w: i32,
    cfg: &FlickConfig,
    font_bold: HFONT,
    font_mono: HFONT,
    font_sm: HFONT,
) {
    // A. Destination Well (Y: 126 to 174)
    draw_panel_box(hdc, 16, 126, w - 32, 48, COLOR_SURFACE_VARIANT, COLOR_OUTLINE);
    SelectObject(hdc, font_sm as _);
    SetTextColor(hdc, COLOR_TEXT_DIM);
    draw_text_left(hdc, "DESTINATION RECEPTACLE:", 28, 132, 200, 16);

    SelectObject(hdc, font_mono as _);
    SetTextColor(hdc, COLOR_TEXT_WHITE);
    let path_display = if cfg.save_directory.chars().count() > 36 {
        let suffix: String = cfg.save_directory.chars().rev().take(33).collect::<Vec<_>>().into_iter().rev().collect();
        format!("...{}", suffix)
    } else {
        cfg.save_directory.clone()
    };
    draw_text_left(hdc, &path_display, 28, 150, w - 120, 18);

    // [CHANGE] mini-button
    draw_mini_button(hdc, w - 86, 138, 56, 24, "CHANGE", COLOR_SURFACE);

    // B. Mode Selector Keycaps (Y: 182 to 226)
    SelectObject(hdc, font_sm as _);
    SetTextColor(hdc, COLOR_TEXT_DIM);
    draw_text_left(hdc, "INJECTION FORMAT MATRIX:", 16, 182, 200, 16);

    let modes = ["SMART_MULTI", "FORCE_PATH", "FORCE_IMAGE", "FORCE_FILE"];
    let mode_labels = ["01 MULTI", "02 PATH", "03 BITMAP", "04 FILE"];
    let key_w = (w - 32 - 12) / 4;

    for i in 0..4 {
        let is_sel = cfg.clipboard_mode == modes[i];
        let kx = 16 + (i as i32) * (key_w + 4);
        draw_3d_keycap(hdc, kx, 200, key_w, 28, mode_labels[i], is_sel, font_bold);
    }

    // C. Paired Hardware Nodes (Y: 238 to 368)
    draw_panel_box(hdc, 16, 238, w - 32, 130, COLOR_SURFACE, COLOR_OUTLINE);
    SelectObject(hdc, font_sm as _);
    SetTextColor(hdc, COLOR_TEXT_DIM);
    draw_text_left(hdc, "AUTHENTICATED HARDWARE NODES:", 28, 246, 240, 16);

    let mut dev_y = 268;
    if cfg.paired_devices.is_empty() {
        SelectObject(hdc, font_mono as _);
        SetTextColor(hdc, COLOR_TEXT_DIM);
        draw_text_left(hdc, "NO NODES PAIRED — OPEN TAB 02 TO SCAN QR", 28, 290, w - 60, 20);
    } else {
        SelectObject(hdc, font_mono as _);
        for dev in cfg.paired_devices.iter().take(3) {
            SetTextColor(hdc, COLOR_TEXT_WHITE);
            let name_label = format!("📱 {}", dev.client_name);
            draw_text_left(hdc, &name_label, 28, dev_y, 180, 18);

            SetTextColor(hdc, COLOR_TEXT_DIM);
            let pin_short = if dev.client_spki_pin.len() >= 16 {
                format!("SPKI: {}...", &dev.client_spki_pin[..14])
            } else {
                "SPKI: VERIFIED".to_string()
            };
            draw_text_left(hdc, &pin_short, 210, dev_y, 180, 18);

            // [UNPAIR] mini button
            draw_mini_button(hdc, w - 86, dev_y - 2, 54, 20, "UNPAIR", COLOR_SURFACE_VARIANT);

            dev_y += 28;
        }
    }

    // D. Telemetry Log // Recent Transfers (Y: 378 to 564)
    draw_panel_box(hdc, 16, 378, w - 32, 186, COLOR_SURFACE_VARIANT, COLOR_OUTLINE);
    SelectObject(hdc, font_sm as _);
    SetTextColor(hdc, COLOR_TEXT_DIM);
    draw_text_left(hdc, "TELEMETRY LOG • RECENT TRANSFERS:", 28, 386, 260, 16);

    draw_mini_button(hdc, w - 74, 384, 42, 18, "CLEAR", COLOR_SURFACE);

    let transfers = get_recent_transfers();
    let mut log_y = 410;
    SelectObject(hdc, font_mono as _);

    if transfers.is_empty() {
        SetTextColor(hdc, COLOR_TEXT_DIM);
        draw_text_left(hdc, "READY • AWAITING STREAM PACKETS", 28, 450, w - 60, 20);
    } else {
        for t in transfers.iter().take(6) {
            SetTextColor(hdc, if t.mode == "TEXT" { COLOR_TEXT_GREEN } else { COLOR_ORANGE });
            draw_text_left(hdc, &format!("[{}]", t.mode), 28, log_y, 54, 18);

            SetTextColor(hdc, COLOR_TEXT_WHITE);
            let display_name = if t.file_name.chars().count() > 28 {
                let prefix: String = t.file_name.chars().take(25).collect();
                format!("{}...", prefix)
            } else {
                t.file_name.clone()
            };
            draw_text_left(hdc, &display_name, 86, log_y, 250, 18);

            SetTextColor(hdc, COLOR_TEXT_DIM);
            let size_str = format_bytes(t.file_size);
            draw_text_left(hdc, &size_str, w - 110, log_y, 80, 18);

            log_y += 24;
        }
    }

    // E. Bottom Action Bar (Y: 576 to 620)
    let autostart = crate::config::is_autostart_enabled();
    let btn_w = (w - 32 - 8) / 3;
    let b1_x = 16;
    let b2_x = 16 + btn_w + 4;
    let b3_x = 16 + (btn_w + 4) * 2;

    draw_3d_keycap(hdc, b1_x, 576, btn_w, 42, "OPEN FOLDER", true, font_bold);
    draw_3d_keycap(
        hdc,
        b2_x,
        576,
        btn_w,
        42,
        if autostart { "AUTOSTART: ON" } else { "AUTOSTART: OFF" },
        autostart,
        font_bold,
    );
    draw_3d_keycap(hdc, b3_x, 576, btn_w, 42, "HIDE TO TRAY", false, font_bold);
}

// ----------------------------------------------------------------------------
// Tab 1: QR Pairing View
// ----------------------------------------------------------------------------

unsafe fn render_tab_qr(
    hdc: HDC,
    w: i32,
    state: Option<&GuiState>,
    font_bold: HFONT,
    font_mono: HFONT,
) {
    // Optical Reticle Frame
    let center_x = w / 2;
    let qr_box_size = 280;
    let qr_x = center_x - qr_box_size / 2;
    let qr_y = 140;

    draw_panel_box(hdc, qr_x - 12, qr_y - 12, qr_box_size + 24, qr_box_size + 24, COLOR_SURFACE, COLOR_OUTLINE);

    // Draw Corner Reticle Accents #FF4400
    draw_reticle_corners(hdc, qr_x - 14, qr_y - 14, qr_box_size + 28, qr_box_size + 28);

    // Render QR Code Pixels
    if let Some(s) = state {
        if let Some(matrix) = &s.qr_matrix {
            let modules = matrix.len();
            let pixel_size = qr_box_size / modules as i32;
            let offset_x = qr_x + (qr_box_size - pixel_size * modules as i32) / 2;
            let offset_y = qr_y + (qr_box_size - pixel_size * modules as i32) / 2;

            // Fill white QR background
            let qr_bg_rect = RECT {
                left: offset_x - 4,
                top: offset_y - 4,
                right: offset_x + pixel_size * modules as i32 + 4,
                bottom: offset_y + pixel_size * modules as i32 + 4,
            };
            let white_brush = CreateSolidBrush(0x00FFFFFF);
            FillRect(hdc, &qr_bg_rect, white_brush);
            DeleteObject(white_brush as _);

            // Draw dark modules
            let dark_brush = CreateSolidBrush(0x00000000);
            for (r, row) in matrix.iter().enumerate() {
                for (c, &is_dark) in row.iter().enumerate() {
                    if is_dark {
                        let m_rect = RECT {
                            left: offset_x + c as i32 * pixel_size,
                            top: offset_y + r as i32 * pixel_size,
                            right: offset_x + (c as i32 + 1) * pixel_size,
                            bottom: offset_y + (r as i32 + 1) * pixel_size,
                        };
                        FillRect(hdc, &m_rect, dark_brush);
                    }
                }
            }
            DeleteObject(dark_brush as _);
        }

        // Countdown Timer
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let remaining_sec = (s.qr_expiry - now).max(0.0) as i32;
        let mins = remaining_sec / 60;
        let secs = remaining_sec % 60;

        SelectObject(hdc, font_bold as _);
        SetTextColor(hdc, COLOR_ORANGE);
        let time_str = format!("EPHEMERAL CODE EXPIRES IN: {:02}:{:02}", mins, secs);
        draw_text_center(hdc, &time_str, 20, 460, w - 40, 24);

        SelectObject(hdc, font_mono as _);
        SetTextColor(hdc, COLOR_TEXT_DIM);
        draw_text_center(hdc, "SCAN WITH FLICK ANDROID APP TO COMPLETE PAIRING", 20, 488, w - 40, 20);
    }

    // Action Buttons
    let btn_w = (w - 36) / 2;
    draw_3d_keycap(hdc, 16, 540, btn_w, 42, "REFRESH CODE", true, font_bold);
    draw_3d_keycap(hdc, 16 + btn_w + 4, 540, btn_w, 42, "BACK TO DASHBOARD", false, font_bold);
}

// ----------------------------------------------------------------------------
// Click Dispatch
// ----------------------------------------------------------------------------

unsafe fn handle_mouse_click(hwnd: HWND, x: i32, y: i32) {
    let mut client_rect: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client_rect);
    let w = client_rect.right - client_rect.left;

    let mut state_guard = GUI_STATE.lock().unwrap();
    let state = match state_guard.as_mut() {
        Some(s) => s,
        None => return,
    };

    // Tab 0 / Tab 1 Switcher (Y: 84 to 116)
    if y >= 84 && y <= 116 {
        let half_w = (w - 36) / 2;
        if x >= 16 && x <= 16 + half_w {
            state.current_tab = 0;
            InvalidateRect(hwnd, std::ptr::null(), 0);
            return;
        } else if x >= 16 + half_w + 4 && x <= w - 16 {
            state.current_tab = 1;
            if state.qr_matrix.is_none() {
                generate_fresh_qr_state(state);
            }
            InvalidateRect(hwnd, std::ptr::null(), 0);
            return;
        }
    }

    if state.current_tab == 0 {
        // Change destination directory button (X: w - 86 to w - 30, Y: 138 to 164)
        if x >= w - 86 && x <= w - 30 && y >= 138 && y <= 164 {
            launch_folder_picker();
            return;
        }

        // Mode Keycaps (Y: 200 to 228)
        if y >= 200 && y <= 228 {
            let key_w = (w - 32 - 12) / 4;
            let modes = ["SMART_MULTI", "FORCE_PATH", "FORCE_IMAGE", "FORCE_FILE"];
            for (i, mode) in modes.iter().enumerate() {
                let kx = 16 + (i as i32) * (key_w + 4);
                if x >= kx && x <= kx + key_w {
                    set_clipboard_mode(mode);
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                    return;
                }
            }
        }

        // Clear log button (X: w - 74 to w - 32, Y: 384 to 402)
        if x >= w - 74 && x <= w - 32 && y >= 384 && y <= 402 {
            clear_recent_transfers();
            InvalidateRect(hwnd, std::ptr::null(), 0);
            return;
        }

        // Unpair buttons (Y: 268..)
        let cfg = get_config();
        let mut dev_y = 268;
        for dev in cfg.paired_devices.iter().take(3) {
            if x >= w - 86 && x <= w - 32 && y >= dev_y - 2 && y <= dev_y + 18 {
                remove_paired_device(&dev.client_id);
                InvalidateRect(hwnd, std::ptr::null(), 0);
                return;
            }
            dev_y += 28;
        }

        // Bottom Actions (Y: 576 to 618)
        if y >= 576 && y <= 618 {
            let btn_w = (w - 32 - 8) / 3;
            let b1_x = 16;
            let b2_x = 16 + btn_w + 4;
            let b3_x = 16 + (btn_w + 4) * 2;

            if x >= b1_x && x <= b1_x + btn_w {
                // Open Downloads Folder
                let path_wide: Vec<u16> = OsStr::new(&cfg.save_directory)
                    .encode_wide()
                    .chain(Some(0))
                    .collect();
                let op_wide: Vec<u16> = OsStr::new("open").encode_wide().chain(Some(0)).collect();
                ShellExecuteW(
                    0 as HWND,
                    op_wide.as_ptr(),
                    path_wide.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    SW_SHOWNORMAL,
                );
                return;
            } else if x >= b2_x && x <= b2_x + btn_w {
                // Toggle Autostart with Windows
                let current = crate::config::is_autostart_enabled();
                crate::config::set_autostart_enabled(!current);
                InvalidateRect(hwnd, std::ptr::null(), 0);
                return;
            } else if x >= b3_x && x <= b3_x + btn_w {
                // Hide to Tray
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
                return;
            }
        }
    } else {
        // QR Tab Actions (Y: 540 to 582)
        if y >= 540 && y <= 582 {
            let btn_w = (w - 36) / 2;
            if x >= 16 && x <= 16 + btn_w {
                // Refresh QR Code
                generate_fresh_qr_state(state);
                InvalidateRect(hwnd, std::ptr::null(), 0);
                return;
            } else if x >= 16 + btn_w + 4 && x <= w - 16 {
                // Back to Dashboard
                state.current_tab = 0;
                InvalidateRect(hwnd, std::ptr::null(), 0);
                return;
            }
        }
    }
}

// ----------------------------------------------------------------------------
// GDI Helper Primitives
// ----------------------------------------------------------------------------

unsafe fn draw_panel_box(hdc: HDC, x: i32, y: i32, w: i32, h: i32, fill: u32, border: u32) {
    let r = RECT { left: x, top: y, right: x + w, bottom: y + h };
    let brush = CreateSolidBrush(fill);
    FillRect(hdc, &r, brush);
    DeleteObject(brush as _);

    let border_brush = CreateSolidBrush(border);
    FrameRect(hdc, &r, border_brush);
    DeleteObject(border_brush as _);
}

unsafe fn draw_3d_keycap(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    text: &str,
    is_active: bool,
    font: HFONT,
) {
    let (fill_color, lip_color, text_color) = if is_active {
        (COLOR_ORANGE, COLOR_ORANGE_LIP, COLOR_TEXT_WHITE)
    } else {
        (COLOR_KEYCAP_DARK, COLOR_KEYCAP_LIP, COLOR_TEXT_DIM)
    };

    // Bottom shadow lip
    let lip_r = RECT { left: x, top: y + h - 4, right: x + w, bottom: y + h };
    let lip_brush = CreateSolidBrush(lip_color);
    FillRect(hdc, &lip_r, lip_brush);
    DeleteObject(lip_brush as _);

    // Main button face
    let face_r = RECT { left: x, top: y, right: x + w, bottom: y + h - 3 };
    let face_brush = CreateSolidBrush(fill_color);
    FillRect(hdc, &face_r, face_brush);
    DeleteObject(face_brush as _);

    // Subtle outline
    let border_brush = CreateSolidBrush(COLOR_OUTLINE);
    FrameRect(hdc, &RECT { left: x, top: y, right: x + w, bottom: y + h }, border_brush);
    DeleteObject(border_brush as _);

    SelectObject(hdc, font as _);
    SetTextColor(hdc, text_color);
    let mut text_r = RECT { left: x, top: y, right: x + w, bottom: y + h - 3 };
    let text_wide: Vec<u16> = OsStr::new(text).encode_wide().chain(Some(0)).collect();
    DrawTextW(
        hdc,
        text_wide.as_ptr(),
        text_wide.len() as i32 - 1,
        &mut text_r,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOCLIP,
    );
}

unsafe fn draw_mini_button(hdc: HDC, x: i32, y: i32, w: i32, h: i32, text: &str, bg: u32) {
    let r = RECT { left: x, top: y, right: x + w, bottom: y + h };
    let b = CreateSolidBrush(bg);
    FillRect(hdc, &r, b);
    DeleteObject(b as _);

    let ob = CreateSolidBrush(COLOR_OUTLINE);
    FrameRect(hdc, &r, ob);
    DeleteObject(ob as _);

    SetTextColor(hdc, COLOR_TEXT_DIM);
    let mut tr = r;
    let wide: Vec<u16> = OsStr::new(text).encode_wide().chain(Some(0)).collect();
    DrawTextW(hdc, wide.as_ptr(), wide.len() as i32 - 1, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
}

unsafe fn draw_reticle_corners(hdc: HDC, x: i32, y: i32, w: i32, h: i32) {
    let orange_brush = CreateSolidBrush(COLOR_ORANGE);
    let arm = 14;
    let thick = 3;

    // Top-left
    FillRect(hdc, &RECT { left: x, top: y, right: x + arm, bottom: y + thick }, orange_brush);
    FillRect(hdc, &RECT { left: x, top: y, right: x + thick, bottom: y + arm }, orange_brush);

    // Top-right
    FillRect(hdc, &RECT { left: x + w - arm, top: y, right: x + w, bottom: y + thick }, orange_brush);
    FillRect(hdc, &RECT { left: x + w - thick, top: y, right: x + w, bottom: y + arm }, orange_brush);

    // Bottom-left
    FillRect(hdc, &RECT { left: x, top: y + h - thick, right: x + arm, bottom: y + h }, orange_brush);
    FillRect(hdc, &RECT { left: x, top: y + h - arm, right: x + thick, bottom: y + h }, orange_brush);

    // Bottom-right
    FillRect(hdc, &RECT { left: x + w - arm, top: y + h - thick, right: x + w, bottom: y + h }, orange_brush);
    FillRect(hdc, &RECT { left: x + w - thick, top: y + h - arm, right: x + w, bottom: y + h }, orange_brush);

    DeleteObject(orange_brush as _);
}

unsafe fn draw_text_left(hdc: HDC, text: &str, x: i32, y: i32, w: i32, h: i32) {
    let mut r = RECT { left: x, top: y, right: x + w, bottom: y + h };
    let wide: Vec<u16> = OsStr::new(text).encode_wide().chain(Some(0)).collect();
    DrawTextW(hdc, wide.as_ptr(), wide.len() as i32 - 1, &mut r, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOCLIP);
}

unsafe fn draw_text_center(hdc: HDC, text: &str, x: i32, y: i32, w: i32, h: i32) {
    let mut r = RECT { left: x, top: y, right: x + w, bottom: y + h };
    let wide: Vec<u16> = OsStr::new(text).encode_wide().chain(Some(0)).collect();
    DrawTextW(hdc, wide.as_ptr(), wide.len() as i32 - 1, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOCLIP);
}

unsafe fn create_gdi_font(size: i32, bold: bool, monospace: bool) -> HFONT {
    let font_name = if monospace { "Consolas" } else { "Segoe UI" };
    let font_name_wide: Vec<u16> = OsStr::new(font_name).encode_wide().chain(Some(0)).collect();

    CreateFontW(
        -size,
        0,
        0,
        0,
        if bold { 700 } else { 400 },
        0,
        0,
        0,
        1, // DEFAULT_CHARSET
        0,
        0,
        5, // CLEARTYPE_QUALITY
        0,
        font_name_wide.as_ptr(),
    )
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
