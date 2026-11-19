//! Progress HUD (`progress_hud.rs`)
//!
//! Frameless, zero-chrome translucent glass HUD for long-running file transfers.
//!
//! Rules:
//! - Appears only when transfer duration exceeds 4.0s (zero UI for fast transfers).
//! - Progress already starts at true non-zero percentage (e.g. 20-30%+).
//! - Dismisses automatically at >= 90% so the desktop is pristine before spatial drop.

use std::ffi::{c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LRESULT, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DrawTextW,
    SelectObject, SetBkMode, SetTextColor, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, DT_END_ELLIPSIS, DT_LEFT,
    DT_RIGHT, DT_SINGLELINE, DT_VCENTER, HDC, HFONT, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, LoadCursorW, PostMessageW,
    RegisterClassExW, SetWindowPos, ShowWindow, SystemParametersInfoW, TranslateMessage,
    UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW, IDC_ARROW, SPI_GETWORKAREA, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WM_CLOSE, WM_DESTROY,
    WM_ERASEBKGND, WM_PAINT, WM_USER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const HUD_W: i32 = 320;
const HUD_H: i32 = 64;

const WM_HUD_UPDATE: u32 = WM_USER + 410;
const WM_HUD_DISMISS: u32 = WM_USER + 411;

struct HudState {
    file_name: String,
    percent: u32,
    speed_mb: f64,
    is_visible: bool,
    pos_x: i32,
    pos_y: i32,
    mem_dc: HDC,
    dib_bits: *mut u8,
    #[allow(dead_code)]
    dib_bitmap: isize,
    font_bold: HFONT,
    font_regular: HFONT,
}

unsafe impl Send for HudState {}

static HUD_HWND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static HUD_STATE: Mutex<Option<HudState>> = Mutex::new(None);

pub fn update_progress(file_name: &str, percent: u32, speed_mb: f64) {
    ensure_hud_window();

    let hwnd = HUD_HWND.load(Ordering::SeqCst) as HWND;
    if hwnd.is_null() {
        return;
    }

    {
        let mut guard = HUD_STATE.lock().unwrap();
        if let Some(ref mut s) = *guard {
            s.file_name = file_name.to_string();
            s.percent = percent.min(100);
            s.speed_mb = speed_mb;
        }
    }

    unsafe {
        PostMessageW(hwnd, WM_HUD_UPDATE, 0, 0);
    }
}

pub fn dismiss() {
    let hwnd = HUD_HWND.load(Ordering::SeqCst) as HWND;
    if !hwnd.is_null() {
        unsafe {
            PostMessageW(hwnd, WM_HUD_DISMISS, 0, 0);
        }
    }
}

fn ensure_hud_window() {
    let existing = HUD_HWND.load(Ordering::SeqCst);
    if !existing.is_null() {
        return;
    }

    std::thread::spawn(|| {
        run_hud_thread();
    });

    for _ in 0..50 {
        if !HUD_HWND.load(Ordering::SeqCst).is_null() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn run_hud_thread() {
    unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());
        let class_name: Vec<u16> = OsStr::new("FlickProgressHudWindow")
            .encode_wide()
            .chain(Some(0))
            .collect();

        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(hud_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: 0 as _,
            hCursor: LoadCursorW(0 as _, IDC_ARROW),
            hbrBackground: 0 as _,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: 0 as _,
        };

        RegisterClassExW(&wnd_class);

        let mut work_area: RECT = std::mem::zeroed();
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            &mut work_area as *mut _ as *mut _,
            0,
        );

        let x = (work_area.right - HUD_W - 24).max(0);
        let y = (work_area.bottom - HUD_H - 16).max(0);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_POPUP,
            x,
            y,
            HUD_W,
            HUD_H,
            0 as HWND,
            0 as _,
            hinstance,
            std::ptr::null(),
        );

        if hwnd.is_null() {
            return;
        }

        // Initialize DIB Section and Fonts
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = HUD_W;
        bmi.bmiHeader.biHeight = -HUD_H; // Top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB as u32;

        let mut bits_ptr: *mut c_void = std::ptr::null_mut();
        let hdc_screen = CreateCompatibleDC(0 as _);
        let dib_bitmap = CreateDIBSection(
            hdc_screen,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits_ptr,
            std::ptr::null_mut(),
            0,
        );

        let mem_dc = CreateCompatibleDC(hdc_screen);
        SelectObject(mem_dc, dib_bitmap as _);
        DeleteDC(hdc_screen);

        let font_face: Vec<u16> = OsStr::new("Work Sans")
            .encode_wide()
            .chain(Some(0))
            .collect();
        let font_bold = CreateFontW(
            13, 0, 0, 0, 700, 0, 0, 0, 0, 0, 0, 5, 0, font_face.as_ptr(),
        );
        let font_regular = CreateFontW(
            11, 0, 0, 0, 400, 0, 0, 0, 0, 0, 0, 5, 0, font_face.as_ptr(),
        );

        {
            let mut guard = HUD_STATE.lock().unwrap();
            *guard = Some(HudState {
                file_name: String::new(),
                percent: 0,
                speed_mb: 0.0,
                is_visible: false,
                pos_x: x,
                pos_y: y,
                mem_dc,
                dib_bits: bits_ptr as *mut u8,
                dib_bitmap: dib_bitmap as isize,
                font_bold,
                font_regular,
            });
        }

        HUD_HWND.store(hwnd as _, Ordering::SeqCst);

        let mut msg = std::mem::zeroed();
        while GetMessageW(&mut msg, 0 as HWND, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        HUD_HWND.store(std::ptr::null_mut(), Ordering::SeqCst);
    }
}

unsafe extern "system" fn hud_wnd_proc(
    hwnd: HWND,
    msg: u32,
    _wparam: usize,
    _lparam: isize,
) -> LRESULT {
    match msg {
        WM_HUD_UPDATE => {
            render_and_commit(hwnd);
            0
        }
        WM_HUD_DISMISS => {
            let mut guard = HUD_STATE.lock().unwrap();
            if let Some(ref mut s) = *guard {
                if s.is_visible {
                    s.is_visible = false;
                    ShowWindow(hwnd, SW_HIDE);
                }
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => 0,
        WM_CLOSE => {
            ShowWindow(hwnd, SW_HIDE);
            0
        }
        WM_DESTROY => 0,
        _ => DefWindowProcW(hwnd, msg, _wparam, _lparam),
    }
}

unsafe fn render_and_commit(hwnd: HWND) {
    let mut guard = HUD_STATE.lock().unwrap();
    let s = match *guard {
        Some(ref mut state) => state,
        None => return,
    };

    let total_bytes = (HUD_W * HUD_H * 4) as usize;
    let buffer = std::slice::from_raw_parts_mut(s.dib_bits, total_bytes);

    // 1. Clear to transparent
    buffer.fill(0);

    // 2. Draw Rounded Obsidian Glass Plate (#0D0E10, 92% alpha, 1px border #25282E)
    fill_rounded_box(
        buffer, HUD_W, HUD_H, 0, 0, HUD_W, HUD_H, 10,
        0x0D0E10, 235, 0x25282E, 255,
    );

    // 3. Draw Header Row: "⚡ FLICK // TELEPORT" (Safety Orange #FF4400) + Speed & %
    draw_hud_text(
        s.mem_dc,
        buffer,
        HUD_W,
        HUD_H,
        "⚡ FLICK // TELEPORT",
        14,
        8,
        180,
        22,
        0x000044FF, // Safety Orange BGR
        s.font_bold,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );

    let speed_percent_str = format!("{:.1} MB/S • {}%", s.speed_mb, s.percent);
    draw_hud_text(
        s.mem_dc,
        buffer,
        HUD_W,
        HUD_H,
        &speed_percent_str,
        180,
        8,
        HUD_W - 14,
        22,
        0x00EDEDED, // Crisp off-white BGR
        s.font_bold,
        DT_RIGHT | DT_SINGLELINE | DT_VCENTER,
    );

    // 4. Draw File Name (Muted #8E95A2)
    draw_hud_text(
        s.mem_dc,
        buffer,
        HUD_W,
        HUD_H,
        &s.file_name,
        14,
        26,
        HUD_W - 14,
        42,
        0x00A2958E, // Slate Muted BGR
        s.font_regular,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
    );

    // 5. Draw 2.5px Sleek Progress Bar
    let bar_x0 = 14;
    let bar_x1 = HUD_W - 14;
    let bar_y0 = 48;
    let bar_y1 = 51;
    let total_bar_w = bar_x1 - bar_x0;
    let fill_w = ((total_bar_w as f32 * (s.percent as f32 / 100.0)).round() as i32)
        .clamp(0, total_bar_w);

    // Track groove (#1C1F25)
    for y in bar_y0..bar_y1 {
        let row = (y as usize) * (HUD_W as usize) * 4;
        for x in bar_x0..bar_x1 {
            let p = row + (x as usize) * 4;
            buffer[p + 0] = 0x25;
            buffer[p + 1] = 0x1F;
            buffer[p + 2] = 0x1C;
            buffer[p + 3] = 255;
        }
    }

    // Active fill (#FF4400)
    for y in bar_y0..bar_y1 {
        let row = (y as usize) * (HUD_W as usize) * 4;
        for x in bar_x0..(bar_x0 + fill_w) {
            let p = row + (x as usize) * 4;
            buffer[p + 0] = 0x00;
            buffer[p + 1] = 0x44;
            buffer[p + 2] = 0xFF;
            buffer[p + 3] = 255;
        }
    }

    // 6. Commit to Windows Layered Window
    let mut pt_dst = POINT { x: s.pos_x, y: s.pos_y };
    let mut size = SIZE {
        cx: HUD_W,
        cy: HUD_H,
    };
    let mut pt_src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    UpdateLayeredWindow(
        hwnd,
        0 as _,
        &mut pt_dst,
        &mut size,
        s.mem_dc,
        &mut pt_src,
        0,
        &blend,
        ULW_ALPHA,
    );

    if !s.is_visible {
        s.is_visible = true;
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetWindowPos(
            hwnd,
            windows_sys::Win32::UI::WindowsAndMessaging::HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

unsafe fn draw_hud_text(
    hdc: HDC,
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    text: &str,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    colorref: u32,
    font: HFONT,
    flags: u32,
) {
    let mut rc = RECT {
        left: x0,
        top: y0,
        right: x1,
        bottom: y1,
    };

    let old_font = SelectObject(hdc, font as _);
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, colorref);

    let text_wide: Vec<u16> = OsStr::new(text).encode_wide().collect();
    DrawTextW(
        hdc,
        text_wide.as_ptr(),
        text_wide.len() as i32,
        &mut rc,
        flags,
    );

    SelectObject(hdc, old_font);

    // Alpha fixup pass for GDI 32-bit DIB
    let rx0 = rc.left.max(0).min(canvas_w - 1) as usize;
    let rx1 = rc.right.max(0).min(canvas_w) as usize;
    let ry0 = rc.top.max(0).min(canvas_h - 1) as usize;
    let ry1 = rc.bottom.max(0).min(canvas_h) as usize;

    for y in ry0..ry1 {
        let row = y * (canvas_w as usize) * 4;
        for x in rx0..rx1 {
            let p_idx = row + x * 4;
            if buffer[p_idx + 3] == 0 && (buffer[p_idx] != 0 || buffer[p_idx + 1] != 0 || buffer[p_idx + 2] != 0) {
                buffer[p_idx + 3] = 255;
            }
        }
    }
}

fn fill_rounded_box(
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    r: i32,
    bg_rgb: u32,
    bg_a: u8,
    border_rgb: u32,
    border_a: u8,
) {
    let r_f = r as f32;
    for y in y0..y1 {
        if y < 0 || y >= canvas_h {
            continue;
        }
        let row = (y as usize) * (canvas_w as usize) * 4;
        for x in x0..x1 {
            if x < 0 || x >= canvas_w {
                continue;
            }

            let mut dist_from_corner_center = 0.0f32;
            let mut in_corner = false;

            if x < x0 + r && y < y0 + r {
                let dx = (x0 + r - x) as f32;
                let dy = (y0 + r - y) as f32;
                dist_from_corner_center = (dx * dx + dy * dy).sqrt();
                in_corner = true;
            } else if x >= x1 - r && y < y0 + r {
                let dx = (x - (x1 - r - 1)) as f32;
                let dy = (y0 + r - y) as f32;
                dist_from_corner_center = (dx * dx + dy * dy).sqrt();
                in_corner = true;
            } else if x >= x1 - r && y >= y1 - r {
                let dx = (x - (x1 - r - 1)) as f32;
                let dy = (y - (y1 - r - 1)) as f32;
                dist_from_corner_center = (dx * dx + dy * dy).sqrt();
                in_corner = true;
            } else if x < x0 + r && y >= y1 - r {
                let dx = (x0 + r - x) as f32;
                let dy = (y - (y1 - r - 1)) as f32;
                dist_from_corner_center = (dx * dx + dy * dy).sqrt();
                in_corner = true;
            }

            if in_corner && dist_from_corner_center > r_f {
                continue;
            }

            let p_idx = row + (x as usize) * 4;
            let is_border = x == x0 || x == x1 - 1 || y == y0 || y == y1 - 1
                || (in_corner && dist_from_corner_center >= r_f - 1.2);

            let (rgb, a) = if is_border {
                (border_rgb, border_a)
            } else {
                (bg_rgb, bg_a)
            };

            let r_col = ((rgb >> 16) & 0xFF) as u8;
            let g_col = ((rgb >> 8) & 0xFF) as u8;
            let b_col = (rgb & 0xFF) as u8;

            let alpha_factor = (a as f32) / 255.0;
            buffer[p_idx + 0] = ((b_col as f32) * alpha_factor) as u8;
            buffer[p_idx + 1] = ((g_col as f32) * alpha_factor) as u8;
            buffer[p_idx + 2] = ((r_col as f32) * alpha_factor) as u8;
            buffer[p_idx + 3] = a;
        }
    }
}
