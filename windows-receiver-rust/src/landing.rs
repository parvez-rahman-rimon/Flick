//! Spatial Landing Canvas (`landing.rs`)
//!
//! Recreates Declan Tan's Flik / Tony Stark 3D spatial photo landing experience:
//! - **Zero-Chrome Transparent Floating Canvas:**
//!   No white container box or modal frames. Photos float directly in 3D space on the desktop.
//! - **True 3D Spatial Perspective Arc (Cover Flow / Cylindrical Gallery):**
//!   - Single Photo: Full-size high-res floating photo card with ambient drop shadow.
//!   - 2 Photos: Casually overlapping polaroids with natural physical rotation and depth.
//!   - 3+ Photos: 3D perspective fan-out arc curving into the screen (yaw perspective rotation).
//!   - 4-5+ Photos: Full 5-card spatial gallery with "+N more" badge.
//! - **Physical Sticky Note Drop:** Drops down from above the screen with damped harmonic spring bounce.
//! - **Native Multi-File OLE Drag & Drop:** Dragging ANY photo drops all batch files via CF_HDROP.
//! - **Instant Paste Dismissal:** `Ctrl + V` pastes into the active application and dissolves the cards.
//! - **Escape Dismissal:** `Esc` clears the desktop instantly.

use std::ffi::{c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use windows_sys::core::{GUID, HRESULT};
use windows_sys::Win32::Foundation::{
    BOOL, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, HWND, LRESULT, POINT, RECT, SIZE, S_OK,
};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW,
    GetDIBits, GetObjectW, SelectObject, SetBkMode, SetTextColor, AC_SRC_ALPHA, AC_SRC_OVER,
    BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, DT_CENTER,
    DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_TOP, DT_VCENTER,
    DT_WORDBREAK, HBITMAP, HDC, HFONT, TRANSPARENT,
};
use windows_sys::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, FORMATETC, STGMEDIUM,
    TYMED_HGLOBAL,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
};
use windows_sys::Win32::System::Ole::{
    DoDragDrop, CF_HDROP, DROPEFFECT_COPY, DROPEFFECT_MOVE,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, ReleaseCapture, VK_CONTROL, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT,
};
use windows_sys::Win32::UI::Shell::{SHCreateItemFromParsingName, ShellExecuteW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetSystemMetrics,
    GetWindowRect, KillTimer, LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassExW,
    SendMessageW, SetTimer, SetWindowPos, ShowWindow, SystemParametersInfoW, TranslateMessage,
    UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW, HTCAPTION, IDC_ARROW, SM_CXDRAG, SM_CYDRAG,
    SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, SW_SHOWNORMAL,
    ULW_ALPHA, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCLBUTTONDOWN, WM_PAINT, WM_TIMER, WM_USER, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

// ----------------------------------------------------------------------------
// Physics & Animation Constants
// ----------------------------------------------------------------------------
const HWND_TOPMOST: HWND = -1 as isize as _;

const ANIM_DURATION_MS: f32 = 360.0;
const DISMISS_DURATION_MS: f32 = 140.0;
const TIMEOUT_AUTO_DISMISS_SECS: f32 = 20.0;

const TIMER_ANIM_TICK: usize = 1;
const TIMER_KEY_CHECK: usize = 2;
const WM_SHOW_LANDING: u32 = WM_USER + 201;

const DRAGDROP_S_USEDEFAULTCURSORS: HRESULT = 0x00040102;
const MK_LBUTTON: u32 = 0x0001;

// ----------------------------------------------------------------------------
// Types & Structures
// ----------------------------------------------------------------------------

#[derive(Clone)]
pub enum LandingItem {
    Files {
        paths: Vec<PathBuf>,
        primary_name: String,
        count: usize,
        total_bytes: u64,
        is_image: bool,
    },
    Url {
        url: String,
    },
    Text {
        preview: String,
        length: usize,
    },
}

#[derive(PartialEq, Clone, Copy)]
enum AnimState {
    Entering,
    Sustained,
    Exiting,
    Hidden,
}

#[derive(Clone, Copy)]
struct Point2D {
    x: f32,
    y: f32,
}

#[derive(Clone)]
struct ProjectedCard {
    corners: [Point2D; 4], // P0: TL, P1: TR, P2: BR, P3: BL
    #[allow(dead_code)]
    z_depth: f32,
    image_idx: usize,
}

struct LandingState {
    item: LandingItem,
    anim_state: AnimState,
    start_time: Instant,
    last_interaction: Instant,
    canvas_w: i32,
    canvas_h: i32,
    target_x: i32,
    target_y: i32,
    start_y: i32,
    current_y: i32,
    current_alpha: u8,
    is_dragging_candidate: bool,
    drag_start_pt: POINT,
    mem_dc: HDC,
    dib_bitmap: HBITMAP,
    dib_bits: *mut u8,
    cards: Vec<ProjectedCard>,
    images: Vec<LoadedImage>,
    focus_idx: usize,
    clicked_card_idx: Option<usize>,
}

unsafe impl Send for LandingState {}
unsafe impl Sync for LandingState {}

struct PendingPayload {
    item: LandingItem,
    canvas_w: i32,
    canvas_h: i32,
    images: Vec<LoadedImage>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CardVisualType {
    Photo,
    Pdf,
    Video,
}

pub struct LoadedImage {
    pub w: u32,
    pub h: u32,
    pub bgra: Vec<u8>,
    pub visual_type: CardVisualType,
}

static LANDING_HWND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static PENDING_PAYLOAD: Mutex<Option<PendingPayload>> = Mutex::new(None);
static ACTIVE_STATE: Mutex<Option<Arc<Mutex<LandingState>>>> = Mutex::new(None);

// ----------------------------------------------------------------------------
// Public Entry Points
// ----------------------------------------------------------------------------

fn is_visual_ext(ext: &str) -> bool {
    matches!(
        ext,
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "pdf" | "mp4" | "mkv" | "mov" | "avi" | "webm"
    )
}

fn get_file_visual_type(ext: &str) -> Option<CardVisualType> {
    match ext {
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" => Some(CardVisualType::Photo),
        "pdf" => Some(CardVisualType::Pdf),
        "mp4" | "mkv" | "mov" | "avi" | "webm" => Some(CardVisualType::Video),
        _ => None,
    }
}

pub fn show_landing_files(paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }

    let count = paths.len();
    let primary_path = paths[0].clone();
    let primary_name = primary_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());

    let mut total_bytes = 0u64;
    let mut visual_count = 0usize;

    for p in &paths {
        if let Ok(meta) = std::fs::metadata(p) {
            total_bytes += meta.len();
        }
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if is_visual_ext(&ext) {
            visual_count += 1;
        }
    }

    let is_image = visual_count > 0;
    let item = LandingItem::Files {
        paths,
        primary_name,
        count,
        total_bytes,
        is_image,
    };

    dispatch_landing(item);
}

pub fn show_landing_url(url: String) {
    let item = LandingItem::Url { url };
    dispatch_landing(item);
}

pub fn show_landing_text(text: &str) {
    let trimmed = text.trim();
    let preview = if trimmed.len() > 160 {
        format!("{}...", &trimmed[..160])
    } else {
        trimmed.to_string()
    };
    let item = LandingItem::Text {
        preview,
        length: trimmed.chars().count(),
    };
    dispatch_landing(item);
}

// ----------------------------------------------------------------------------
// Windows Shell Thumbnail Factory & Image Loading Engine
// ----------------------------------------------------------------------------

#[repr(C)]
#[allow(non_snake_case)]
struct IShellItemImageFactoryVtbl {
    pub QueryInterface: unsafe extern "system" fn(this: *mut c_void, riid: *const GUID, ppv: *mut *mut c_void) -> HRESULT,
    pub AddRef: unsafe extern "system" fn(this: *mut c_void) -> u32,
    pub Release: unsafe extern "system" fn(this: *mut c_void) -> u32,
    pub GetImage: unsafe extern "system" fn(this: *mut c_void, size: SIZE, flags: u32, phbm: *mut HBITMAP) -> HRESULT,
}

#[repr(C)]
#[allow(non_snake_case)]
struct IShellItemImageFactory {
    pub lpVtbl: *const IShellItemImageFactoryVtbl,
}

// Windows Shell IShellItemImageFactory: {bcc18b79-ba16-442f-80c4-8a59c30c463b}
const IID_ISHELLITEMIMAGEFACTORY: GUID = GUID {
    data1: 0xbcc18b79,
    data2: 0xba16,
    data3: 0x442f,
    data4: [0x80, 0xc4, 0x8a, 0x59, 0xc3, 0x0c, 0x46, 0x3b],
};

const SIIGBF_BIGGERSIZEOK: u32 = 0x01;
const SIIGBF_SCALEUP: u32 = 0x100;

fn extract_shell_thumbnail(
    path: &Path,
    target_w: i32,
    target_h: i32,
    visual_type: CardVisualType,
) -> Option<LoadedImage> {
    unsafe {
        let hr_init = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED as _);
        let should_uninit = hr_init >= 0;

        let path_wide: Vec<u16> = OsStr::new(path.as_os_str())
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut factory_ptr: *mut c_void = std::ptr::null_mut();

        let hr = SHCreateItemFromParsingName(
            path_wide.as_ptr(),
            std::ptr::null_mut(),
            &IID_ISHELLITEMIMAGEFACTORY,
            &mut factory_ptr,
        );

        if hr != S_OK || factory_ptr.is_null() {
            crate::http_server::log_http(&format!("SHCreateItemFromParsingName failed for {:?}: hr=0x{:X}", path, hr as u32));
            if should_uninit {
                CoUninitialize();
            }
            return None;
        }

        let factory = factory_ptr as *mut IShellItemImageFactory;
        let size = SIZE {
            cx: target_w,
            cy: target_h,
        };
        let flags = SIIGBF_BIGGERSIZEOK | SIIGBF_SCALEUP;
        let mut hbm: HBITMAP = 0 as _;

        let hr_img = ((*(*factory).lpVtbl).GetImage)(factory_ptr, size, flags, &mut hbm);
        ((*(*factory).lpVtbl).Release)(factory_ptr);

        if should_uninit {
            CoUninitialize();
        }

        if hr_img != S_OK || hbm.is_null() {
            crate::http_server::log_http(&format!("GetImage failed for {:?}: hr=0x{:X}", path, hr_img as u32));
            return None;
        }

        let mut bm: BITMAP = std::mem::zeroed();
        let ret = GetObjectW(
            hbm as _,
            std::mem::size_of::<BITMAP>() as i32,
            &mut bm as *mut _ as *mut c_void,
        );
        if ret == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
            DeleteObject(hbm as _);
            return None;
        }

        let w = bm.bmWidth as u32;
        let h = bm.bmHeight as u32;
        crate::http_server::log_http(&format!("extract_shell_thumbnail SUCCESS for {:?}: {}x{}", path, w, h));

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: bm.bmWidth,
                biHeight: -bm.bmHeight,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: (w * h * 4) as u32,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [windows_sys::Win32::Graphics::Gdi::RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }; 1],
        };

        let mut bgra = vec![0u8; (w * h * 4) as usize];
        let hdc = CreateCompatibleDC(0 as _);
        let scanlines = GetDIBits(
            hdc,
            hbm,
            0,
            h,
            bgra.as_mut_ptr() as *mut c_void,
            &mut bmi,
            DIB_RGB_COLORS,
        );
        DeleteDC(hdc);
        DeleteObject(hbm as _);

        if scanlines == 0 {
            return None;
        }

        let mut all_zero_alpha = true;
        for chunk in bgra.chunks_exact(4) {
            if chunk[3] != 0 {
                all_zero_alpha = false;
                break;
            }
        }
        if all_zero_alpha {
            for chunk in bgra.chunks_exact_mut(4) {
                chunk[3] = 255;
            }
        }

        Some(LoadedImage {
            w,
            h,
            bgra,
            visual_type,
        })
    }
}

fn crop_to_fill_loaded(img: LoadedImage, target_w: u32, target_h: u32) -> LoadedImage {
    if img.w == target_w && img.h == target_h {
        return img;
    }
    let mut rgba_bytes = img.bgra.clone();
    for chunk in rgba_bytes.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
    if let Some(rgba_img) = image::RgbaImage::from_raw(img.w, img.h, rgba_bytes) {
        let dyn_img = image::DynamicImage::ImageRgba8(rgba_img);
        let filled = dyn_img
            .resize_to_fill(target_w, target_h, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        rgba_to_loaded(&filled, img.visual_type)
    } else {
        img
    }
}

fn load_single_file_visual(path: &Path, vtype: CardVisualType) -> Option<LoadedImage> {
    match vtype {
        CardVisualType::Photo => {
            if let Ok(img) = image::open(path) {
                let (ow, oh) = (img.width(), img.height());
                let aspect = ow as f32 / oh.max(1) as f32;
                let (target_w, target_h) = if aspect > 1.2 {
                    (560u32, (560.0 / aspect).round() as u32)
                } else if aspect < 0.8 {
                    ((500.0 * aspect).round().max(320.0) as u32, 500u32)
                } else {
                    (460u32, 460u32)
                };
                let resized = img
                    .resize_to_fill(target_w, target_h, image::imageops::FilterType::Lanczos3)
                    .to_rgba8();
                Some(rgba_to_loaded(&resized, CardVisualType::Photo))
            } else {
                None
            }
        }
        CardVisualType::Pdf => {
            extract_shell_thumbnail(path, 400, 560, CardVisualType::Pdf)
        }
        CardVisualType::Video => {
            extract_shell_thumbnail(path, 560, 320, CardVisualType::Video)
        }
    }
}

fn load_batch_file_visual(
    path: &Path,
    target_w: u32,
    target_h: u32,
    vtype: CardVisualType,
) -> Option<LoadedImage> {
    match vtype {
        CardVisualType::Photo => {
            if let Ok(img) = image::open(path) {
                let resized = img
                    .resize_to_fill(target_w, target_h, image::imageops::FilterType::Lanczos3)
                    .to_rgba8();
                Some(rgba_to_loaded(&resized, CardVisualType::Photo))
            } else {
                None
            }
        }
        CardVisualType::Pdf | CardVisualType::Video => {
            let raw_thumb = extract_shell_thumbnail(path, target_w as i32, target_h as i32, vtype)?;
            Some(crop_to_fill_loaded(raw_thumb, target_w, target_h))
        }
    }
}

fn rgba_to_loaded(img: &image::RgbaImage, visual_type: CardVisualType) -> LoadedImage {
    let (w, h) = img.dimensions();
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for pixel in img.pixels() {
        let [r, g, b, a] = pixel.0;
        bgra.push(b);
        bgra.push(g);
        bgra.push(r);
        bgra.push(a);
    }
    LoadedImage {
        w,
        h,
        bgra,
        visual_type,
    }
}

// ----------------------------------------------------------------------------
// Scene Assembly & Dispatch
// ----------------------------------------------------------------------------

fn dispatch_landing(item: LandingItem) {
    let mut images = Vec::new();
    let (canvas_w, canvas_h) = match &item {
        LandingItem::Files { paths, is_image, count, .. } if *is_image && !paths.is_empty() => {
            if *count == 1 {
                let p = &paths[0];
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                let vtype = get_file_visual_type(&ext).unwrap_or(CardVisualType::Photo);

                if let Some(img) = load_single_file_visual(p, vtype) {
                    let aspect = img.w as f32 / img.h.max(1) as f32;
                    let (cw, ch) = if aspect > 1.2 {
                        (540.0, 540.0 / aspect)
                    } else if aspect < 0.8 {
                        (480.0 * aspect, 480.0)
                    } else {
                        (440.0, 440.0)
                    };
                    images.push(img);
                    let win_w = (cw + 240.0).max(780.0) as i32;
                    let win_h = (ch + 200.0).max(640.0) as i32;
                    (win_w, win_h)
                } else {
                    (540, 280)
                }
            } else if *count == 2 {
                for p in paths.iter().take(2) {
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    let vtype = get_file_visual_type(&ext).unwrap_or(CardVisualType::Photo);
                    if let Some(img) = load_batch_file_visual(p, 360, 460, vtype) {
                        images.push(img);
                    }
                }
                if !images.is_empty() {
                    (880, 600)
                } else {
                    (540, 280)
                }
            } else {
                // Dynamic 3D Fanned Arc Deck for 3, 4, 5, 6, 7, 8, 9+ visual files!
                let take_n = (*count).min(9);
                for p in paths.iter().take(take_n) {
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    let vtype = get_file_visual_type(&ext).unwrap_or(CardVisualType::Photo);
                    if let Some(img) = load_batch_file_visual(p, 380, 480, vtype) {
                        images.push(img);
                    }
                }
                if !images.is_empty() {
                    let w = match images.len() {
                        3 => 1120,
                        4..=5 => 1280,
                        6..=7 => 1440,
                        _ => 1560,
                    };
                    (w, 700)
                } else {
                    (540, 280)
                }
            }
        }
        LandingItem::Files { .. } => (540, 280),
        LandingItem::Url { .. } => (540, 280),
        LandingItem::Text { .. } => (540, 300),
    };

    {
        let mut guard = PENDING_PAYLOAD.lock().unwrap();
        *guard = Some(PendingPayload {
            item,
            canvas_w,
            canvas_h,
            images,
        });
    }

    ensure_ui_thread_running();

    let hwnd = LANDING_HWND.load(Ordering::SeqCst) as HWND;
    if !hwnd.is_null() {
        unsafe {
            PostMessageW(hwnd, WM_SHOW_LANDING, 0, 0);
        }
    }
}

// ----------------------------------------------------------------------------
// Dedicated Persistent Win32 UI Thread
// ----------------------------------------------------------------------------

fn ensure_ui_thread_running() {
    let hwnd = LANDING_HWND.load(Ordering::SeqCst) as HWND;
    if !hwnd.is_null() {
        return;
    }

    static SPAWN_LOCK: Mutex<()> = Mutex::new(());
    let _guard = SPAWN_LOCK.lock().unwrap();

    let hwnd_check = LANDING_HWND.load(Ordering::SeqCst) as HWND;
    if !hwnd_check.is_null() {
        return;
    }

    std::thread::spawn(|| {
        run_persistent_landing_thread();
    });

    for _ in 0..50 {
        let h = LANDING_HWND.load(Ordering::SeqCst) as HWND;
        if !h.is_null() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn run_persistent_landing_thread() {
    unsafe {
        CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED as _);

        let hinstance = GetModuleHandleW(std::ptr::null());
        let class_name: Vec<u16> = OsStr::new("FlickSpatialFloatingCanvasClass")
            .encode_wide()
            .chain(Some(0))
            .collect();

        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(landing_wnd_proc),
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

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_POPUP,
            0,
            -2000,
            800,
            600,
            0 as HWND,
            0 as _,
            hinstance,
            std::ptr::null(),
        );

        if hwnd.is_null() {
            crate::http_server::log_http("Failed to create floating spatial landing window");
            CoUninitialize();
            return;
        }

        LANDING_HWND.store(hwnd as _, Ordering::SeqCst);

        PostMessageW(hwnd, WM_SHOW_LANDING, 0, 0);

        let mut msg = std::mem::zeroed();
        while GetMessageW(&mut msg, 0 as HWND, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        LANDING_HWND.store(std::ptr::null_mut(), Ordering::SeqCst);
        CoUninitialize();
    }
}

// ----------------------------------------------------------------------------
// Window Procedure Callback
// ----------------------------------------------------------------------------

unsafe extern "system" fn landing_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> LRESULT {
    match msg {
        WM_SHOW_LANDING => {
            handle_show_landing(hwnd);
            0
        }
        WM_TIMER => {
            if wparam == TIMER_ANIM_TICK {
                handle_anim_tick(hwnd);
            } else if wparam == TIMER_KEY_CHECK {
                handle_key_check(hwnd);
            }
            0
        }
        WM_PAINT => 0,
        WM_ERASEBKGND => 1,
        WM_LBUTTONDOWN => {
            let x = (lparam & 0xFFFF) as i16 as f32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as f32;

            if let Some(state_arc) = get_active_state() {
                let mut s = state_arc.lock().unwrap();

                // 1. Check pagination dots first!
                let total_dots = s.images.len().min(12);
                let mut clicked_dot = None;
                if total_dots > 1 {
                    let dot_spacing = 16.0f32;
                    let cx = (s.canvas_w as f32) / 2.0;
                    let cy = (s.canvas_h as f32) / 2.0;
                    let start_x = cx - ((total_dots as f32 - 1.0) * dot_spacing) / 2.0;
                    let y_center = cy + 252.0;

                    if (y - y_center).abs() <= 14.0 {
                        for i in 0..total_dots {
                            let dot_x = start_x + (i as f32) * dot_spacing;
                            if (x - dot_x).abs() <= 10.0 {
                                clicked_dot = Some(i);
                                break;
                            }
                        }
                    }
                }

                if let Some(dot_idx) = clicked_dot {
                    if dot_idx != s.focus_idx {
                        s.focus_idx = dot_idx;
                        let total_bytes = (s.canvas_w * s.canvas_h * 4) as usize;
                        let dib_slice = unsafe { std::slice::from_raw_parts_mut(s.dib_bits, total_bytes) };
                        s.cards = render_3d_spatial_scene(
                            s.mem_dc,
                            dib_slice,
                            s.canvas_w,
                            s.canvas_h,
                            &s.item,
                            &s.images,
                            s.focus_idx,
                        );
                        unsafe {
                            commit_layered_frame(hwnd, &s);
                        }
                    }
                    s.last_interaction = Instant::now();
                    return 0;
                }

                // 2. Check if clicked inside any 3D card (iterate in reverse: front card first!)
                let clicked_card = s
                    .cards
                    .iter()
                    .rev()
                    .find(|c| point_in_quad(Point2D { x, y }, &c.corners))
                    .map(|c| c.image_idx);

                if let Some(card_idx) = clicked_card {
                    s.is_dragging_candidate = true;
                    s.clicked_card_idx = Some(card_idx);
                    s.drag_start_pt = POINT {
                        x: x as i32,
                        y: y as i32,
                    };
                    s.last_interaction = Instant::now();
                } else {
                    // Clicked empty area around floating cards: Move window!
                    drop(s);
                    ReleaseCapture();
                    SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);

                    let mut rc: RECT = std::mem::zeroed();
                    GetWindowRect(hwnd, &mut rc);
                    if let Some(st_arc) = get_active_state() {
                        let mut st = st_arc.lock().unwrap();
                        st.target_x = rc.left;
                        st.target_y = rc.top;
                    }
                }
            }
            0
        }
        WM_MOUSEMOVE => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            if let Some(state_arc) = get_active_state() {
                let mut s = state_arc.lock().unwrap();

                if s.is_dragging_candidate && (wparam & MK_LBUTTON as usize) != 0 {
                    let dx = (x - s.drag_start_pt.x).abs();
                    let dy = (y - s.drag_start_pt.y).abs();
                    let drag_cx = GetSystemMetrics(SM_CXDRAG);
                    let drag_cy = GetSystemMetrics(SM_CYDRAG);

                    if dx > drag_cx || dy > drag_cy {
                        s.is_dragging_candidate = false;
                        s.clicked_card_idx = None;
                        let item_clone = s.item.clone();
                        drop(s);

                        // Initiate native Windows OLE DoDragDrop!
                        if let LandingItem::Files { paths, .. } = item_clone {
                            let dropped = execute_file_drag_drop(&paths);
                            if dropped {
                                trigger_dismiss();
                            }
                        }
                    }
                }
            }
            0
        }
        WM_LBUTTONUP => {
            if let Some(state_arc) = get_active_state() {
                let mut s = state_arc.lock().unwrap();
                if s.is_dragging_candidate {
                    s.is_dragging_candidate = false;
                    if let Some(clicked_idx) = s.clicked_card_idx.take() {
                        if !s.images.is_empty() {
                            if clicked_idx != s.focus_idx {
                                s.focus_idx = clicked_idx;
                                let total_bytes = (s.canvas_w * s.canvas_h * 4) as usize;
                                let dib_slice = unsafe { std::slice::from_raw_parts_mut(s.dib_bits, total_bytes) };
                                s.cards = render_3d_spatial_scene(
                                    s.mem_dc,
                                    dib_slice,
                                    s.canvas_w,
                                    s.canvas_h,
                                    &s.item,
                                    &s.images,
                                    s.focus_idx,
                                );
                                unsafe {
                                    commit_layered_frame(hwnd, &s);
                                }
                                s.last_interaction = Instant::now();
                            } else {
                                // Frontmost focused card clicked directly: Open directly!
                                let item_clone = s.item.clone();
                                drop(s);
                                if let LandingItem::Files { paths, .. } = item_clone {
                                    if let Some(path) = paths.get(clicked_idx) {
                                        open_file_directly(path);
                                        trigger_dismiss();
                                    }
                                }
                            }
                        } else {
                            // Non-image card clicked (URL, Text, or File)
                            let item_clone = s.item.clone();
                            drop(s);
                            match item_clone {
                                LandingItem::Url { url } => {
                                    open_url_in_browser(&url);
                                    trigger_dismiss();
                                }
                                LandingItem::Files { paths, .. } => {
                                    if let Some(first_path) = paths.first() {
                                        let ext = first_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                                        if ext == "pdf" || matches!(ext.as_str(), "mp4" | "mkv" | "mov" | "avi" | "webm") {
                                            open_file_directly(first_path);
                                        } else {
                                            show_file_in_explorer(first_path);
                                        }
                                    }
                                    trigger_dismiss();
                                }
                                LandingItem::Text { .. } => {
                                    trigger_dismiss();
                                }
                            }
                        }
                    }
                }
            }
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) & 0xFFFF) as i16;
            if let Some(state_arc) = get_active_state() {
                let mut s = state_arc.lock().unwrap();
                let prev_idx = s.focus_idx;
                if delta > 0 && s.focus_idx > 0 {
                    s.focus_idx -= 1;
                } else if delta < 0 && !s.images.is_empty() && s.focus_idx + 1 < s.images.len() {
                    s.focus_idx += 1;
                }
                if s.focus_idx != prev_idx {
                    let total_bytes = (s.canvas_w * s.canvas_h * 4) as usize;
                    let dib_slice = unsafe { std::slice::from_raw_parts_mut(s.dib_bits, total_bytes) };
                    s.cards = render_3d_spatial_scene(
                        s.mem_dc,
                        dib_slice,
                        s.canvas_w,
                        s.canvas_h,
                        &s.item,
                        &s.images,
                        s.focus_idx,
                    );
                    unsafe {
                        commit_layered_frame(hwnd, &s);
                    }
                    s.last_interaction = Instant::now();
                }
            }
            0
        }
        WM_KEYDOWN => {
            if wparam == VK_ESCAPE as usize {
                trigger_dismiss();
            } else if wparam == VK_RETURN as usize {
                if let Some(state_arc) = get_active_state() {
                    let s = state_arc.lock().unwrap();
                    let focus_idx = s.focus_idx;
                    let item_clone = s.item.clone();
                    drop(s);
                    match item_clone {
                        LandingItem::Url { url } => {
                            open_url_in_browser(&url);
                            trigger_dismiss();
                        }
                        LandingItem::Files { paths, .. } => {
                            if let Some(path) = paths.get(focus_idx) {
                                open_file_directly(path);
                            } else if let Some(first_path) = paths.first() {
                                open_file_directly(first_path);
                            }
                            trigger_dismiss();
                        }
                        _ => {
                            trigger_dismiss();
                        }
                    }
                }
            } else if wparam == VK_LEFT as usize {
                if let Some(state_arc) = get_active_state() {
                    let mut s = state_arc.lock().unwrap();
                    if s.focus_idx > 0 {
                        s.focus_idx -= 1;
                        let total_bytes = (s.canvas_w * s.canvas_h * 4) as usize;
                        let dib_slice = unsafe { std::slice::from_raw_parts_mut(s.dib_bits, total_bytes) };
                        s.cards = render_3d_spatial_scene(
                            s.mem_dc,
                            dib_slice,
                            s.canvas_w,
                            s.canvas_h,
                            &s.item,
                            &s.images,
                            s.focus_idx,
                        );
                        unsafe {
                            commit_layered_frame(hwnd, &s);
                        }
                        s.last_interaction = Instant::now();
                    }
                }
            } else if wparam == VK_RIGHT as usize {
                if let Some(state_arc) = get_active_state() {
                    let mut s = state_arc.lock().unwrap();
                    if !s.images.is_empty() && s.focus_idx + 1 < s.images.len() {
                        s.focus_idx += 1;
                        let total_bytes = (s.canvas_w * s.canvas_h * 4) as usize;
                        let dib_slice = unsafe { std::slice::from_raw_parts_mut(s.dib_bits, total_bytes) };
                        s.cards = render_3d_spatial_scene(
                            s.mem_dc,
                            dib_slice,
                            s.canvas_w,
                            s.canvas_h,
                            &s.item,
                            &s.images,
                            s.focus_idx,
                        );
                        unsafe {
                            commit_layered_frame(hwnd, &s);
                        }
                        s.last_interaction = Instant::now();
                    }
                }
            }
            0
        }
        WM_CLOSE => {
            trigger_dismiss();
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_ANIM_TICK);
            KillTimer(hwnd, TIMER_KEY_CHECK);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ----------------------------------------------------------------------------
// Show Landing Setup
// ----------------------------------------------------------------------------

unsafe fn handle_show_landing(hwnd: HWND) {
    let payload = {
        let mut guard = PENDING_PAYLOAD.lock().unwrap();
        guard.take()
    };

    let p = match payload {
        Some(data) => data,
        None => return,
    };

    let (canvas_w, canvas_h) = (p.canvas_w, p.canvas_h);

    let mut work_area: RECT = std::mem::zeroed();
    SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        &mut work_area as *mut _ as *mut c_void,
        0,
    );

    let screen_w = work_area.right - work_area.left;
    let screen_h = work_area.bottom - work_area.top;

    // Centered in upper-center of screen (Tony Stark spatial HUD height)
    let target_x = work_area.left + (screen_w - canvas_w) / 2;
    let target_y = work_area.top + ((screen_h - canvas_h).max(40) / 4);

    // Starts completely above the top edge
    let start_y = work_area.top - canvas_h - 40;

    // Allocate 32-bit DIB section for UpdateLayeredWindow
    let hdc_screen = CreateCompatibleDC(0 as _);
    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = canvas_w;
    bmi.bmiHeader.biHeight = -canvas_h; // Top-down DIB
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB as u32;

    let mut bits_ptr: *mut c_void = std::ptr::null_mut();
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

    let dib_slice = std::slice::from_raw_parts_mut(
        bits_ptr as *mut u8,
        (canvas_w * canvas_h * 4) as usize,
    );

    // Render 3D Spatial Scene onto 32-bit ARGB surface once
    let initial_focus = if p.images.is_empty() { 0 } else { p.images.len() / 2 };
    let projected_cards = render_3d_spatial_scene(
        mem_dc,
        dib_slice,
        canvas_w,
        canvas_h,
        &p.item,
        &p.images,
        initial_focus,
    );

    let state = Arc::new(Mutex::new(LandingState {
        item: p.item,
        anim_state: AnimState::Entering,
        start_time: Instant::now(),
        last_interaction: Instant::now(),
        canvas_w,
        canvas_h,
        target_x,
        target_y,
        start_y,
        current_y: start_y,
        current_alpha: 0,
        is_dragging_candidate: false,
        drag_start_pt: POINT { x: 0, y: 0 },
        mem_dc,
        dib_bitmap,
        dib_bits: bits_ptr as *mut u8,
        cards: projected_cards,
        images: p.images,
        focus_idx: initial_focus,
        clicked_card_idx: None,
    }));

    {
        let mut guard = ACTIVE_STATE.lock().unwrap();
        // Clean up previous DIB if any
        if let Some(ref old_state) = *guard {
            let s = old_state.lock().unwrap();
            if !s.dib_bitmap.is_null() {
                DeleteObject(s.dib_bitmap as _);
            }
            if !s.mem_dc.is_null() {
                DeleteDC(s.mem_dc);
            }
        }
        *guard = Some(state);
    }

    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        target_x,
        start_y,
        canvas_w,
        canvas_h,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );

    ShowWindow(hwnd, SW_SHOWNOACTIVATE);

    // Play tactile spatial landing sound effect
    crate::http_server::play_arrival_chime();

    SetTimer(hwnd, TIMER_ANIM_TICK, 16, None);
    SetTimer(hwnd, TIMER_KEY_CHECK, 30, None);
}

// ----------------------------------------------------------------------------
// Animation Tick & Compositor Update
// ----------------------------------------------------------------------------

fn handle_anim_tick(hwnd: HWND) {
    let state_arc = match get_active_state() {
        Some(a) => a,
        None => return,
    };

    let mut s = state_arc.lock().unwrap();
    let elapsed = s.start_time.elapsed().as_secs_f32() * 1000.0;

    match s.anim_state {
        AnimState::Entering => {
            let progress = (elapsed / ANIM_DURATION_MS).min(1.0);

            // Downward sticky note spring bounce physics:
            let spring = 1.0 - (-5.2 * progress).exp() * (4.2 * progress).cos();
            let current_y = (s.start_y as f32 + (s.target_y - s.start_y) as f32 * spring) as i32;
            s.current_y = current_y;

            let current_alpha = ((progress / 0.22).min(1.0) * 255.0) as u8;
            s.current_alpha = current_alpha;

            unsafe {
                commit_layered_frame(hwnd, &s);
            }

            if progress >= 1.0 {
                s.anim_state = AnimState::Sustained;
                s.last_interaction = Instant::now();
            }
        }
        AnimState::Sustained => {
            if s.last_interaction.elapsed().as_secs_f32() > TIMEOUT_AUTO_DISMISS_SECS {
                s.anim_state = AnimState::Exiting;
                s.start_time = Instant::now();
            }
        }
        AnimState::Exiting => {
            let progress = (elapsed / DISMISS_DURATION_MS).min(1.0);
            let ease = progress * progress;

            let current_y = (s.target_y as f32 - (24.0 * ease)) as i32;
            let current_alpha = ((1.0 - ease) * 255.0) as u8;

            s.current_y = current_y;
            s.current_alpha = current_alpha;

            unsafe {
                commit_layered_frame(hwnd, &s);
            }

            if progress >= 1.0 {
                s.anim_state = AnimState::Hidden;
                unsafe {
                    ShowWindow(hwnd, SW_HIDE);
                    KillTimer(hwnd, TIMER_ANIM_TICK);
                    KillTimer(hwnd, TIMER_KEY_CHECK);
                }
            }
        }
        AnimState::Hidden => {}
    }
}

unsafe fn commit_layered_frame(hwnd: HWND, s: &LandingState) {
    let mut pt_dst = POINT {
        x: s.target_x,
        y: s.current_y,
    };
    let mut size = SIZE {
        cx: s.canvas_w,
        cy: s.canvas_h,
    };
    let mut pt_src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: s.current_alpha,
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
}

fn handle_key_check(_hwnd: HWND) {
    let state_arc = match get_active_state() {
        Some(a) => a,
        None => return,
    };

    let s = state_arc.lock().unwrap();
    if s.anim_state != AnimState::Sustained {
        return;
    }

    if s.start_time.elapsed().as_secs_f32() < 0.6 {
        return;
    }

    let is_esc_down = unsafe { (GetAsyncKeyState(VK_ESCAPE as i32) as u16 & 0x8000) != 0 };
    let is_ctrl_down = unsafe { (GetAsyncKeyState(VK_CONTROL as i32) as u16 & 0x8000) != 0 };
    let is_v_down = unsafe { (GetAsyncKeyState(b'V' as i32) as u16 & 0x8000) != 0 };

    if is_esc_down || (is_ctrl_down && is_v_down) {
        drop(s);
        trigger_dismiss();
    }
}

pub fn trigger_dismiss() {
    if let Some(state_arc) = get_active_state() {
        let mut s = state_arc.lock().unwrap();
        if s.anim_state == AnimState::Entering || s.anim_state == AnimState::Sustained {
            s.anim_state = AnimState::Exiting;
            s.start_time = Instant::now();
        }
    }
}

fn get_active_state() -> Option<Arc<Mutex<LandingState>>> {
    let guard = ACTIVE_STATE.lock().unwrap();
    guard.clone()
}

// ----------------------------------------------------------------------------
// 3D Perspective Rasterizer Engine
// ----------------------------------------------------------------------------

struct CardConfig {
    image_idx: usize,
    w: f32,
    h: f32,
    yaw_deg: f32,
    pitch_deg: f32,
    roll_deg: f32,
    trans_x: f32,
    trans_y: f32,
    trans_z: f32,
    is_focused: bool,
}

fn render_3d_spatial_scene(
    hdc: HDC,
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    item: &LandingItem,
    images: &[LoadedImage],
    focus_idx: usize,
) -> Vec<ProjectedCard> {
    // 1. Clear buffer to complete transparency (RGBA 0, 0, 0, 0)
    buffer.fill(0);

    let mut projected_cards = Vec::new();

    if images.is_empty() {
        // Fallback for non-image or text items: Liquid Glass Floating Card
        render_floating_glass_card(hdc, buffer, canvas_w, canvas_h, item, &mut projected_cards);
        return projected_cards;
    }

    let count = images.len();
    let cx = (canvas_w as f32) / 2.0;
    let cy = (canvas_h as f32) / 2.0;
    let focal = 900.0f32;
    let camera_dist = 900.0f32;

    // 2. Define 3D Card Layouts matching the Reel & Quiet Forms Fanned Arc
    let mut card_configs: Vec<CardConfig> = match count {
        1 => {
            let img = &images[0];
            let aspect = img.w as f32 / img.h.max(1) as f32;
            let (cw, ch) = if aspect > 1.2 {
                (540.0, 540.0 / aspect)
            } else if aspect < 0.8 {
                (480.0 * aspect, 480.0)
            } else {
                (440.0, 440.0)
            };
            vec![CardConfig {
                image_idx: 0,
                w: cw,
                h: ch,
                yaw_deg: 0.0,
                pitch_deg: 0.0,
                roll_deg: 0.0,
                trans_x: 0.0,
                trans_y: 0.0,
                trans_z: -20.0,
                is_focused: true,
            }]
        }
        2 => {
            let active = focus_idx.min(1);
            let (z0, z1, y0, y1, roll0, roll1) = if active == 0 {
                (-30.0, 20.0, -14.0, 10.0, -4.5, 4.5)
            } else {
                (20.0, -30.0, 10.0, -14.0, -4.5, 4.5)
            };
            vec![
                CardConfig {
                    image_idx: 0,
                    w: 360.0,
                    h: 460.0,
                    yaw_deg: 0.0,
                    pitch_deg: 0.0,
                    roll_deg: roll0,
                    trans_x: -115.0,
                    trans_y: y0,
                    trans_z: z0,
                    is_focused: active == 0,
                },
                CardConfig {
                    image_idx: 1,
                    w: 360.0,
                    h: 460.0,
                    yaw_deg: 0.0,
                    pitch_deg: 0.0,
                    roll_deg: roll1,
                    trans_x: 115.0,
                    trans_y: y1,
                    trans_z: z1,
                    is_focused: active == 1,
                },
            ]
        }
        _ => {
            // Interactive 3D Fanned Arc Deck (Quiet forms style: hand-of-cards curve, roll rotation, zero harsh yaw)
            let active = focus_idx.min(count - 1);
            let mut list = Vec::new();

            for i in 0..count {
                let diff = i as isize - active as isize;
                let abs_diff = diff.abs() as f32;

                if diff == 0 {
                    // Center Hero Card: elevated at arc apex, straight forward, frontmost
                    list.push(CardConfig {
                        image_idx: i,
                        w: 380.0,
                        h: 480.0,
                        yaw_deg: 0.0,
                        pitch_deg: 0.0,
                        roll_deg: 0.0,
                        trans_x: 0.0,
                        trans_y: -24.0, // Elevated up at apex of arc
                        trans_z: -40.0, // Frontmost
                        is_focused: true,
                    });
                } else {
                    // Side Cards: gentle roll rotation, curving downward along rainbow arc
                    let roll = (diff as f32 * 5.2).clamp(-18.0, 18.0);
                    // Very subtle yaw just for tiny physical depth (facing slightly towards center)
                    let yaw = (diff as f32 * -1.8).clamp(-5.0, 5.0);
                    // Smooth curved arch: drops down gracefully as distance increases
                    let trans_y = -24.0 + abs_diff * 14.0 + (abs_diff * abs_diff * 1.5);
                    // Overlapping horizontal spread (~145px visible per card)
                    let trans_x = diff as f32 * 145.0;
                    // Stacking depth: further cards are pushed behind
                    let trans_z = 15.0 + abs_diff * 20.0;

                    // Slightly scale down side cards
                    let scale = (1.0 - abs_diff * 0.04).max(0.85);

                    list.push(CardConfig {
                        image_idx: i,
                        w: 380.0 * scale,
                        h: 480.0 * scale,
                        yaw_deg: yaw,
                        pitch_deg: 0.0,
                        roll_deg: roll,
                        trans_x,
                        trans_y,
                        trans_z,
                        is_focused: false,
                    });
                }
            }
            list
        }
    };

    // 3. Sort cards by depth Z (back to front painter's algorithm)
    card_configs.sort_by(|a, b| b.trans_z.partial_cmp(&a.trans_z).unwrap());

    // 4. Project 3D Coordinates & Rasterize
    for cfg in &card_configs {
        let corners_3d = compute_3d_corners(cfg.w, cfg.h, cfg.yaw_deg, cfg.pitch_deg, cfg.roll_deg, cfg.trans_x, cfg.trans_y, cfg.trans_z, camera_dist);
        let screen_corners = project_corners(&corners_3d, cx, cy, focal);

        projected_cards.push(ProjectedCard {
            corners: screen_corners,
            z_depth: cfg.trans_z,
            image_idx: cfg.image_idx,
        });

        // Soft ambient drop shadow underneath card
        rasterize_drop_shadow(buffer, canvas_w, canvas_h, &screen_corners, cfg.is_focused);

        // Rasterize card image with 3D homography & rounded corners
        let img = &images[cfg.image_idx];
        rasterize_perspective_card(buffer, canvas_w, canvas_h, &screen_corners, cfg.w, cfg.h, img, cfg.is_focused);

        // Visual overlays for Video and PDF cards
        match img.visual_type {
            CardVisualType::Video => {
                render_video_card_overlay(hdc, buffer, canvas_w, canvas_h, &screen_corners, cfg.is_focused);
            }
            CardVisualType::Pdf => {
                render_pdf_card_overlay(hdc, buffer, canvas_w, canvas_h, &screen_corners, cfg.is_focused);
            }
            CardVisualType::Photo => {}
        }
    }

    // Badge for extra photos if batch > 5
    if let LandingItem::Files { count: total, .. } = item {
        if *total > images.len() && !projected_cards.is_empty() {
            render_more_photos_badge(buffer, canvas_w, canvas_h, &projected_cards[0].corners, *total - images.len());
        }
    }

    // Render pagination dots under Fanned Arc if multiple images
    if images.len() > 1 {
        let active = focus_idx.min(images.len() - 1);
        render_pagination_dots(buffer, canvas_w, canvas_h, images.len(), active, cx, cy);
    }

    projected_cards
}

struct Point3D {
    x: f32,
    y: f32,
    z: f32,
}

fn compute_3d_corners(
    w: f32,
    h: f32,
    yaw_deg: f32,
    pitch_deg: f32,
    roll_deg: f32,
    tx: f32,
    ty: f32,
    tz: f32,
    camera_dist: f32,
) -> [Point3D; 4] {
    let hw = w / 2.0;
    let hh = h / 2.0;
    let local = [
        (-hw, -hh), // TL
        (hw, -hh),  // TR
        (hw, hh),   // BR
        (-hw, hh),  // BL
    ];

    let yaw = yaw_deg.to_radians();
    let pitch = pitch_deg.to_radians();
    let roll = roll_deg.to_radians();

    let cos_y = yaw.cos();
    let sin_y = yaw.sin();
    let cos_p = pitch.cos();
    let sin_p = pitch.sin();
    let cos_r = roll.cos();
    let sin_r = roll.sin();

    let mut result = [
        Point3D { x: 0.0, y: 0.0, z: 0.0 },
        Point3D { x: 0.0, y: 0.0, z: 0.0 },
        Point3D { x: 0.0, y: 0.0, z: 0.0 },
        Point3D { x: 0.0, y: 0.0, z: 0.0 },
    ];

    for (i, &(lx, ly)) in local.iter().enumerate() {
        // Yaw (Y-axis)
        let x1 = lx * cos_y;
        let z1 = -lx * sin_y;
        let y1 = ly;

        // Pitch (X-axis)
        let y2 = y1 * cos_p - z1 * sin_p;
        let z2 = y1 * sin_p + z1 * cos_p;
        let x2 = x1;

        // Roll (Z-axis)
        let x3 = x2 * cos_r - y2 * sin_r;
        let y3 = x2 * sin_r + y2 * cos_r;
        let z3 = z2;

        result[i] = Point3D {
            x: x3 + tx,
            y: y3 + ty,
            z: z3 + tz + camera_dist,
        };
    }

    result
}

fn project_corners(pts: &[Point3D; 4], cx: f32, cy: f32, focal: f32) -> [Point2D; 4] {
    let mut out = [
        Point2D { x: 0.0, y: 0.0 },
        Point2D { x: 0.0, y: 0.0 },
        Point2D { x: 0.0, y: 0.0 },
        Point2D { x: 0.0, y: 0.0 },
    ];

    for i in 0..4 {
        let z = pts[i].z.max(10.0);
        let scale = focal / z;
        out[i] = Point2D {
            x: cx + pts[i].x * scale,
            y: cy + pts[i].y * scale,
        };
    }

    out
}

// ----------------------------------------------------------------------------
// Inverse Projective Homography & Bilinear Texture Mapping
// ----------------------------------------------------------------------------

fn compute_homography_inv(p0: Point2D, p1: Point2D, p2: Point2D, p3: Point2D) -> Option<[f64; 9]> {
    let dx1 = (p1.x - p2.x) as f64;
    let dx2 = (p3.x - p2.x) as f64;
    let sx = (p0.x - p1.x + p2.x - p3.x) as f64;
    let dy1 = (p1.y - p2.y) as f64;
    let dy2 = (p3.y - p2.y) as f64;
    let sy = (p0.y - p1.y + p2.y - p3.y) as f64;

    let det = dx1 * dy2 - dx2 * dy1;
    if det.abs() < 1e-7 {
        return None;
    }

    let g = (sx * dy2 - sy * dx2) / det;
    let h = (dx1 * sy - dy1 * sx) / det;
    let a = (p1.x - p0.x) as f64 + g * p1.x as f64;
    let b = (p3.x - p0.x) as f64 + h * p3.x as f64;
    let c = p0.x as f64;
    let d = (p1.y - p0.y) as f64 + g * p1.y as f64;
    let e = (p3.y - p0.y) as f64 + h * p3.y as f64;
    let f = p0.y as f64;

    let inv_a = e - f * h;
    let inv_b = c * h - b;
    let inv_c = b * f - c * e;
    let inv_d = f * g - d;
    let inv_e = a - c * g;
    let inv_f = c * d - a * f;
    let inv_g = d * h - e * g;
    let inv_h = b * g - a * h;
    let inv_i = a * e - b * d;

    let m_det = a * inv_a + b * inv_d + c * inv_g;
    if m_det.abs() < 1e-9 {
        return None;
    }

    let inv_det = 1.0 / m_det;
    Some([
        inv_a * inv_det, inv_b * inv_det, inv_c * inv_det,
        inv_d * inv_det, inv_e * inv_det, inv_f * inv_det,
        inv_g * inv_det, inv_h * inv_det, inv_i * inv_det,
    ])
}

fn rasterize_perspective_card(
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    corners: &[Point2D; 4],
    card_w: f32,
    card_h: f32,
    img: &LoadedImage,
    is_focused: bool,
) {
    let inv = match compute_homography_inv(corners[0], corners[1], corners[2], corners[3]) {
        Some(m) => m,
        None => return,
    };

    let min_x = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min).floor().max(0.0) as i32;
    let max_x = corners.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max).ceil().min((canvas_w - 1) as f32) as i32;
    let min_y = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min).floor().max(0.0) as i32;
    let max_y = corners.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max).ceil().min((canvas_h - 1) as f32) as i32;

    let r_u = (22.0 / card_w as f64).min(0.22);
    let r_v = (22.0 / card_h as f64).min(0.22);

    let border_u = if is_focused { 3.2 / card_w as f64 } else { 2.4 / card_w as f64 };
    let border_v = if is_focused { 3.2 / card_h as f64 } else { 2.4 / card_h as f64 };

    let src_w = img.w as usize;
    let src_h = img.h as usize;

    let base_light = if is_focused { 1.0f32 } else { 0.88f32 };

    for y in min_y..=max_y {
        let row_offset = (y as usize) * (canvas_w as usize) * 4;
        for x in min_x..=max_x {
            let x_f = x as f64;
            let y_f = y as f64;

            let w_denom = inv[6] * x_f + inv[7] * y_f + inv[8];
            if w_denom.abs() < 1e-9 {
                continue;
            }

            let u = (inv[0] * x_f + inv[1] * y_f + inv[2]) / w_denom;
            let v = (inv[3] * x_f + inv[4] * y_f + inv[5]) / w_denom;

            if u < 0.0 || u > 1.0 || v < 0.0 || v > 1.0 {
                continue;
            }

            // Rounded corner test
            let mut is_corner = false;
            let mut corner_dist_sq = 0.0;

            if u < r_u && v < r_v {
                let du = (r_u - u) / r_u;
                let dv = (r_v - v) / r_v;
                corner_dist_sq = du * du + dv * dv;
                is_corner = true;
            } else if u > 1.0 - r_u && v < r_v {
                let du = (u - (1.0 - r_u)) / r_u;
                let dv = (r_v - v) / r_v;
                corner_dist_sq = du * du + dv * dv;
                is_corner = true;
            } else if u > 1.0 - r_u && v > 1.0 - r_v {
                let du = (u - (1.0 - r_u)) / r_u;
                let dv = (v - (1.0 - r_v)) / r_v;
                corner_dist_sq = du * du + dv * dv;
                is_corner = true;
            } else if u < r_u && v > 1.0 - r_v {
                let du = (r_u - u) / r_u;
                let dv = (v - (1.0 - r_v)) / r_v;
                corner_dist_sq = du * du + dv * dv;
                is_corner = true;
            }

            if is_corner && corner_dist_sq > 1.0 {
                continue;
            }

            // Crisp solid white polaroid edge (Quiet forms style!)
            let is_border = u < border_u
                || u > 1.0 - border_u
                || v < border_v
                || v > 1.0 - border_v
                || (is_corner && corner_dist_sq > 0.70);

            let (b, g, r) = if is_border {
                (255u8, 255u8, 255u8) // Pure solid white border
            } else {
                // Bilinear sampling
                let sx_f = u * (src_w - 1) as f64;
                let sy_f = v * (src_h - 1) as f64;
                let sx0 = sx_f.floor() as usize;
                let sy0 = sy_f.floor() as usize;
                let sx1 = (sx0 + 1).min(src_w - 1);
                let sy1 = (sy0 + 1).min(src_h - 1);
                let fx = (sx_f - sx0 as f64) as f32;
                let fy = (sy_f - sy0 as f64) as f32;

                let get_p = |gx: usize, gy: usize| -> (u8, u8, u8) {
                    let idx = (gy * src_w + gx) * 4;
                    (img.bgra[idx], img.bgra[idx + 1], img.bgra[idx + 2])
                };

                let p00 = get_p(sx0, sy0);
                let p10 = get_p(sx1, sy0);
                let p01 = get_p(sx0, sy1);
                let p11 = get_p(sx1, sy1);

                let interp = |c00: u8, c10: u8, c01: u8, c11: u8| -> u8 {
                    let top = (c00 as f32) * (1.0 - fx) + (c10 as f32) * fx;
                    let bot = (c01 as f32) * (1.0 - fx) + (c11 as f32) * fx;
                    (top * (1.0 - fy) + bot * fy).round().min(255.0) as u8
                };

                let raw_b = interp(p00.0, p10.0, p01.0, p11.0);
                let raw_g = interp(p00.1, p10.1, p01.1, p11.1);
                let raw_r = interp(p00.2, p10.2, p01.2, p11.2);

                (
                    ((raw_b as f32) * base_light).min(255.0) as u8,
                    ((raw_g as f32) * base_light).min(255.0) as u8,
                    ((raw_r as f32) * base_light).min(255.0) as u8,
                )
            };

            let p_idx = row_offset + (x as usize) * 4;
            buffer[p_idx + 0] = b;
            buffer[p_idx + 1] = g;
            buffer[p_idx + 2] = r;
            buffer[p_idx + 3] = 255; // Fully opaque card pixel
        }
    }
}

// ----------------------------------------------------------------------------
// Soft Drop Shadows
// ----------------------------------------------------------------------------

fn rasterize_drop_shadow(
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    corners: &[Point2D; 4],
    is_focused: bool,
) {
    let offset_y = if is_focused { 22.0f32 } else { 16.0f32 };
    let margin = if is_focused { 28.0f32 } else { 20.0f32 };
    let max_alpha = if is_focused { 115.0f32 } else { 70.0f32 };

    let min_x = (corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min) - margin).floor().max(0.0) as i32;
    let max_x = (corners.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max) + margin).ceil().min((canvas_w - 1) as f32) as i32;
    let min_y = (corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min) + offset_y - margin).floor().max(0.0) as i32;
    let max_y = (corners.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max) + offset_y + margin).ceil().min((canvas_h - 1) as f32) as i32;

    let center_x = (corners[0].x + corners[1].x + corners[2].x + corners[3].x) / 4.0;
    let center_y = (corners[0].y + corners[1].y + corners[2].y + corners[3].y) / 4.0 + offset_y;

    let max_dist = ((corners[2].x - center_x).powi(2) + (corners[2].y + offset_y - center_y).powi(2)).sqrt() + margin;

    for y in min_y..=max_y {
        let row_offset = (y as usize) * (canvas_w as usize) * 4;
        let dy = y as f32 - center_y;
        for x in min_x..=max_x {
            let dx = x as f32 - center_x;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist < max_dist {
                let falloff = 1.0 - (dist / max_dist).min(1.0);
                let shadow_alpha = (falloff * falloff * max_alpha) as u8;

                let p_idx = row_offset + (x as usize) * 4;
                if buffer[p_idx + 3] < 255 {
                    buffer[p_idx + 0] = 0;
                    buffer[p_idx + 1] = 0;
                    buffer[p_idx + 2] = 0;
                    buffer[p_idx + 3] = buffer[p_idx + 3].max(shadow_alpha);
                }
            }
        }
    }
}

// ----------------------------------------------------------------------------
// Cover Flow Pagination Dots
// ----------------------------------------------------------------------------

fn render_pagination_dots(
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    count: usize,
    active_idx: usize,
    cx: f32,
    cy: f32,
) {
    let total = count.min(12);
    if total <= 1 {
        return;
    }
    let dot_spacing = 16.0f32;
    let start_x = cx - ((total as f32 - 1.0) * dot_spacing) / 2.0;
    let y_center = cy + 252.0;

    for i in 0..total {
        let dot_cx = start_x + (i as f32) * dot_spacing;
        let is_active = i == active_idx;
        let radius = if is_active { 4.5f32 } else { 3.0f32 };
        let (b, g, r, a) = if is_active {
            (0u8, 85u8, 255u8, 240u8) // Bright orange
        } else {
            (160u8, 170u8, 185u8, 140u8) // Soft translucent slate
        };

        let min_x = (dot_cx - radius - 1.0).floor().max(0.0) as i32;
        let max_x = (dot_cx + radius + 1.0).ceil().min((canvas_w - 1) as f32) as i32;
        let min_y = (y_center - radius - 1.0).floor().max(0.0) as i32;
        let max_y = (y_center + radius + 1.0).ceil().min((canvas_h - 1) as f32) as i32;

        for y in min_y..=max_y {
            let row = (y as usize) * (canvas_w as usize) * 4;
            let dy = y as f32 - y_center;
            for x in min_x..=max_x {
                let dx = x as f32 - dot_cx;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= radius {
                    let edge_alpha = (1.0 - (dist - (radius - 1.0)).max(0.0)).min(1.0);
                    let final_a = (a as f32 * edge_alpha) as u8;
                    let p_idx = row + (x as usize) * 4;
                    buffer[p_idx + 0] = b;
                    buffer[p_idx + 1] = g;
                    buffer[p_idx + 2] = r;
                    buffer[p_idx + 3] = buffer[p_idx + 3].max(final_a);
                }
            }
        }
    }
}

// ----------------------------------------------------------------------------
// "+N more" Batch Badge
// ----------------------------------------------------------------------------

fn render_more_photos_badge(
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    card_corners: &[Point2D; 4],
    extra_count: usize,
) {
    let center_x = (card_corners[1].x + card_corners[2].x) / 2.0;
    let center_y = (card_corners[1].y + card_corners[2].y) / 2.0;

    let badge_w = 42;
    let badge_h = 24;
    let bx0 = (center_x as i32 - badge_w / 2).max(0);
    let by0 = (center_y as i32 - badge_h / 2).max(0);
    let bx1 = (bx0 + badge_w).min(canvas_w - 1);
    let by1 = (by0 + badge_h).min(canvas_h - 1);

    for y in by0..=by1 {
        let row = (y as usize) * (canvas_w as usize) * 4;
        for x in bx0..=bx1 {
            let p_idx = row + (x as usize) * 4;
            // Translucent dark pill badge
            buffer[p_idx + 0] = 20;
            buffer[p_idx + 1] = 20;
            buffer[p_idx + 2] = 20;
            buffer[p_idx + 3] = 220;
        }
    }
    let _ = extra_count; // Number can be drawn if GDI font is hooked
}

// ----------------------------------------------------------------------------
// 3D Card Overlays (Video Play Button & PDF / Video Badges)
// ----------------------------------------------------------------------------

fn render_video_card_overlay(
    hdc: HDC,
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    corners: &[Point2D; 4],
    is_focused: bool,
) {
    // 1. Center coordinates of the projected card
    let cx = (corners[0].x + corners[1].x + corners[2].x + corners[3].x) / 4.0;
    let cy = (corners[0].y + corners[1].y + corners[2].y + corners[3].y) / 4.0;

    // 2. Circular Frosted Acrylic Play Button (Apple TV style)
    let radius = if is_focused { 28.0f32 } else { 22.0f32 };
    let min_x = (cx - radius - 2.0).floor().max(0.0) as i32;
    let max_x = (cx + radius + 2.0).ceil().min((canvas_w - 1) as f32) as i32;
    let min_y = (cy - radius - 2.0).floor().max(0.0) as i32;
    let max_y = (cy + radius + 2.0).ceil().min((canvas_h - 1) as f32) as i32;

    for y in min_y..=max_y {
        let row = (y as usize) * (canvas_w as usize) * 4;
        let dy = y as f32 - cy;
        for x in min_x..=max_x {
            let dx = x as f32 - cx;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                let p_idx = row + (x as usize) * 4;
                let is_rim = dist >= (radius - 1.5);

                if is_rim {
                    // 1.5px Glass White Rim
                    let blend_a = 0.70f32;
                    let (cr, cg, cb) = (255.0, 255.0, 255.0);
                    buffer[p_idx + 0] = (buffer[p_idx + 0] as f32 * (1.0 - blend_a) + cb * blend_a) as u8;
                    buffer[p_idx + 1] = (buffer[p_idx + 1] as f32 * (1.0 - blend_a) + cg * blend_a) as u8;
                    buffer[p_idx + 2] = (buffer[p_idx + 2] as f32 * (1.0 - blend_a) + cr * blend_a) as u8;
                    buffer[p_idx + 3] = 255;
                } else {
                    // Frosted Obsidian Black Center (Slate 900: 15, 23, 42 with 75% opacity)
                    let blend_a = 0.75f32;
                    let (cr, cg, cb) = (15.0, 23.0, 42.0);
                    buffer[p_idx + 0] = (buffer[p_idx + 0] as f32 * (1.0 - blend_a) + cb * blend_a) as u8;
                    buffer[p_idx + 1] = (buffer[p_idx + 1] as f32 * (1.0 - blend_a) + cg * blend_a) as u8;
                    buffer[p_idx + 2] = (buffer[p_idx + 2] as f32 * (1.0 - blend_a) + cr * blend_a) as u8;
                    buffer[p_idx + 3] = 255;
                }
            }
        }
    }

    // 3. Crisp Equilateral White Triangle ▶ pointing right
    let tri_h = if is_focused { 18.0f32 } else { 14.0f32 };
    let tri_w = tri_h * 0.866;
    let tri_x0 = cx - (tri_w / 3.0) + 1.5;
    let tri_x1 = tri_x0 + tri_w;
    let tri_y0 = cy - (tri_h / 2.0);
    let tri_y1 = cy + (tri_h / 2.0);

    let t_min_x = tri_x0.floor().max(0.0) as i32;
    let t_max_x = tri_x1.ceil().min((canvas_w - 1) as f32) as i32;
    let t_min_y = tri_y0.floor().max(0.0) as i32;
    let t_max_y = tri_y1.ceil().min((canvas_h - 1) as f32) as i32;

    for y in t_min_y..=t_max_y {
        let y_f = y as f32;
        if y_f < tri_y0 || y_f > tri_y1 { continue; }
        let dist_y = (y_f - cy).abs();
        let max_dx_at_y = tri_w * (1.0 - dist_y / (tri_h / 2.0));
        let x_edge = tri_x0 + max_dx_at_y;

        let row = (y as usize) * (canvas_w as usize) * 4;
        for x in t_min_x..=t_max_x {
            let x_f = x as f32;
            if x_f >= tri_x0 && x_f <= x_edge {
                let p_idx = row + (x as usize) * 4;
                buffer[p_idx + 0] = 255;
                buffer[p_idx + 1] = 255;
                buffer[p_idx + 2] = 255;
                buffer[p_idx + 3] = 255;
            }
        }
    }

    // 4. Sleek Frosted Top-Left Badge: [ 🎬 VIDEO ]
    let bx0 = (corners[0].x + 14.0).max(0.0) as i32;
    let by0 = (corners[0].y + 14.0).max(0.0) as i32;
    let bx1 = (bx0 + 74).min(canvas_w - 1);
    let by1 = (by0 + 24).min(canvas_h - 1);

    fill_rounded_box(
        buffer, canvas_w, canvas_h,
        bx0, by0, bx1, by1, 8,
        0x0F172A, 220,
        0x475569, 240,
    );

    unsafe {
        let badge_font = create_gdi_font(10, true, false);
        draw_glass_text(
            hdc, buffer, canvas_w, canvas_h,
            "🎬 VIDEO", badge_font, 0x00FFFFFF,
            RECT { left: bx0, top: by0, right: bx1, bottom: by1 },
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        DeleteObject(badge_font as _);
    }
}

fn render_pdf_card_overlay(
    hdc: HDC,
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    corners: &[Point2D; 4],
    _is_focused: bool,
) {
    // Sleek Crimson Pill Badge: [ 📄 PDF ] in top-left
    let bx0 = (corners[0].x + 14.0).max(0.0) as i32;
    let by0 = (corners[0].y + 14.0).max(0.0) as i32;
    let bx1 = (bx0 + 64).min(canvas_w - 1);
    let by1 = (by0 + 24).min(canvas_h - 1);

    fill_rounded_box(
        buffer, canvas_w, canvas_h,
        bx0, by0, bx1, by1, 8,
        0xDC2626, 230,
        0xFCA5A5, 240,
    );

    unsafe {
        let badge_font = create_gdi_font(10, true, false);
        draw_glass_text(
            hdc, buffer, canvas_w, canvas_h,
            "📄 PDF", badge_font, 0x00FFFFFF,
            RECT { left: bx0, top: by0, right: bx1, bottom: by1 },
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        DeleteObject(badge_font as _);
    }
}

// ----------------------------------------------------------------------------
// Liquid Glass Floating Card (URLs, Text, and Non-Image Files)
// ----------------------------------------------------------------------------

fn open_url_in_browser(url: &str) {
    let url_wide: Vec<u16> = OsStr::new(url).encode_wide().chain(Some(0)).collect();
    let op_wide: Vec<u16> = OsStr::new("open").encode_wide().chain(Some(0)).collect();
    unsafe {
        ShellExecuteW(
            0 as HWND,
            op_wide.as_ptr(),
            url_wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

fn open_file_directly(path: &Path) {
    let path_wide: Vec<u16> = OsStr::new(path.as_os_str()).encode_wide().chain(Some(0)).collect();
    let op_wide: Vec<u16> = OsStr::new("open").encode_wide().chain(Some(0)).collect();
    unsafe {
        ShellExecuteW(
            0 as HWND,
            op_wide.as_ptr(),
            path_wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

fn show_file_in_explorer(path: &Path) {
    let param = format!("/select,\"{}\"", path.display());
    let param_wide: Vec<u16> = OsStr::new(&param).encode_wide().chain(Some(0)).collect();
    let explorer_wide: Vec<u16> = OsStr::new("explorer.exe").encode_wide().chain(Some(0)).collect();
    let op_wide: Vec<u16> = OsStr::new("open").encode_wide().chain(Some(0)).collect();
    unsafe {
        ShellExecuteW(
            0 as HWND,
            op_wide.as_ptr(),
            explorer_wide.as_ptr(),
            param_wide.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

fn extract_domain(raw_url: &str) -> String {
    let mut s = raw_url.trim();
    if let Some(rest) = s.strip_prefix("https://") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix("http://") {
        s = rest;
    }
    let mut domain = s.split(['/', '?', '#', ':']).next().unwrap_or(s);
    if let Some(rest) = domain.strip_prefix("www.") {
        domain = rest;
    }
    if domain.is_empty() {
        "Link".to_string()
    } else {
        domain.to_string()
    }
}

fn format_size(bytes: u64) -> String {
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

unsafe fn create_gdi_font(size: i32, bold: bool, monospace: bool) -> HFONT {
    let font_name: Vec<u16> = if monospace {
        OsStr::new("Consolas").encode_wide().chain(Some(0)).collect()
    } else {
        OsStr::new("Segoe UI").encode_wide().chain(Some(0)).collect()
    };
    let weight = if bold { 700 } else { 400 };
    CreateFontW(
        -size,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        1,
        0,
        0,
        5,
        0,
        font_name.as_ptr(),
    )
}

unsafe fn draw_glass_text(
    hdc: HDC,
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    text: &str,
    font: HFONT,
    rgb_color: u32,
    mut rc: RECT,
    flags: u32,
) {
    let old_font = SelectObject(hdc, font as _);
    SetBkMode(hdc, TRANSPARENT as _);
    let r = (rgb_color >> 16) & 0xFF;
    let g = (rgb_color >> 8) & 0xFF;
    let b = rgb_color & 0xFF;
    let colorref = r | (g << 8) | (b << 16);
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

    // Alpha fixup pass: GDI zeroes out the high byte of 32-bit DIB pixels it touched.
    // Any pixel in rc that has alpha == 0 was rendered by GDI - set alpha to 255.
    let x0 = rc.left.max(0).min(canvas_w - 1) as usize;
    let x1 = rc.right.max(0).min(canvas_w) as usize;
    let y0 = rc.top.max(0).min(canvas_h - 1) as usize;
    let y1 = rc.bottom.max(0).min(canvas_h) as usize;

    for y in y0..y1 {
        let row = y * (canvas_w as usize) * 4;
        for x in x0..x1 {
            let p_idx = row + x * 4;
            if buffer[p_idx + 3] == 0 {
                buffer[p_idx + 3] = 255;
            }
        }
    }
}

#[inline(always)]
fn set_bgra(buffer: &mut [u8], p_idx: usize, rgb: u32, a: u8) {
    let r = ((rgb >> 16) & 0xFF) as u8;
    let g = ((rgb >> 8) & 0xFF) as u8;
    let b = (rgb & 0xFF) as u8;
    buffer[p_idx + 0] = b;
    buffer[p_idx + 1] = g;
    buffer[p_idx + 2] = r;
    buffer[p_idx + 3] = a;
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
        if y < 0 || y >= canvas_h { continue; }
        let row = (y as usize) * (canvas_w as usize) * 4;
        for x in x0..x1 {
            if x < 0 || x >= canvas_w { continue; }

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

            let is_border = if in_corner {
                dist_from_corner_center >= (r_f - 1.2)
            } else {
                x == x0 || x == x1 - 1 || y == y0 || y == y1 - 1
            };

            let p_idx = row + (x as usize) * 4;
            if is_border {
                set_bgra(buffer, p_idx, border_rgb, border_a);
            } else {
                set_bgra(buffer, p_idx, bg_rgb, bg_a);
            }
        }
    }
}

fn render_ambient_card_shadow(
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    r: i32,
) {
    let shadow_pad = 12;
    let offset_y = 2;
    let r_f = r as f32;

    let min_x = (x0 - shadow_pad).max(0);
    let max_x = (x1 + shadow_pad).min(canvas_w);
    let min_y = (y0 - shadow_pad + offset_y).max(0);
    let max_y = (y1 + shadow_pad + offset_y).min(canvas_h);

    for y in min_y..max_y {
        let row = (y as usize) * (canvas_w as usize) * 4;
        let sy = y - offset_y;
        for x in min_x..max_x {
            let dx = if x < x0 + r {
                (x0 + r - x) as f32
            } else if x >= x1 - r {
                (x - (x1 - r - 1)) as f32
            } else {
                0.0f32
            };

            let dy = if sy < y0 + r {
                (y0 + r - sy) as f32
            } else if sy >= y1 - r {
                (sy - (y1 - r - 1)) as f32
            } else {
                0.0f32
            };

            let dist = if dx > 0.0 && dy > 0.0 {
                (dx * dx + dy * dy).sqrt() - r_f
            } else if dx > 0.0 {
                dx - r_f
            } else if dy > 0.0 {
                dy - r_f
            } else {
                -1.0f32
            };

            if dist > 0.0 && dist <= shadow_pad as f32 {
                let norm = 1.0 - (dist / shadow_pad as f32);
                let shadow_a = (norm * norm * 24.0) as u8; // Crisp, subtle, zero muddy smear
                let p_idx = row + (x as usize) * 4;
                if shadow_a > buffer[p_idx + 3] {
                    buffer[p_idx + 0] = 0;
                    buffer[p_idx + 1] = 0;
                    buffer[p_idx + 2] = 0;
                    buffer[p_idx + 3] = shadow_a;
                }
            }
        }
    }
}

fn render_floating_glass_card(
    hdc: HDC,
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    item: &LandingItem,
    projected_cards: &mut Vec<ProjectedCard>,
) {
    let card_w = 460;
    let card_h = match item {
        LandingItem::Text { .. } => 220,
        _ => 200,
    };
    let x0 = (canvas_w - card_w) / 2;
    let y0 = (canvas_h - card_h) / 2;
    let x1 = x0 + card_w;
    let y1 = y0 + card_h;
    let r = 18;

    // 1. Subtle, clean ambient drop shadow (no dirty bars)
    render_ambient_card_shadow(buffer, canvas_w, canvas_h, x0, y0, x1, y1, r);

    // 2. 100% Solid Pure Studio White Card (#FFFFFF) with crisp border (#E2E8F0)
    fill_rounded_box(
        buffer, canvas_w, canvas_h,
        x0, y0, x1, y1, r,
        0xFFFFFF, 255, // Pure White
        0xE2E8F0, 255, // Crisp 1.2px Border
    );

    // Register card in projected_cards for click / drag interaction!
    projected_cards.push(ProjectedCard {
        image_idx: 0,
        z_depth: 0.0,
        corners: [
            Point2D { x: x0 as f32, y: y0 as f32 },
            Point2D { x: x1 as f32, y: y0 as f32 },
            Point2D { x: x1 as f32, y: y1 as f32 },
            Point2D { x: x0 as f32, y: y1 as f32 },
        ],
    });

    unsafe {
        match item {
            LandingItem::Url { url } => {
                // Link Pill Badge: Soft Sky Blue (#EFF6FF), Sky Border (#BFDBFE)
                let pill_x0 = x0 + 24;
                let pill_y0 = y0 + 20;
                let pill_x1 = x0 + 94;
                let pill_y1 = y0 + 44;
                fill_rounded_box(
                    buffer, canvas_w, canvas_h,
                    pill_x0, pill_y0, pill_x1, pill_y1, 10,
                    0xEFF6FF, 255,
                    0xBFDBFE, 255,
                );

                let badge_font = create_gdi_font(11, true, false);
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    "🔗 LINK", badge_font, 0x001D4ED8, // Rich Royal Blue
                    RECT { left: pill_x0, top: pill_y0, right: pill_x1, bottom: pill_y1 },
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // Right Hint: "CLICK TO OPEN ↗"
                let hint_font = create_gdi_font(11, false, false);
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    "CLICK TO OPEN ↗", hint_font, 0x0064748B,
                    RECT { left: x1 - 180, top: y0 + 22, right: x1 - 24, bottom: y0 + 42 },
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // Extracted Domain Title (Deep charcoal black #0F172A)
                let domain = extract_domain(url);
                let title_font = create_gdi_font(22, true, false);
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    &domain, title_font, 0x000F172A,
                    RECT { left: x0 + 24, top: y0 + 52, right: x1 - 24, bottom: y0 + 86 },
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                );

                // Full URL Subtitle (Clear Hyperlink Blue #2563EB)
                let url_font = create_gdi_font(13, false, false);
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    url, url_font, 0x002563EB,
                    RECT { left: x0 + 24, top: y0 + 88, right: x1 - 24, bottom: y0 + 114 },
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                );

                // Action CTA Button: "Open in Browser ↗" (Flick Electric Orange #FF5722)
                let btn_x0 = x0 + 24;
                let btn_y0 = y0 + 134;
                let btn_x1 = x0 + 195;
                let btn_y1 = y0 + 172;
                fill_rounded_box(
                    buffer, canvas_w, canvas_h,
                    btn_x0, btn_y0, btn_x1, btn_y1, 12,
                    0xFF5722, 255, // Electric Orange
                    0xEA580C, 255, // Deep Orange Border
                );

                let btn_font = create_gdi_font(12, true, false);
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    "Open in Browser ↗", btn_font, 0x00FFFFFF,
                    RECT { left: btn_x0, top: btn_y0, right: btn_x1, bottom: btn_y1 },
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // Subtle footer hint: "Esc to close"
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    "Esc to close", hint_font, 0x0094A3B8,
                    RECT { left: x1 - 140, top: y0 + 144, right: x1 - 24, bottom: y0 + 166 },
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                DeleteObject(badge_font as _);
                DeleteObject(hint_font as _);
                DeleteObject(title_font as _);
                DeleteObject(url_font as _);
                DeleteObject(btn_font as _);
            }
            LandingItem::Text { preview, length } => {
                // Clipboard Pill Badge: Soft Mint (#ECFDF5), Emerald Border (#A7F3D0)
                let pill_x0 = x0 + 24;
                let pill_y0 = y0 + 20;
                let pill_x1 = x0 + 130;
                let pill_y1 = y0 + 44;
                fill_rounded_box(
                    buffer, canvas_w, canvas_h,
                    pill_x0, pill_y0, pill_x1, pill_y1, 10,
                    0xECFDF5, 255,
                    0xA7F3D0, 255,
                );

                let badge_font = create_gdi_font(11, true, false);
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    "📋 CLIPBOARD", badge_font, 0x00065F46, // Deep Emerald
                    RECT { left: pill_x0, top: pill_y0, right: pill_x1, bottom: pill_y1 },
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // Right Char Count
                let hint_font = create_gdi_font(11, false, false);
                let count_str = format!("{} CHARACTERS", length);
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    &count_str, hint_font, 0x0064748B,
                    RECT { left: x1 - 200, top: y0 + 22, right: x1 - 24, bottom: y0 + 42 },
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // Body Text (Deep charcoal #1E293B)
                let body_font = create_gdi_font(14, false, false);
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    preview, body_font, 0x001E293B,
                    RECT { left: x0 + 24, top: y0 + 56, right: x1 - 24, bottom: y0 + 145 },
                    DT_LEFT | DT_TOP | DT_WORDBREAK | DT_END_ELLIPSIS | DT_NOPREFIX,
                );

                // Footer Status
                let status_font = create_gdi_font(12, true, false);
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    "⚡ Copied to Windows Clipboard", status_font, 0x00059669,
                    RECT { left: x0 + 24, top: y0 + 165, right: x1 - 24, bottom: y0 + 195 },
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                DeleteObject(badge_font as _);
                DeleteObject(hint_font as _);
                DeleteObject(body_font as _);
                DeleteObject(status_font as _);
            }
            LandingItem::Files { primary_name, count, total_bytes, .. } => {
                let ext = primary_name.rsplit('.').next().unwrap_or("").to_lowercase();
                let is_pdf = ext == "pdf";
                let is_video = matches!(ext.as_str(), "mp4" | "mkv" | "mov" | "avi" | "webm");

                let (badge_text, badge_bg, badge_border, badge_text_col, btn_text, btn_bg, btn_border) = if is_pdf {
                    ("📄 PDF DOCUMENT", 0xFEF2F2, 0xFECACA, 0x00DC2626, "Open PDF ↗", 0xDC2626, 0xB91C1C)
                } else if is_video {
                    ("🎬 VIDEO FILE", 0xF5F3FF, 0xDDD6FE, 0x007C3AED, "Play Video ▶", 0x7C3AED, 0x6D28D9)
                } else {
                    ("📁 FILE READY", 0xF1F5F9, 0xCBD5E1, 0x00334155, "Show in Explorer ↗", 0x0F172A, 0x020617)
                };

                // Pill Badge
                let pill_x0 = x0 + 24;
                let pill_y0 = y0 + 20;
                let pill_x1 = x0 + if is_pdf { 145 } else if is_video { 125 } else { 120 };
                let pill_y1 = y0 + 44;
                fill_rounded_box(
                    buffer, canvas_w, canvas_h,
                    pill_x0, pill_y0, pill_x1, pill_y1, 10,
                    badge_bg, 255,
                    badge_border, 255,
                );

                let badge_font = create_gdi_font(11, true, false);
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    badge_text, badge_font, badge_text_col,
                    RECT { left: pill_x0, top: pill_y0, right: pill_x1, bottom: pill_y1 },
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // Right Size
                let hint_font = create_gdi_font(11, false, false);
                let size_str = format_size(*total_bytes);
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    &size_str, hint_font, 0x0064748B,
                    RECT { left: x1 - 180, top: y0 + 22, right: x1 - 24, bottom: y0 + 42 },
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // File Name Title (#0F172A)
                let title_font = create_gdi_font(20, true, false);
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    primary_name, title_font, 0x000F172A,
                    RECT { left: x0 + 24, top: y0 + 52, right: x1 - 24, bottom: y0 + 86 },
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                );

                // File Count Subtitle (#64748B)
                let sub_font = create_gdi_font(13, false, false);
                let count_str = if *count > 1 {
                    format!("{} files ready to paste or drag", count)
                } else if is_pdf {
                    format!("PDF Document • {}", size_str)
                } else if is_video {
                    format!("Video • {}", size_str)
                } else {
                    format!("File ready to paste or open • {}", size_str)
                };
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    &count_str, sub_font, 0x0064748B,
                    RECT { left: x0 + 24, top: y0 + 88, right: x1 - 24, bottom: y0 + 114 },
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                // Action Button
                let btn_x0 = x0 + 24;
                let btn_y0 = y0 + 134;
                let btn_x1 = x0 + if is_pdf || is_video { 160 } else { 195 };
                let btn_y1 = y0 + 172;
                fill_rounded_box(
                    buffer, canvas_w, canvas_h,
                    btn_x0, btn_y0, btn_x1, btn_y1, 12,
                    btn_bg, 255,
                    btn_border, 255,
                );

                let btn_font = create_gdi_font(12, true, false);
                draw_glass_text(
                    hdc, buffer, canvas_w, canvas_h,
                    btn_text, btn_font, 0x00FFFFFF,
                    RECT { left: btn_x0, top: btn_y0, right: btn_x1, bottom: btn_y1 },
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );

                DeleteObject(badge_font as _);
                DeleteObject(hint_font as _);
                DeleteObject(title_font as _);
                DeleteObject(sub_font as _);
                DeleteObject(btn_font as _);
            }
        }
    }
}

// ----------------------------------------------------------------------------
// Hit Testing (Point inside Quadrilateral)
// ----------------------------------------------------------------------------

fn point_in_quad(p: Point2D, quad: &[Point2D; 4]) -> bool {
    let mut inside = false;
    let mut j = 3;
    for i in 0..4 {
        if ((quad[i].y > p.y) != (quad[j].y > p.y))
            && (p.x < (quad[j].x - quad[i].x) * (p.y - quad[i].y) / (quad[j].y - quad[i].y) + quad[i].x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

// ----------------------------------------------------------------------------
// Native OLE DoDragDrop Implementation
// ----------------------------------------------------------------------------

fn execute_file_drag_drop(paths: &[PathBuf]) -> bool {
    let refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
    let hdrop_bytes = crate::clipboard::build_hdrop_bytes(&refs);

    let data_object = Box::new(SimpleDataObject {
        vtable: &DATA_OBJECT_VTBL,
        hdrop_bytes,
    });
    let drop_source = Box::new(SimpleDropSource {
        vtable: &DROP_SOURCE_VTBL,
    });

    let p_data_object = Box::into_raw(data_object) as *mut c_void;
    let p_drop_source = Box::into_raw(drop_source) as *mut c_void;

    let mut dw_effect: u32 = 0;
    let hr = unsafe {
        DoDragDrop(
            p_data_object as _,
            p_drop_source as _,
            DROPEFFECT_COPY | DROPEFFECT_MOVE,
            &mut dw_effect,
        )
    };

    unsafe {
        let _ = Box::from_raw(p_data_object as *mut SimpleDataObject);
        let _ = Box::from_raw(p_drop_source as *mut SimpleDropSource);
    }

    hr == DRAGDROP_S_DROP
}

const E_POINTER: HRESULT = 0x80004003u32 as i32;
#[allow(dead_code)]
const E_FAIL: HRESULT = 0x80004005u32 as i32;
const E_OUTOFMEMORY: HRESULT = 0x8007000Eu32 as i32;
const E_NOTIMPL: HRESULT = 0x80004001u32 as i32;
const DV_E_FORMATETC: HRESULT = 0x80040064u32 as i32;
const OLE_E_ADVISENOTSUPPORTED: HRESULT = 0x80040003u32 as i32;
const DATA_S_SAMEFORMATETC: HRESULT = 0x00040130;

#[repr(C)]
struct SimpleDropSource {
    vtable: *const IDropSourceVtbl,
}

#[repr(C)]
#[allow(non_snake_case)]
struct IDropSourceVtbl {
    QueryInterface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    AddRef: unsafe extern "system" fn(*mut c_void) -> u32,
    Release: unsafe extern "system" fn(*mut c_void) -> u32,
    QueryContinueDrag: unsafe extern "system" fn(*mut c_void, BOOL, u32) -> HRESULT,
    GiveFeedback: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
}

static DROP_SOURCE_VTBL: IDropSourceVtbl = IDropSourceVtbl {
    QueryInterface: ds_query_interface,
    AddRef: ds_add_ref,
    Release: ds_release,
    QueryContinueDrag: ds_query_continue_drag,
    GiveFeedback: ds_give_feedback,
};

unsafe extern "system" fn ds_query_interface(this: *mut c_void, _riid: *const GUID, ppv: *mut *mut c_void) -> HRESULT {
    if ppv.is_null() { return E_POINTER; }
    *ppv = this;
    S_OK
}

unsafe extern "system" fn ds_add_ref(_this: *mut c_void) -> u32 { 1 }
unsafe extern "system" fn ds_release(_this: *mut c_void) -> u32 { 1 }

unsafe extern "system" fn ds_query_continue_drag(_this: *mut c_void, f_escape_pressed: BOOL, grf_key_state: u32) -> HRESULT {
    if f_escape_pressed != 0 {
        DRAGDROP_S_CANCEL
    } else if (grf_key_state & MK_LBUTTON) == 0 {
        DRAGDROP_S_DROP
    } else {
        S_OK
    }
}

unsafe extern "system" fn ds_give_feedback(_this: *mut c_void, _dw_effect: u32) -> HRESULT {
    DRAGDROP_S_USEDEFAULTCURSORS
}

#[repr(C)]
struct SimpleDataObject {
    vtable: *const IDataObjectVtbl,
    hdrop_bytes: Vec<u8>,
}

#[repr(C)]
#[allow(non_snake_case)]
struct IDataObjectVtbl {
    QueryInterface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> HRESULT,
    AddRef: unsafe extern "system" fn(*mut c_void) -> u32,
    Release: unsafe extern "system" fn(*mut c_void) -> u32,
    GetData: unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut STGMEDIUM) -> HRESULT,
    GetDataHere: unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut STGMEDIUM) -> HRESULT,
    QueryGetData: unsafe extern "system" fn(*mut c_void, *const FORMATETC) -> HRESULT,
    GetCanonicalFormatEtc: unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut FORMATETC) -> HRESULT,
    SetData: unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut STGMEDIUM, BOOL) -> HRESULT,
    EnumFormatEtc: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> HRESULT,
    DAdvise: unsafe extern "system" fn(*mut c_void, *const FORMATETC, u32, *mut c_void, *mut u32) -> HRESULT,
    DUnadvise: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
    EnumDAdvise: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
}

static DATA_OBJECT_VTBL: IDataObjectVtbl = IDataObjectVtbl {
    QueryInterface: do_query_interface,
    AddRef: do_add_ref,
    Release: do_release,
    GetData: do_get_data,
    GetDataHere: do_get_data_here,
    QueryGetData: do_query_get_data,
    GetCanonicalFormatEtc: do_get_canonical_format_etc,
    SetData: do_set_data,
    EnumFormatEtc: do_enum_format_etc,
    DAdvise: do_d_advise,
    DUnadvise: do_d_unadvise,
    EnumDAdvise: do_enum_d_advise,
};

unsafe extern "system" fn do_query_interface(this: *mut c_void, _riid: *const GUID, ppv: *mut *mut c_void) -> HRESULT {
    if ppv.is_null() { return E_POINTER; }
    *ppv = this;
    S_OK
}

unsafe extern "system" fn do_add_ref(_this: *mut c_void) -> u32 { 1 }
unsafe extern "system" fn do_release(_this: *mut c_void) -> u32 { 1 }

unsafe extern "system" fn do_get_data(this: *mut c_void, pformatetc_in: *const FORMATETC, pmedium: *mut STGMEDIUM) -> HRESULT {
    if pformatetc_in.is_null() || pmedium.is_null() { return E_POINTER; }
    let fmt = &*pformatetc_in;
    if fmt.cfFormat != CF_HDROP || (fmt.tymed & TYMED_HGLOBAL as u32) == 0 {
        return DV_E_FORMATETC;
    }

    let obj = &*(this as *const SimpleDataObject);
    let bytes = &obj.hdrop_bytes;

    let hmem = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
    if hmem.is_null() { return E_OUTOFMEMORY; }

    let ptr = GlobalLock(hmem);
    if ptr.is_null() { return E_OUTOFMEMORY; }

    std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
    GlobalUnlock(hmem);

    (*pmedium).tymed = TYMED_HGLOBAL as u32;
    (*pmedium).u.hGlobal = hmem;
    (*pmedium).pUnkForRelease = std::ptr::null_mut();

    S_OK
}

unsafe extern "system" fn do_get_data_here(_this: *mut c_void, _pformatetc: *const FORMATETC, _pmedium: *mut STGMEDIUM) -> HRESULT {
    E_NOTIMPL
}

unsafe extern "system" fn do_query_get_data(_this: *mut c_void, pformatetc: *const FORMATETC) -> HRESULT {
    if pformatetc.is_null() { return E_POINTER; }
    let fmt = &*pformatetc;
    if fmt.cfFormat == CF_HDROP && (fmt.tymed & TYMED_HGLOBAL as u32) != 0 {
        S_OK
    } else {
        DV_E_FORMATETC
    }
}

unsafe extern "system" fn do_get_canonical_format_etc(_this: *mut c_void, _pformatetc_in: *const FORMATETC, pformatetc_out: *mut FORMATETC) -> HRESULT {
    if pformatetc_out.is_null() { return E_POINTER; }
    DATA_S_SAMEFORMATETC
}

unsafe extern "system" fn do_set_data(_this: *mut c_void, _pformatetc: *const FORMATETC, _pmedium: *mut STGMEDIUM, _frelease: BOOL) -> HRESULT {
    E_NOTIMPL
}

unsafe extern "system" fn do_enum_format_etc(_this: *mut c_void, _dw_direction: u32, ppenum_format_etc: *mut *mut c_void) -> HRESULT {
    if ppenum_format_etc.is_null() { return E_POINTER; }
    *ppenum_format_etc = std::ptr::null_mut();
    E_NOTIMPL
}

unsafe extern "system" fn do_d_advise(_this: *mut c_void, _pformatetc: *const FORMATETC, _advf: u32, _padv_sink: *mut c_void, pdw_connection: *mut u32) -> HRESULT {
    if !pdw_connection.is_null() { *pdw_connection = 0; }
    OLE_E_ADVISENOTSUPPORTED
}

unsafe extern "system" fn do_d_unadvise(_this: *mut c_void, _dw_connection: u32) -> HRESULT {
    OLE_E_ADVISENOTSUPPORTED
}

unsafe extern "system" fn do_enum_d_advise(_this: *mut c_void, ppenum_advise: *mut *mut c_void) -> HRESULT {
    if ppenum_advise.is_null() { return E_POINTER; }
    *ppenum_advise = std::ptr::null_mut();
    OLE_E_ADVISENOTSUPPORTED
}
