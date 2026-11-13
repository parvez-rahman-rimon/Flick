"""
Flick Windows Receiver - Win32 Multi-Format Clipboard Injector
Simultaneously injects CF_HDROP, CF_UNICODETEXT, and CF_DIB into the Windows Clipboard.
Supports SMART_MULTI, FORCE_PATH, FORCE_IMAGE, and FORCE_FILE modes.
"""

import io
import os
import struct
import threading
import time
from typing import List

import win32clipboard
import win32con
from PIL import Image, ImageOps

# Thread lock to serialize Win32 clipboard access across HTTP server worker threads
_clipboard_lock = threading.Lock()

IMAGE_EXTENSIONS = {".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif"}
MAX_DIB_IMAGE_BYTES = 32 * 1024 * 1024  # 32 MB disk limit
MAX_PIXEL_COUNT = 36_000_000            # 36 Megapixels decompression bomb protection (e.g. 6000x6000)


def build_cf_hdrop_payload(file_paths: List[str]) -> bytes:
    """
    Constructs a valid Win32 DROPFILES structure in UTF-16LE (Unicode) mode:
    typedef struct _DROPFILES {
        DWORD pFiles; // Offset of file list from start of struct (20 bytes)
        POINT pt;     // Drop point (0, 0) -> 8 bytes
        BOOL  fNC;    // Non-client area flag (0) -> 4 bytes
        BOOL  fWide;  // TRUE (1) for UTF-16LE Unicode strings -> 4 bytes
    } DROPFILES;
    Total header = 20 bytes ("<IiiII").
    Followed by each path encoded in UTF-16LE + b"\\x00\\x00",
    terminated by a final b"\\x00\\x00".
    """
    header = struct.pack("<IiiII", 20, 0, 0, 0, 1)
    encoded_paths = b"".join(os.path.abspath(p).encode("utf-16le") + b"\x00\x00" for p in file_paths)
    return header + encoded_paths + b"\x00\x00"


def build_cf_dib_payload(image_path: str) -> bytes:
    """
    Decodes the image with Pillow, normalizes EXIF orientation, composites alpha over white,
    and returns the DIB byte stream (BMP format with 14-byte BITMAPFILEHEADER stripped).
    Guards against decompression bombs with MAX_PIXEL_COUNT limit.
    """
    with Image.open(image_path) as img:
        w, h = img.size
        if (w * h) > MAX_PIXEL_COUNT:
            raise ValueError(f"Image dimensions ({w}x{h}={w*h} pixels) exceed safety ceiling of {MAX_PIXEL_COUNT} pixels")
        img = ImageOps.exif_transpose(img)
        if img.mode != "RGB":
            # Composite alpha over white background for clean 24-bit bitmap rendering
            bg = Image.new("RGB", img.size, (255, 255, 255))
            if "A" in img.getbands():
                bg.paste(img, mask=img.split()[-1])
                img = bg
            else:
                img = img.convert("RGB")
        buf = io.BytesIO()
        img.save(buf, format="BMP")
        bmp_bytes = buf.getvalue()
        # DIB starts immediately at the BITMAPINFOHEADER (offset 14)
        return bmp_bytes[14:]


def _open_clipboard_with_retry(max_retries: int = 10, delay_sec: float = 0.025) -> None:
    """
    Windows clipboard can be locked temporarily by clipboard monitors or IDEs.
    Retries up to max_retries before propagating the error.
    """
    last_err = None
    for attempt in range(max_retries):
        try:
            win32clipboard.OpenClipboard(0)
            return
        except Exception as e:
            last_err = e
            time.sleep(delay_sec)
    raise RuntimeError(f"Could not open Windows clipboard after {max_retries} attempts: {last_err}")


# Windows 10/11 Privacy Formats: Opt out of Win+V history and Cloud Clipboard synchronization
CLIPBOARD_EXCLUDE_FORMATS = (
    "ExcludeClipboardContentFromMonitorProcessing",
    "CanIncludeInClipboardHistory",
    "CanUploadToCloudClipboard"
)


def _protect_clipboard_privacy() -> None:
    """Instructs Windows Shell to omit this clipboard payload from history (Win+V) and Cloud sync."""
    for fmt_name in CLIPBOARD_EXCLUDE_FORMATS:
        try:
            fmt_id = win32clipboard.RegisterClipboardFormat(fmt_name)
            win32clipboard.SetClipboardData(fmt_id, struct.pack("<I", 0))
        except Exception:
            pass


def inject_files_to_clipboard(file_paths: List[str], mode: str = "SMART_MULTI") -> List[str]:
    """
    Injects the given file paths into the Windows Clipboard according to the specified mode.
    Returns a list of injected format names (e.g. ['CF_HDROP', 'CF_UNICODETEXT', 'CF_DIB']).
    """
    if not file_paths:
        return []

    # Filter to existing files and normalize absolute paths
    abs_paths = [os.path.abspath(p) for p in file_paths if os.path.exists(p)]
    if not abs_paths:
        return []

    mode = mode.upper()
    injected_formats: List[str] = []

    # Determine if we should generate a bitmap (CF_DIB)
    should_include_dib = False
    dib_bytes = None
    if mode in ("SMART_MULTI", "FORCE_IMAGE") and len(abs_paths) == 1:
        first_path = abs_paths[0]
        ext = os.path.splitext(first_path)[1].lower()
        if ext in IMAGE_EXTENSIONS and os.path.getsize(first_path) <= MAX_DIB_IMAGE_BYTES:
            try:
                dib_bytes = build_cf_dib_payload(first_path)
                should_include_dib = True
            except Exception as e:
                print(f"[ClipboardEngine] Warning: Failed to generate DIB for {first_path}: {e}")
                should_include_dib = False

    with _clipboard_lock:
        _open_clipboard_with_retry()
        try:
            win32clipboard.EmptyClipboard()
            _protect_clipboard_privacy()

            # Format 1: CF_HDROP (File Drop)
            if mode in ("SMART_MULTI", "FORCE_FILE", "FORCE_IMAGE"):
                hdrop_bytes = build_cf_hdrop_payload(abs_paths)
                win32clipboard.SetClipboardData(win32con.CF_HDROP, hdrop_bytes)
                injected_formats.append("CF_HDROP")

            # Format 2: CF_UNICODETEXT (Text Path)
            if mode in ("SMART_MULTI", "FORCE_PATH"):
                path_text = "\r\n".join(abs_paths)
                win32clipboard.SetClipboardData(win32con.CF_UNICODETEXT, path_text)
                injected_formats.append("CF_UNICODETEXT")

            # Format 3: CF_DIB (Bitmap Pixels)
            if should_include_dib and dib_bytes:
                win32clipboard.SetClipboardData(win32con.CF_DIB, dib_bytes)
                injected_formats.append("CF_DIB")

        finally:
            try:
                win32clipboard.CloseClipboard()
            except Exception:
                pass

    return injected_formats


def inject_text_to_clipboard(text: str) -> List[str]:
    """
    Injects a plain text or URL string into the Windows Clipboard as CF_UNICODETEXT.
    """
    if text is None:
        text = ""

    with _clipboard_lock:
        _open_clipboard_with_retry()
        try:
            win32clipboard.EmptyClipboard()
            _protect_clipboard_privacy()
            win32clipboard.SetClipboardData(win32con.CF_UNICODETEXT, text)
        finally:
            try:
                win32clipboard.CloseClipboard()
            except Exception:
                pass

    return ["CF_UNICODETEXT"]
