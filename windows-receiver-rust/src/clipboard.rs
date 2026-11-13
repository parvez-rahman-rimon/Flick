use std::ffi::OsStr;
use std::fs::File;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::copy_nonoverlapping;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::{GetLastError, GlobalFree, HANDLE, HWND};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
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

fn build_hdrop_bytes(paths: &[&Path]) -> Vec<u8> {
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

pub fn inject_files(paths: &[&Path], mode: &str) -> Result<Vec<&'static str>, String> {
    let _mutex = CLIPBOARD_MUTEX.lock().unwrap();
    let _guard = ClipboardGuard::open()?;

    unsafe {
        EmptyClipboard();
    }

    let mut formats_applied = Vec::new();

    // 1. Text format (Path representation)
    if mode != "FORCE_IMAGE" {
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

    // 2. CF_HDROP format (File drop)
    if mode != "FORCE_PATH" && mode != "FORCE_IMAGE" {
        let hdrop_bytes = build_hdrop_bytes(paths);
        unsafe {
            if alloc_and_set_clipboard(CF_HDROP as u32, &hdrop_bytes).is_ok() {
                formats_applied.push("CF_HDROP");
            }
        }
    }

    // 3. CF_DIB format (Bitmap) if single image and supported mode
    if (mode == "SMART_MULTI" || mode == "FORCE_IMAGE") && paths.len() == 1 {
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
