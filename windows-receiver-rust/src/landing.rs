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
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    DIB_RGB_COLORS, HBITMAP, HDC,
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
    GetAsyncKeyState, ReleaseCapture, VK_CONTROL, VK_ESCAPE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetSystemMetrics,
    GetWindowRect, KillTimer, LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassExW,
    SendMessageW, SetTimer, SetWindowPos, ShowWindow, SystemParametersInfoW, TranslateMessage,
    UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW, HTCAPTION, IDC_ARROW, SM_CXDRAG, SM_CYDRAG,
    SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA,
    WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_NCLBUTTONDOWN, WM_PAINT, WM_TIMER, WM_USER, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
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
    #[allow(dead_code)]
    dib_bits: *mut u8,
    cards: Vec<ProjectedCard>,
}

unsafe impl Send for LandingState {}
unsafe impl Sync for LandingState {}

struct PendingPayload {
    item: LandingItem,
    canvas_w: i32,
    canvas_h: i32,
    images: Vec<LoadedImage>,
}

struct LoadedImage {
    w: u32,
    h: u32,
    bgra: Vec<u8>,
}

static LANDING_HWND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static PENDING_PAYLOAD: Mutex<Option<PendingPayload>> = Mutex::new(None);
static ACTIVE_STATE: Mutex<Option<Arc<Mutex<LandingState>>>> = Mutex::new(None);

// ----------------------------------------------------------------------------
// Public Entry Points
// ----------------------------------------------------------------------------

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
    let mut image_count = 0usize;

    for p in &paths {
        if let Ok(meta) = std::fs::metadata(p) {
            total_bytes += meta.len();
        }
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif") {
            image_count += 1;
        }
    }

    let is_image = image_count == count;
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
// Scene Assembly & Dispatch
// ----------------------------------------------------------------------------

fn dispatch_landing(item: LandingItem) {
    let mut images = Vec::new();
    let (canvas_w, canvas_h) = match &item {
        LandingItem::Files { paths, is_image, count, .. } if *is_image && !paths.is_empty() => {
            if *count == 1 {
                if let Ok(img) = image::open(&paths[0]) {
                    let (ow, oh) = (img.width(), img.height());
                    let aspect = ow as f32 / oh.max(1) as f32;
                    let (target_w, target_h) = if aspect > 1.2 {
                        // Landscape photo
                        (560u32, (560.0 / aspect).round() as u32)
                    } else if aspect < 0.8 {
                        // Portrait photo
                        ((500.0 * aspect).round().max(320.0) as u32, 500u32)
                    } else {
                        // Square-ish
                        (460u32, 460u32)
                    };
                    let resized = img
                        .resize_to_fill(target_w, target_h, image::imageops::FilterType::Lanczos3)
                        .to_rgba8();
                    images.push(rgba_to_loaded(&resized));
                }
                (780, 640)
            } else if *count == 2 {
                for p in paths.iter().take(2) {
                    if let Ok(img) = image::open(p) {
                        let resized = img
                            .resize_to_fill(340, 460, image::imageops::FilterType::Lanczos3)
                            .to_rgba8();
                        images.push(rgba_to_loaded(&resized));
                    }
                }
                (880, 600)
            } else if *count == 3 {
                for p in paths.iter().take(3) {
                    if let Ok(img) = image::open(p) {
                        let resized = img
                            .resize_to_fill(320, 440, image::imageops::FilterType::Lanczos3)
                            .to_rgba8();
                        images.push(rgba_to_loaded(&resized));
                    }
                }
                (1040, 620)
            } else {
                // 4 or 5+ photos
                for p in paths.iter().take(5) {
                    if let Ok(img) = image::open(p) {
                        let resized = img
                            .resize_to_fill(300, 420, image::imageops::FilterType::Lanczos3)
                            .to_rgba8();
                        images.push(rgba_to_loaded(&resized));
                    }
                }
                (1180, 640)
            }
        }
        LandingItem::Files { .. } => (520, 360),
        LandingItem::Url { .. } => (480, 260),
        LandingItem::Text { .. } => (460, 240),
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

fn rgba_to_loaded(img: &image::RgbaImage) -> LoadedImage {
    let (w, h) = img.dimensions();
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for pixel in img.pixels() {
        let [r, g, b, a] = pixel.0;
        bgra.push(b);
        bgra.push(g);
        bgra.push(r);
        bgra.push(a);
    }
    LoadedImage { w, h, bgra }
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

                // Check if clicked inside any 3D card
                let clicked_card = s
                    .cards
                    .iter()
                    .any(|c| point_in_quad(Point2D { x, y }, &c.corners));

                if clicked_card {
                    s.is_dragging_candidate = true;
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
                s.is_dragging_candidate = false;
            }
            0
        }
        WM_KEYDOWN => {
            if wparam == VK_ESCAPE as usize {
                trigger_dismiss();
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
    let projected_cards = render_3d_spatial_scene(
        dib_slice,
        canvas_w,
        canvas_h,
        &p.item,
        &p.images,
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
}

fn render_3d_spatial_scene(
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    item: &LandingItem,
    images: &[LoadedImage],
) -> Vec<ProjectedCard> {
    // 1. Clear buffer to complete transparency (RGBA 0, 0, 0, 0)
    buffer.fill(0);

    let mut projected_cards = Vec::new();

    if images.is_empty() {
        // Fallback for non-image or text items: Clean floating card
        render_floating_doc_card(buffer, canvas_w, canvas_h, item);
        return projected_cards;
    }

    let count = images.len();
    let cx = (canvas_w as f32) / 2.0;
    let cy = (canvas_h as f32) / 2.0;
    let focal = 850.0f32;
    let camera_dist = 850.0f32;

    // 2. Define 3D Card Layouts matching the Reel
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
                pitch_deg: -1.0,
                roll_deg: 0.0,
                trans_x: 0.0,
                trans_y: 0.0,
                trans_z: -20.0,
            }]
        }
        2 => {
            vec![
                CardConfig {
                    image_idx: 0,
                    w: 330.0,
                    h: 440.0,
                    yaw_deg: 8.0,
                    pitch_deg: -1.5,
                    roll_deg: -3.0,
                    trans_x: -135.0,
                    trans_y: 10.0,
                    trans_z: 35.0, // Behind
                },
                CardConfig {
                    image_idx: 1,
                    w: 330.0,
                    h: 440.0,
                    yaw_deg: -5.0,
                    pitch_deg: -1.0,
                    roll_deg: 2.0,
                    trans_x: 135.0,
                    trans_y: -10.0,
                    trans_z: -20.0, // Front
                },
            ]
        }
        3 => {
            vec![
                CardConfig {
                    image_idx: 0,
                    w: 290.0,
                    h: 410.0,
                    yaw_deg: 30.0,
                    pitch_deg: -1.5,
                    roll_deg: 0.0,
                    trans_x: -280.0,
                    trans_y: 0.0,
                    trans_z: 75.0, // Left wing
                },
                CardConfig {
                    image_idx: 2,
                    w: 290.0,
                    h: 410.0,
                    yaw_deg: -30.0,
                    pitch_deg: -1.5,
                    roll_deg: 0.0,
                    trans_x: 280.0,
                    trans_y: 0.0,
                    trans_z: 75.0, // Right wing
                },
                CardConfig {
                    image_idx: 1,
                    w: 320.0,
                    h: 445.0,
                    yaw_deg: 0.0,
                    pitch_deg: 0.0,
                    roll_deg: 0.0,
                    trans_x: 0.0,
                    trans_y: 0.0,
                    trans_z: -25.0, // Center hero in front
                },
            ]
        }
        _ => {
            // 4 or 5+ cards: Full 5-card spatial arc
            let total_slots = count.min(5);
            let mut list = Vec::new();
            if total_slots >= 5 {
                list.push(CardConfig {
                    image_idx: 0,
                    w: 260.0,
                    h: 380.0,
                    yaw_deg: 44.0,
                    pitch_deg: -2.0,
                    roll_deg: 0.0,
                    trans_x: -410.0,
                    trans_y: 6.0,
                    trans_z: 130.0,
                });
                list.push(CardConfig {
                    image_idx: 4,
                    w: 260.0,
                    h: 380.0,
                    yaw_deg: -44.0,
                    pitch_deg: -2.0,
                    roll_deg: 0.0,
                    trans_x: 410.0,
                    trans_y: 6.0,
                    trans_z: 130.0,
                });
            }
            if total_slots >= 3 {
                list.push(CardConfig {
                    image_idx: 1,
                    w: 285.0,
                    h: 410.0,
                    yaw_deg: 24.0,
                    pitch_deg: -1.5,
                    roll_deg: 0.0,
                    trans_x: -210.0,
                    trans_y: 2.0,
                    trans_z: 50.0,
                });
                list.push(CardConfig {
                    image_idx: if total_slots >= 4 { 3 } else { 2 },
                    w: 285.0,
                    h: 410.0,
                    yaw_deg: -24.0,
                    pitch_deg: -1.5,
                    roll_deg: 0.0,
                    trans_x: 210.0,
                    trans_y: 2.0,
                    trans_z: 50.0,
                });
            }
            // Center hero
            list.push(CardConfig {
                image_idx: if total_slots >= 3 { 2 } else { 0 },
                w: 320.0,
                h: 445.0,
                yaw_deg: 0.0,
                pitch_deg: 0.0,
                roll_deg: 0.0,
                trans_x: 0.0,
                trans_y: 0.0,
                trans_z: -30.0,
            });
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
        });

        // Soft ambient drop shadow underneath card
        rasterize_drop_shadow(buffer, canvas_w, canvas_h, &screen_corners);

        // Rasterize card image with 3D homography & rounded corners
        let img = &images[cfg.image_idx];
        rasterize_perspective_card(buffer, canvas_w, canvas_h, &screen_corners, cfg.w, cfg.h, img, cfg.yaw_deg);
    }

    // Badge for extra photos if batch > 5
    if let LandingItem::Files { count: total, .. } = item {
        if *total > 5 && !projected_cards.is_empty() {
            render_more_photos_badge(buffer, canvas_w, canvas_h, &projected_cards[0].corners, *total - 5);
        }
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
    yaw_deg: f32,
) {
    let inv = match compute_homography_inv(corners[0], corners[1], corners[2], corners[3]) {
        Some(m) => m,
        None => return,
    };

    let min_x = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min).floor().max(0.0) as i32;
    let max_x = corners.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max).ceil().min((canvas_w - 1) as f32) as i32;
    let min_y = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min).floor().max(0.0) as i32;
    let max_y = corners.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max).ceil().min((canvas_h - 1) as f32) as i32;

    let r_u = (18.0 / card_w as f64).min(0.2);
    let r_v = (18.0 / card_h as f64).min(0.2);

    let border_u = 2.0 / card_w as f64;
    let border_v = 2.0 / card_h as f64;

    let src_w = img.w as usize;
    let src_h = img.h as usize;

    let base_light = 1.0 - (yaw_deg.abs() / 45.0) * 0.14;

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

            // Crisp 1.5px white polaroid edge
            let is_border = u < border_u
                || u > 1.0 - border_u
                || v < border_v
                || v > 1.0 - border_v
                || (is_corner && corner_dist_sq > 0.72);

            let (mut b, mut g, mut r) = if is_border {
                (252u8, 252u8, 252u8) // Crisp white border
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

                (
                    interp(p00.0, p10.0, p01.0, p11.0),
                    interp(p00.1, p10.1, p01.1, p11.1),
                    interp(p00.2, p10.2, p01.2, p11.2),
                )
            };

            // 3D Depth lighting
            let light = base_light;
            b = ((b as f32) * light).min(255.0) as u8;
            g = ((g as f32) * light).min(255.0) as u8;
            r = ((r as f32) * light).min(255.0) as u8;

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
) {
    let offset_y = 16.0f32;
    let margin = 18.0f32;

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
                let shadow_alpha = (falloff * falloff * 90.0) as u8; // max opacity ~35%

                let p_idx = row_offset + (x as usize) * 4;
                if buffer[p_idx + 3] < 255 {
                    // Only apply to background/transparent areas or blend with existing shadow
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
// Fallback Non-Image / Text Card
// ----------------------------------------------------------------------------

fn render_floating_doc_card(
    buffer: &mut [u8],
    canvas_w: i32,
    canvas_h: i32,
    item: &LandingItem,
) {
    let card_w = 460;
    let card_h = 240;
    let x0 = (canvas_w - card_w) / 2;
    let y0 = (canvas_h - card_h) / 2;
    let x1 = x0 + card_w;
    let y1 = y0 + card_h;

    // Soft drop shadow
    let shadow_margin = 20;
    for y in (y0 - 5)..(y1 + shadow_margin) {
        if y < 0 || y >= canvas_h { continue; }
        let row = (y as usize) * (canvas_w as usize) * 4;
        for x in (x0 - shadow_margin)..(x1 + shadow_margin) {
            if x < 0 || x >= canvas_w { continue; }
            let p_idx = row + (x as usize) * 4;
            buffer[p_idx + 0] = 0;
            buffer[p_idx + 1] = 0;
            buffer[p_idx + 2] = 0;
            buffer[p_idx + 3] = 70;
        }
    }

    // Pure pristine white floating card with rounded corners
    let r = 22;
    for y in y0..y1 {
        if y < 0 || y >= canvas_h { continue; }
        let row = (y as usize) * (canvas_w as usize) * 4;
        for x in x0..x1 {
            if x < 0 || x >= canvas_w { continue; }

            // Rounded corner check
            let mut corner_sq = 0.0f32;
            let mut is_c = false;
            if x < x0 + r && y < y0 + r {
                let dx = (x0 + r - x) as f32 / r as f32;
                let dy = (y0 + r - y) as f32 / r as f32;
                corner_sq = dx * dx + dy * dy;
                is_c = true;
            } else if x > x1 - r && y < y0 + r {
                let dx = (x - (x1 - r)) as f32 / r as f32;
                let dy = (y0 + r - y) as f32 / r as f32;
                corner_sq = dx * dx + dy * dy;
                is_c = true;
            } else if x > x1 - r && y > y1 - r {
                let dx = (x - (x1 - r)) as f32 / r as f32;
                let dy = (y - (y1 - r)) as f32 / r as f32;
                corner_sq = dx * dx + dy * dy;
                is_c = true;
            } else if x < x0 + r && y > y1 - r {
                let dx = (x0 + r - x) as f32 / r as f32;
                let dy = (y - (y1 - r)) as f32 / r as f32;
                corner_sq = dx * dx + dy * dy;
                is_c = true;
            }

            if is_c && corner_sq > 1.0 {
                continue;
            }

            let p_idx = row + (x as usize) * 4;
            buffer[p_idx + 0] = 255;
            buffer[p_idx + 1] = 255;
            buffer[p_idx + 2] = 255;
            buffer[p_idx + 3] = 255;
        }
    }
    let _ = item;
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
