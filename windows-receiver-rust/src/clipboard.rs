use std::ffi::OsStr;
use std::fs::File;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::copy_nonoverlapping;
use std::sync::{mpsc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::{GetLastError, GlobalFree, HANDLE, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
};
use windows_sys::Win32::System::Ole::CF_HDROP;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};

pub const CF_DIB: u32 = 8;
pub const CF_UNICODETEXT: u32 = 13;

static CLIPBOARD_MUTEX: Mutex<()> = Mutex::new(());

struct ClipboardGuard {
    is_open: bool,
}

impl ClipboardGuard {
    fn open() -> Result<Self, String> {
        for _ in 0..10 {
            unsafe {
                if OpenClipboard(0 as HWND) != 0 {
                    return Ok(ClipboardGuard { is_open: true });
                }
            }
            thread::sleep(Duration::from_millis(25));
        }
        Err(format!(
            "Failed to open clipboard after 10 retries: {}",
            unsafe { GetLastError() }
        ))
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        if self.is_open {
            unsafe {
                CloseClipboard();
            }
        }
    }
}

unsafe fn alloc_and_set_clipboard(format: u32, bytes: &[u8]) -> Result<(), String> {
    let hglobal = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len());
    if hglobal.is_null() {
        return Err(format!("GlobalAlloc failed: {}", GetLastError()));
    }
    let locked_ptr = GlobalLock(hglobal) as *mut u8;
    if locked_ptr.is_null() {
        GlobalFree(hglobal);
        return Err(format!("GlobalLock failed: {}", GetLastError()));
    }
    copy_nonoverlapping(bytes.as_ptr(), locked_ptr, bytes.len());
    GlobalUnlock(hglobal);

    let res = SetClipboardData(format, hglobal as HANDLE);
    if res.is_null() {
        GlobalFree(hglobal);
        return Err(format!(
            "SetClipboardData({}) failed: {}",
            format,
            GetLastError()
        ));
    }
    Ok(())
}

#[repr(C, packed)]
struct DropFilesHeader {
    p_files: u32, // 20
    pt_x: i32,    // 0
    pt_y: i32,    // 0
    f_nc: i32,    // 0
    f_wide: i32,  // 1 (TRUE -> UTF-16LE)
}

pub fn build_hdrop_bytes(paths: &[&Path]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let header = DropFilesHeader {
        p_files: std::mem::size_of::<DropFilesHeader>() as u32,
        pt_x: 0,
        pt_y: 0,
        f_nc: 0,
        f_wide: 1,
    };

    let header_slice = unsafe {
        std::slice::from_raw_parts(
            &header as *const _ as *const u8,
            std::mem::size_of::<DropFilesHeader>(),
        )
    };
    bytes.extend_from_slice(header_slice);

    for p in paths {
        let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let path_str = canonical.to_string_lossy();
        // Strip \\?\ prefix if present
        let clean_path = if path_str.starts_with(r"\\?\") {
            &path_str[4..]
        } else {
            &path_str
        };

        let wide: Vec<u16> = OsStr::new(clean_path).encode_wide().chain(Some(0)).collect();
        let wide_bytes = unsafe {
            std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2)
        };
        bytes.extend_from_slice(wide_bytes);
    }

    // Final null terminator
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

fn build_unicode_text_bytes(text: &str) -> Vec<u8> {
    let wide: Vec<u16> = OsStr::new(text).encode_wide().chain(Some(0)).collect();
    unsafe {
        std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2).to_vec()
    }
}

pub fn trim_working_set() {
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

pub fn inject_text(text: &str) -> Result<(), String> {
    let _mutex = CLIPBOARD_MUTEX.lock().unwrap();
    let _guard = ClipboardGuard::open()?;

    unsafe {
        EmptyClipboard();
    }

    let bytes = build_unicode_text_bytes(text);
    unsafe {
        alloc_and_set_clipboard(CF_UNICODETEXT, &bytes)?;
    }

    trim_working_set();
    Ok(())
}

const MAX_DIB_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DIB_PIXELS: u64 = 36_000_000;
const WINV_CAPTURE_DELAY_MS: u64 = 700;
const MAX_WINV_STACK_IMAGES: usize = 10;

pub fn is_supported_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif"
            )
        })
        .unwrap_or(false)
}

/// Returns whether individual `CF_DIB` snapshots should be pushed first so Windows
/// `Win + V` Clipboard History records clean image thumbnails before the final payload.
pub fn should_prepush_dib_to_winv(
    mode: &str,
    file_count: usize,
    image_count: usize,
    stack_winv: bool,
) -> bool {
    if !stack_winv || image_count == 0 {
        return false;
    }
    match normalize_clipboard_mode(mode) {
        // SMART_MULTI always includes CF_UNICODETEXT, which overrides CF_DIB in Win+V unless
        // a pure CF_DIB snapshot is pushed beforehand.
        "SMART_MULTI" => true,
        // FORCE_IMAGE omits CF_UNICODETEXT: for 1 image, inject_files already sets CF_DIB
        // directly; for >1 images, pre-push each image individually to Win+V.
        "FORCE_IMAGE" => file_count > 1,
        _ => false,
    }
}

pub fn planned_clipboard_formats(paths: &[PathBuf], mode: &str) -> Vec<&'static str> {
    let (include_text, include_hdrop, include_dib) =
        compute_clipboard_format_plan(mode, paths.len());
    let mut out = Vec::new();
    if include_text {
        out.push("CF_UNICODETEXT");
    }
    if include_hdrop {
        out.push("CF_HDROP");
    }
    if include_dib && paths.len() == 1 && is_supported_image_path(&paths[0]) {
        out.push("CF_DIB");
    }
    out
}

/// Injects only `CF_DIB` for a single image so Windows `Win + V` captures its thumbnail.
pub fn inject_single_dib(path: &Path) -> Result<(), String> {
    if !is_supported_image_path(path) {
        return Err("Unsupported image format for CF_DIB".to_string());
    }
    let dib_bytes = decode_image_to_dib(path)?;
    let _mutex = CLIPBOARD_MUTEX.lock().unwrap();
    let _guard = ClipboardGuard::open()?;
    unsafe {
        EmptyClipboard();
        alloc_and_set_clipboard(CF_DIB, &dib_bytes)?;
    }
    trim_working_set();
    Ok(())
}

enum ClipboardJob {
    Files {
        paths: Vec<PathBuf>,
        mode: String,
        stack_winv: bool,
    },
    Text(String),
}

static CLIPBOARD_TX: OnceLock<mpsc::Sender<ClipboardJob>> = OnceLock::new();

fn get_clipboard_worker() -> &'static mpsc::Sender<ClipboardJob> {
    CLIPBOARD_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<ClipboardJob>();
        let _ = thread::Builder::new()
            .name("flick-clipboard-worker".to_string())
            .stack_size(512 * 1024)
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    match job {
                        ClipboardJob::Text(text) => {
                            let _ = inject_text(&text);
                        }
                        ClipboardJob::Files {
                            paths,
                            mode,
                            stack_winv,
                        } => {
                            deliver_files_job(&paths, &mode, stack_winv);
                        }
                    }
                }
            });
        tx
    })
}

pub fn enqueue_files_delivery(paths: Vec<PathBuf>, mode: String, stack_winv: bool) {
    let _ = get_clipboard_worker().send(ClipboardJob::Files {
        paths,
        mode,
        stack_winv,
    });
}

pub fn enqueue_text_delivery(text: String) {
    let _ = get_clipboard_worker().send(ClipboardJob::Text(text));
}

fn deliver_files_job(paths: &[PathBuf], mode: &str, stack_winv: bool) {
    let image_paths: Vec<&Path> = paths
        .iter()
        .map(|p| p.as_path())
        .filter(|p| is_supported_image_path(p))
        .collect();

    let mut prepushed_any = false;
    if should_prepush_dib_to_winv(mode, paths.len(), image_paths.len(), stack_winv) {
        for img_path in image_paths.iter().take(MAX_WINV_STACK_IMAGES) {
            if inject_single_dib(img_path).is_ok() {
                prepushed_any = true;
                thread::sleep(Duration::from_millis(WINV_CAPTURE_DELAY_MS));
            }
        }
    }

    let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
    let _ = inject_files_with_options(&path_refs, mode, prepushed_any);
}

/// Normalizes a clipboard mode string, mapping empty, `"DEFAULT"`, or unrecognized
/// values to `"SMART_MULTI"`.
pub fn normalize_clipboard_mode(mode: &str) -> &'static str {
    match mode.trim().to_ascii_uppercase().as_str() {
        "FORCE_PATH" => "FORCE_PATH",
        "FORCE_IMAGE" => "FORCE_IMAGE",
        "FORCE_FILE" => "FORCE_FILE",
        _ => "SMART_MULTI",
    }
}

/// Computes `(include_text, include_hdrop, include_dib)` for the given normalized mode
/// and file count. Win32 `CF_DIB` only holds a single bitmap, so multi-image batches
/// rely on `CF_HDROP` to paste all images together.
pub fn compute_clipboard_format_plan(mode: &str, file_count: usize) -> (bool, bool, bool) {
    if file_count == 0 {
        return (false, false, false);
    }
    match normalize_clipboard_mode(mode) {
        "FORCE_PATH" => (true, false, false),
        "FORCE_FILE" => (false, true, false),
        "FORCE_IMAGE" => (false, true, file_count == 1),
        _ => (true, true, file_count == 1), // SMART_MULTI
    }
}

unsafe fn exclude_snapshot_from_winv_history() {
    for fmt_name in [
        "CanIncludeInClipboardHistory",
        "CanUploadToCloudClipboard",
    ] {
        let wide: Vec<u16> = OsStr::new(fmt_name).encode_wide().chain(Some(0)).collect();
        let fmt_id = RegisterClipboardFormatW(wide.as_ptr());
        if fmt_id != 0 {
            let _ = alloc_and_set_clipboard(fmt_id, &0u32.to_le_bytes());
        }
    }
}

#[allow(dead_code)]
pub fn inject_files(paths: &[&Path], mode: &str) -> Result<Vec<&'static str>, String> {
    inject_files_with_options(paths, mode, false)
}

pub fn inject_files_with_options(
    paths: &[&Path],
    mode: &str,
    exclude_from_history: bool,
) -> Result<Vec<&'static str>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let (include_text, include_hdrop, include_dib) =
        compute_clipboard_format_plan(mode, paths.len());

    let _mutex = CLIPBOARD_MUTEX.lock().unwrap();
    let _guard = ClipboardGuard::open()?;

    unsafe {
        EmptyClipboard();
        if exclude_from_history {
            exclude_snapshot_from_winv_history();
        }
    }

    let mut formats_applied = Vec::new();

    // 1. Text format (Path representation)
    if include_text {
        let text_lines: Vec<String> = paths
            .iter()
            .map(|p| {
                let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
                let s = canonical.to_string_lossy();
                if s.starts_with(r"\\?\") {
                    s[4..].to_string()
                } else {
                    s.to_string()
                }
            })
            .collect();
        let text = text_lines.join("\r\n");
        let wide_bytes = build_unicode_text_bytes(&text);
        unsafe {
            if alloc_and_set_clipboard(CF_UNICODETEXT, &wide_bytes).is_ok() {
                formats_applied.push("CF_UNICODETEXT");
            }
        }
    }

    // 2. CF_HDROP format (Single or multi-file drop list)
    if include_hdrop {
        let hdrop_bytes = build_hdrop_bytes(paths);
        unsafe {
            if alloc_and_set_clipboard(CF_HDROP as u32, &hdrop_bytes).is_ok() {
                formats_applied.push("CF_HDROP");
            }
        }
    }

    // 3. CF_DIB format (Bitmap) for single image payloads
    if include_dib && paths.len() == 1 {
        let file_path = paths[0];
        if let Some(ext) = file_path.extension().and_then(|s| s.to_str()) {
            let ext_lower = ext.to_lowercase();
            if matches!(
                ext_lower.as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif"
            ) {
                if let Ok(dib_bytes) = decode_image_to_dib(file_path) {
                    unsafe {
                        if alloc_and_set_clipboard(CF_DIB, &dib_bytes).is_ok() {
                            formats_applied.push("CF_DIB");
                        }
                    }
                }
            }
        }
    }

    trim_working_set();
    Ok(formats_applied)
}

fn decode_image_to_dib(path: &Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_DIB_FILE_BYTES {
        return Err("Image file exceeds 32 MB DIB ceiling".to_string());
    }

    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut exif_orientation = 1u32;

    // Check EXIF orientation
    let mut buf_reader = std::io::BufReader::new(file);
    if let Ok(reader) = exif::Reader::new().read_from_container(&mut buf_reader) {
        if let Some(field) = reader.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
            if let Some(val) = field.value.get_uint(0) {
                exif_orientation = val;
            }
        }
    }

    let mut img = image::open(path).map_err(|e| e.to_string())?;
    let pixels = (img.width() as u64).saturating_mul(img.height() as u64);
    if pixels > MAX_DIB_PIXELS {
        return Err("Image dimensions exceed 36 MP DIB ceiling".to_string());
    }

    // Apply EXIF rotation
    match exif_orientation {
        3 => img = img.rotate180(),
        6 => img = img.rotate90(),
        8 => img = img.rotate270(),
        _ => {}
    }

    // Encode to BMP in-memory
    let mut bmp_buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut bmp_buf, image::ImageFormat::Bmp)
        .map_err(|e| e.to_string())?;
    let bmp_bytes = bmp_buf.into_inner();

    if bmp_bytes.len() < 14 {
        return Err("BMP output too short".to_string());
    }

    // DIB is the BMP payload starting after 14-byte BITMAPFILEHEADER
    Ok(bmp_bytes[14..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_mode_maps_default_and_unknown_to_smart_multi() {
        assert_eq!(normalize_clipboard_mode("DEFAULT"), "SMART_MULTI");
        assert_eq!(normalize_clipboard_mode("default"), "SMART_MULTI");
        assert_eq!(normalize_clipboard_mode(""), "SMART_MULTI");
        assert_eq!(normalize_clipboard_mode("UNKNOWN"), "SMART_MULTI");
        assert_eq!(normalize_clipboard_mode("force_image"), "FORCE_IMAGE");
        assert_eq!(normalize_clipboard_mode("FORCE_PATH"), "FORCE_PATH");
        assert_eq!(normalize_clipboard_mode("FORCE_FILE"), "FORCE_FILE");
    }

    #[test]
    fn format_plan_supports_single_and_multi_file_batches() {
        assert_eq!(compute_clipboard_format_plan("SMART_MULTI", 1), (true, true, true));
        assert_eq!(compute_clipboard_format_plan("SMART_MULTI", 3), (true, true, false));
        assert_eq!(compute_clipboard_format_plan("FORCE_IMAGE", 1), (false, true, true));
        assert_eq!(compute_clipboard_format_plan("FORCE_IMAGE", 4), (false, true, false));
        assert_eq!(compute_clipboard_format_plan("FORCE_FILE", 1), (false, true, false));
        assert_eq!(compute_clipboard_format_plan("FORCE_FILE", 3), (false, true, false));
        assert_eq!(compute_clipboard_format_plan("FORCE_PATH", 3), (true, false, false));
        assert_eq!(compute_clipboard_format_plan("SMART_MULTI", 0), (false, false, false));
    }

    #[test]
    fn winv_prepush_rules_preserve_images_without_duplicate_in_force_image() {
        assert!(should_prepush_dib_to_winv("SMART_MULTI", 1, 1, true));
        assert!(should_prepush_dib_to_winv("SMART_MULTI", 3, 2, true));
        assert!(!should_prepush_dib_to_winv("FORCE_IMAGE", 1, 1, true));
        assert!(should_prepush_dib_to_winv("FORCE_IMAGE", 3, 3, true));
        assert!(!should_prepush_dib_to_winv("FORCE_PATH", 3, 3, true));
        assert!(!should_prepush_dib_to_winv("FORCE_FILE", 3, 3, true));
        assert!(!should_prepush_dib_to_winv("SMART_MULTI", 3, 3, false));
        assert!(!should_prepush_dib_to_winv("SMART_MULTI", 2, 0, true));
    }

    #[test]
    fn build_hdrop_encodes_multiple_utf16_paths_with_double_null_terminator() {
        let p1 = Path::new(r"C:\Temp\img1.png");
        let p2 = Path::new(r"C:\Temp\img2.jpg");
        let bytes = build_hdrop_bytes(&[p1, p2]);

        let header_size = std::mem::size_of::<DropFilesHeader>();
        assert_eq!(header_size, 20);
        let p_files = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let f_wide = i32::from_le_bytes(bytes[16..20].try_into().unwrap());
        assert_eq!(p_files, 20);
        assert_eq!(f_wide, 1);

        let payload_u16: Vec<u16> = bytes[20..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let decoded = String::from_utf16(&payload_u16).unwrap();
        assert!(decoded.ends_with("\0\0"));
        let parts: Vec<&str> = decoded.trim_end_matches('\0').split('\0').collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].ends_with("img1.png"));
        assert!(parts[1].ends_with("img2.jpg"));
    }
}
