# 🛡️ Flick (EkSend) — Full Security Audit & Codebase Bundle

> **Instructions for Claude Sonnet / Security Reviewer:**  
> This file contains the complete architecture and critical source code for **Flick**, an ultra-low-latency local LAN Android-to-Windows file and clipboard teleporter.  
> Please review this codebase from an adversarial penetration tester / application security engineer perspective and provide concrete vulnerabilities, attack scenarios, and architecture hardening recommendations.

---

## 📌 Architecture & Threat Boundaries
- **Core Functionality:** Android user taps 'Flick to PC' in the native Share Sheet -> file/text streams over local Wi-Fi to Windows PC -> automatically injected into the Windows clipboard (CF_HDROP, CF_UNICODETEXT, and CF_DIB) in < 150 ms with zero user interaction needed on PC -> user simply presses Ctrl + V anywhere.
- **Network Boundaries:**
  - UDP port 45454: Local subnet broadcast beacon (announces IP and port every 2.0s).
  - TCP port 45455: Threaded HTTP receiver (/ping, /pair, /upload, /clipboard).
- **Recent Hardening Applied:**
  - Interactive Win32 MessageBoxW prompt on Mode B pairing with cooldown per IP.
  - Comprehensive dangerous extensions quarantine (.blocked) + terminal metacharacters sanitization (preventing cmd/powershell command injection on Ctrl+V).
  - NTFS Mark-of-the-Web (:Zone.Identifier ZoneId=3) stamped on all saved files.
  - Win+V Clipboard History and Cloud Clipboard opt-out formats registered.
  - Safe unguessable tempfile writes preventing race conditions.
  - Android UDP listener enforces actual UDP packet source IP (defeats beacon IP spoofing).
  - Android Share Activity enforces content:// URIs only and rejects internal package authority.

---

# 💻 Source Code Index


## 📄 Windows Receiver: HTTP Streaming Server & Auth Handshake
File Path: windows-receiver/http_receiver.py

`python
"""
Flick Windows Receiver - Threaded HTTP Streaming Receiver
Serves endpoints on TCP port 45455:
  GET  /ping      - Health check, latency ping, auth verification
  POST /pair      - QR-based or LAN-prompt pairing handshake
  POST /upload    - High-throughput binary file streaming & clipboard injection
  POST /clipboard - Instant plain text / URL teleport to Windows clipboard
"""

import ctypes
import hmac
import json
import os
import re
import shutil
import tempfile
import threading
import time
import unicodedata
import urllib.parse
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Dict, List, Optional, Tuple

def safe_print(*args, **kwargs):
    if sys.stdout is not None:
        try:
            print(*args, **kwargs)
        except Exception:
            pass

import io
import math
import struct
import wave

try:
    import winsound
except ImportError:
    winsound = None

import clipboard_engine
from config_manager import add_paired_device, get_config, get_lan_ip

SERVER_PORT = 45455
CHUNK_SIZE = 65536  # 64 KB read chunks for optimal stream efficiency

# Security Thresholds & Hardening
MAX_ALLOWED_FILE_BYTES = 4 * 1024 * 1024 * 1024  # 4 GB max per file
SAFETY_DISK_BUFFER = 500 * 1024 * 1024           # 500 MB minimum remaining free space
MAX_PAIR_BODY_BYTES = 64 * 1024                  # 64 KB max for /pair
MAX_CLIPBOARD_BODY_BYTES = 256 * 1024            # 256 KB max for /clipboard

# Terminal Command Injection & Win32 Quirks Sanitization
_BAD_FILENAME_CHARS = re.compile(
    r'[\x00-\x1f<>:"/\\|?*&;^%$!`\'(),=+@#\[\]{}~\u200b-\u200f\u202a-\u202e\u2066-\u2069]'
)
_RESERVED_NAMES = {"CON", "PRN", "AUX", "NUL", *(f"COM{i}" for i in range(1, 10)), *(f"LPT{i}" for i in range(1, 10))}

# Comprehensive Dangerous / Executable Extensions Denylist
DANGEROUS_EXTENSIONS = {
    ".exe", ".bat", ".cmd", ".vbs", ".ps1", ".scr", ".hta", ".lnk",
    ".msi", ".dll", ".reg", ".com", ".cpl", ".jar", ".pif", ".wsf",
    ".js", ".jse", ".vbe", ".wsh", ".msc", ".xll", ".rdp", ".inf",
    ".appx", ".msix", ".appinstaller", ".application", ".iso", ".vhd",
    ".vhdx", ".chm", ".docm", ".xlsm", ".pptm", ".url", ".scf",
    ".library-ms", ".searchconnector-ms"
}


def sanitize_filename(raw: str, limit: int = 100) -> Tuple[str, bool]:
    """
    Sanitizes filename against Win32 quirks, path traversal, terminal command injection,
    and dangerous executable extensions. Win32 strips trailing dots and spaces on file creation,
    so we strip them BEFORE splitext to prevent bypasses like 'evil.exe.' or 'evil.exe '.
    Returns (safe_filename, is_quarantined).
    """
    raw_name = urllib.parse.unquote_plus(raw) if raw else ""
    n = unicodedata.normalize("NFC", os.path.basename(raw_name.replace("\\", "/")))
    n = _BAD_FILENAME_CHARS.sub("_", n).strip(" .")
    if not n:
        n = f"flick_{int(time.time() * 1000)}"

    stem, ext = os.path.splitext(n)
    ext_lower = ext.lower().strip(" .")
    ext_normalized = f".{ext_lower}" if ext_lower else ""

    if not stem or stem.upper() in _RESERVED_NAMES:
        stem = f"file_{stem}"

    stem = stem[:limit]
    final_ext = ext_normalized[:16]

    is_dangerous = final_ext in DANGEROUS_EXTENSIONS
    if is_dangerous:
        return f"{stem}{final_ext}.blocked", True
    return f"{stem}{final_ext}", False


def apply_mark_of_the_web(file_path: str) -> None:
    """Stamps ZoneId=3 (Internet / Untrusted zone) to invoke Windows SmartScreen on execution."""
    try:
        with open(file_path + ":Zone.Identifier", "w", encoding="utf-8") as z:
            z.write("[ZoneTransfer]\r\nZoneId=3\r\n")
    except Exception:
        pass


# Concurrency & flood control for pairing prompt
_pairing_prompt_active = False
_pairing_lock = threading.Lock()
_pairing_cooldowns: Dict[str, float] = {}

# In-memory batch accumulator for multi-file transfers (ACTION_SEND_MULTIPLE)
_batch_lock = threading.Lock()
_active_batches: Dict[str, Dict[str, Any]] = {}

_notification_callback = None


def set_notification_callback(callback) -> None:
    """Registers a global callback (title, message) -> None to display tray notifications."""
    global _notification_callback
    _notification_callback = callback


def trigger_notification(title: str, message: str) -> None:
    """Triggers an OS/tray balloon notification if callback is registered."""
    if _notification_callback:
        try:
            _notification_callback(title, message)
        except Exception as e:
            safe_print(f"[HTTP] Notification trigger error: {e}")


def _create_soft_arrival_chime() -> bytes:
    """
    Synthesizes a warm, gentle, slow harmonic chime in memory (PCM 16-bit 44.1kHz).
    Uses low volume amplitude (~12%) and smooth exponential decay to be soothing and pleasant.
    """
    sample_rate = 44100
    duration = 0.45  # 450ms slow, soothing chime
    num_samples = int(sample_rate * duration)

    # Warm harmonic frequencies (E5 = 659.25 Hz fundamental + G#5 = 830.61 Hz major third)
    f1 = 659.25
    f2 = 830.61

    buf = io.BytesIO()
    with wave.open(buf, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(sample_rate)

        frames = bytearray()
        for i in range(num_samples):
            t = i / sample_rate
            # 25ms soft attack (zero clicking, gentle swell)
            attack = min(1.0, t / 0.025)
            # Slow, soft exponential tail
            decay = math.exp(-t * 6.5)

            # Pure gentle sine waves, scaled to ~12% volume (-18 dB)
            sample = (0.75 * math.sin(2 * math.pi * f1 * t) + 0.25 * math.sin(2 * math.pi * f2 * t))
            val = int(sample * attack * decay * 4000)
            val = max(-32768, min(32767, val))
            frames.extend(struct.pack("<h", val))

        wav.writeframes(frames)
    return buf.getvalue()


_CACHED_CHIME_BYTES: bytes = _create_soft_arrival_chime()


def play_chime():
    """Plays a soft, gentle arrival chime asynchronously on PC without freezing."""
    if winsound and _CACHED_CHIME_BYTES:
        try:
            winsound.PlaySound(_CACHED_CHIME_BYTES, winsound.SND_MEMORY | winsound.SND_ASYNC)
        except Exception:
            try:
                winsound.MessageBeep(winsound.MB_OK)
            except Exception:
                pass


class FlickRequestHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    timeout = 15.0  # Prevent Slowloris socket hanging

    def log_message(self, format: str, *args: Any) -> None:
        safe_print(f"[HTTP] {self.address_string()} - {format % args}")

    def _send_json_response(self, status_code: int, data: Dict[str, Any]) -> None:
        body = json.dumps(data).encode("utf-8")
        self.send_response(status_code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def _verify_auth(self) -> bool:
        """Verifies the X-Flick-Token header against config in constant time."""
        provided = self.headers.get("X-Flick-Token", "")
        cfg = get_config()
        expected = cfg.get("auth_token", "")
        if not provided or not expected:
            return False
        return hmac.compare_digest(provided, expected)

    def do_GET(self) -> None:
        parsed_url = urllib.parse.urlparse(self.path)

        if parsed_url.path == "/ping":
            cfg = get_config()
            is_auth = self._verify_auth()
            response_data = {
                "status": "ok",
                "pc_id": cfg.get("pc_id", ""),
                "pc_name": cfg.get("pc_name", ""),
                "authenticated": is_auth,
                "clipboard_mode": cfg.get("clipboard_mode", "SMART_MULTI"),
                "version": "1.0.0"
            }
            self._send_json_response(200, response_data)
            return

        self._send_json_response(404, {"status": "not_found", "message": "Unknown endpoint"})

    def do_POST(self) -> None:
        start_time = time.time()
        parsed_url = urllib.parse.urlparse(self.path)

        # CSRF Protection: Prohibit cross-origin web requests from browsing contexts
        if self.headers.get("Origin"):
            self._send_json_response(403, {"status": "error", "message": "Cross-origin requests prohibited"})
            return

        if parsed_url.path == "/pair":
            self._handle_pair()
            return

        if parsed_url.path == "/upload":
            self._handle_upload(start_time)
            return

        if parsed_url.path == "/clipboard":
            self._handle_clipboard()
            return

        self._send_json_response(404, {"status": "not_found", "message": "Unknown endpoint"})

    def _handle_pair(self) -> None:
        try:
            content_length = int(self.headers.get("Content-Length", 0))
        except ValueError:
            self._send_json_response(400, {"status": "error", "message": "Invalid Content-Length"})
            return

        if content_length <= 0 or content_length > MAX_PAIR_BODY_BYTES:
            self._send_json_response(400, {"status": "error", "message": f"Invalid body size (max {MAX_PAIR_BODY_BYTES} bytes)"})
            return

        try:
            body_bytes = self.rfile.read(content_length)
            req = json.loads(body_bytes.decode("utf-8"))
            if not isinstance(req, dict):
                raise ValueError("JSON body must be an object")
        except Exception as e:
            self._send_json_response(400, {"status": "error", "message": f"Malformed JSON: {e}"})
            return

        client_id = str(req.get("client_id", "unknown-client"))[:64]
        raw_client_name = str(req.get("client_name", "Android Device"))
        # Strip control characters, bidi overrides, and limit length to 32 chars
        client_name = _BAD_FILENAME_CHARS.sub("", raw_client_name).strip()[:32] or "Android Device"
        token = str(req.get("token", ""))

        cfg = get_config()
        expected_token = cfg.get("auth_token", "")

        # Mode A: QR Code scan with pre-shared auth_token (Out-of-band physical proof)
        if token and hmac.compare_digest(token, expected_token):
            add_paired_device(client_id, client_name)
            play_chime()
            trigger_notification("⚡ Flick Paired", f"Successfully paired with {client_name}")
            self._send_json_response(200, {
                "status": "paired",
                "pc_id": cfg.get("pc_id", ""),
                "pc_name": cfg.get("pc_name", ""),
                "token": expected_token
            })
            return

        # Mode B: Inbound LAN 1-Tap Pair Request
        # Flood and prompt-fatigue control
        client_ip = self.client_address[0]
        now = time.time()
        if client_ip in _pairing_cooldowns and now < _pairing_cooldowns[client_ip]:
            self._send_json_response(429, {"status": "rejected", "message": "Too many requests. Please wait before retrying."})
            return

        global _pairing_prompt_active
        with _pairing_lock:
            if _pairing_prompt_active:
                self._send_json_response(409, {"status": "busy", "message": "Another pairing prompt is active on PC."})
                return
            _pairing_prompt_active = True

        safe_print(f"[Pairing] Inbound LAN pairing request from {client_name} ({client_ip}). Prompting user...")

        prompt_text = (
            f"Flick Pairing Request:\n\n"
            f"Device: {client_name}\n"
            f"IP Address: {client_ip}\n"
            f"Client ID: {client_id[:8]}...\n\n"
            f"Do you want to allow this device to connect to your PC?"
        )
        try:
            # 0x00000004 = MB_YESNO, 0x00000020 = MB_ICONQUESTION, 0x00040000 = MB_TOPMOST, 0x00010000 = MB_SETFOREGROUND
            user_choice = ctypes.windll.user32.MessageBoxW(
                0,
                prompt_text,
                "⚡ Flick — Pair Request",
                0x00000004 | 0x00000020 | 0x00040000 | 0x00010000
            )
            IDYES = 6
            is_approved = (user_choice == IDYES)
        except Exception as e:
            safe_print(f"[Pairing] Prompt error: {e}")
            is_approved = False
        finally:
            with _pairing_lock:
                _pairing_prompt_active = False

        if is_approved:
            safe_print(f"[Pairing] User APPROVED pairing request from {client_name} ({client_ip})")
            add_paired_device(client_id, client_name)
            play_chime()
            trigger_notification("⚡ Flick Paired", f"Approved and connected with {client_name}")
            self._send_json_response(200, {
                "status": "paired",
                "pc_id": cfg.get("pc_id", ""),
                "pc_name": cfg.get("pc_name", ""),
                "token": expected_token
            })
        else:
            safe_print(f"[Pairing] User REJECTED pairing request from {client_name} ({client_ip})")
            _pairing_cooldowns[client_ip] = now + 15.0  # 15s cooldown per IP
            self._send_json_response(403, {
                "status": "rejected",
                "message": "Pairing request was rejected by PC user."
            })

    def _handle_upload(self, start_time: float) -> None:
        if not self._verify_auth():
            self._send_json_response(401, {"status": "unauthorized", "message": "Invalid or missing X-Flick-Token"})
            return

        cfg = get_config()
        save_dir = cfg.get("save_directory", os.path.expanduser("~/Downloads/Flick"))
        os.makedirs(save_dir, exist_ok=True)

        # 1. Validate Content-Length and Disk Space
        try:
            content_length = int(self.headers.get("Content-Length", 0))
        except ValueError:
            self._send_json_response(400, {"status": "error", "message": "Invalid Content-Length"})
            return

        if content_length <= 0:
            self._send_json_response(400, {"status": "error", "message": "Content-Length must be greater than 0"})
            return

        if content_length > MAX_ALLOWED_FILE_BYTES:
            self._send_json_response(413, {"status": "error", "message": f"File size ({content_length} bytes) exceeds 4 GB limit"})
            return

        try:
            free_space = shutil.disk_usage(save_dir).free
            if free_space < (content_length + SAFETY_DISK_BUFFER):
                self._send_json_response(507, {"status": "error", "message": "Insufficient disk space on PC"})
                return
        except Exception as e:
            safe_print(f"[HTTP] Disk space check warning: {e}")

        # 2. Sanitize Filename (strip trailing dots/spaces, handle reserved names, terminal chars)
        raw_filename = self.headers.get("X-File-Name", "")
        filename, is_quarantined = sanitize_filename(raw_filename)
        if is_quarantined:
            safe_print(f"[HTTP] Quarantined dangerous file: {filename}")

        # Resolve filename collision safely
        dest_path = os.path.join(save_dir, filename)
        if os.path.exists(dest_path):
            n, e = os.path.splitext(filename)
            dest_path = os.path.join(save_dir, f"{n}_{int(time.time()*1000)}{e}")

        # Unique, unguessable temporary file in save_dir (defeats collision and race conditions)
        temp_file = tempfile.NamedTemporaryFile(dir=save_dir, prefix="flick_tmp_", suffix=".part", delete=False)
        temp_path = temp_file.name
        bytes_left = content_length
        bytes_written = 0

        try:
            with temp_file as f:
                while bytes_left > 0:
                    read_len = min(CHUNK_SIZE, bytes_left)
                    chunk = self.rfile.read(read_len)
                    if not chunk:
                        raise ConnectionResetError("Connection closed while streaming upload payload")
                    f.write(chunk)
                    bytes_written += len(chunk)
                    bytes_left -= len(chunk)

            # Atomic replace to final destination
            os.replace(temp_path, dest_path)
            apply_mark_of_the_web(dest_path)

        except Exception as e:
            if os.path.exists(temp_path):
                try:
                    os.remove(temp_path)
                except Exception:
                    pass
            self._send_json_response(500, {"status": "error", "message": f"File write failure: {e}"})
            return

        # Multi-file batch handling
        try:
            batch_index = int(self.headers.get("X-Batch-Index", "0"))
            batch_total = int(self.headers.get("X-Batch-Total", "1"))
        except ValueError:
            batch_index = 0
            batch_total = 1

        client_key = self.client_address[0]
        files_to_inject: List[str] = []

        with _batch_lock:
            if client_key not in _active_batches:
                _active_batches[client_key] = {"paths": [], "timestamp": time.time()}

            _active_batches[client_key]["paths"].append(dest_path)

            # If this is the last file in the batch (or single file)
            if batch_index >= batch_total - 1:
                files_to_inject = list(_active_batches[client_key]["paths"])
                del _active_batches[client_key]

        # Determine effective clipboard mode (quarantined files forced to text path only)
        header_mode = self.headers.get("X-Clipboard-Mode", "DEFAULT").upper()
        effective_mode = cfg.get("clipboard_mode", "SMART_MULTI") if header_mode == "DEFAULT" else header_mode
        has_quarantined = any(p.endswith(".blocked") for p in files_to_inject)
        if has_quarantined:
            effective_mode = "FORCE_PATH"

        injected_formats: List[str] = []
        if files_to_inject:
            try:
                injected_formats = clipboard_engine.inject_files_to_clipboard(files_to_inject, mode=effective_mode)
            except Exception as e:
                safe_print(f"[HTTP] Clipboard injection error: {e}")

        if cfg.get("play_sound", True):
            play_chime()

        if files_to_inject:
            if has_quarantined:
                trigger_notification("⚠️ File Quarantined", "Saved with .blocked extension for security")
            elif len(files_to_inject) == 1:
                base = os.path.basename(files_to_inject[0])
                trigger_notification("⚡ Flick Received", f"'{base}' ready to paste (Ctrl+V)")
            else:
                trigger_notification("⚡ Flick Received", f"{len(files_to_inject)} files copied to clipboard (Ctrl+V)")

        elapsed_ms = int((time.time() - start_time) * 1000)
        self._send_json_response(200, {
            "status": "ok",
            "saved_path": dest_path,
            "bytes_written": bytes_written,
            "clipboard_formats": injected_formats,
            "elapsed_ms": elapsed_ms
        })

    def _handle_clipboard(self) -> None:
        if not self._verify_auth():
            self._send_json_response(401, {"status": "unauthorized", "message": "Invalid or missing X-Flick-Token"})
            return

        try:
            content_length = int(self.headers.get("Content-Length", 0))
        except ValueError:
            self._send_json_response(400, {"status": "error", "message": "Invalid Content-Length"})
            return

        if content_length <= 0 or content_length > MAX_CLIPBOARD_BODY_BYTES:
            self._send_json_response(400, {"status": "error", "message": f"Invalid body size (max {MAX_CLIPBOARD_BODY_BYTES} bytes)"})
            return

        try:
            body_bytes = self.rfile.read(content_length)
            text = body_bytes.decode("utf-8")
        except Exception as e:
            self._send_json_response(400, {"status": "error", "message": f"Malformed text: {e}"})
            return

        formats = clipboard_engine.inject_text_to_clipboard(text)
        cfg = get_config()
        if cfg.get("play_sound", True):
            play_chime()

        # REDACTED: Omit raw text preview from Windows notification toast (prevents OTP/password leak on lock screen)
        trigger_notification("⚡ Flick Clipboard", f"Text copied to clipboard ({len(text)} chars)")

        self._send_json_response(200, {
            "status": "ok",
            "characters_copied": len(text),
            "clipboard_formats": formats
        })


class ThreadedHttpReceiver:
    def __init__(self, port: int = SERVER_PORT):
        self.port = port
        self.server: Optional[ThreadingHTTPServer] = None
        self._thread: Optional[threading.Thread] = None

    def start(self) -> None:
        try:
            self.server = ThreadingHTTPServer(("0.0.0.0", self.port), FlickRequestHandler)
            self._thread = threading.Thread(target=self.server.serve_forever, daemon=True, name="FlickHttpServer")
            self._thread.start()
            safe_print(f"[HttpReceiver] HTTP Receiver server listening on 0.0.0.0:{self.port}")
        except OSError as e:
            safe_print(f"[HttpReceiver] Port {self.port} bind error: {e}")
            raise

    def stop(self) -> None:
        if self.server:
            self.server.shutdown()
            self.server.server_close()
            self.server = None
        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=1.0)
            self._thread = None
        safe_print("[HttpReceiver] HTTP Receiver server stopped.")


_global_receiver: Optional[ThreadedHttpReceiver] = None


def start_http_receiver(port: int = SERVER_PORT) -> ThreadedHttpReceiver:
    global _global_receiver
    if _global_receiver is None:
        _global_receiver = ThreadedHttpReceiver(port=port)
    _global_receiver.start()
    return _global_receiver


def stop_http_receiver() -> None:
    global _global_receiver
    if _global_receiver:
        _global_receiver.stop()
        _global_receiver = None

`

---

## 📄 Windows Receiver: Win32 Multi-Format Clipboard Engine
File Path: windows-receiver/clipboard_engine.py

`python
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

`

---

## 📄 Windows Receiver: Configuration & Token Manager
File Path: windows-receiver/config_manager.py

`python
"""
Flick Windows Receiver - Configuration Manager
Manages ~/.flick/config.json, token generation, paired devices, and network discovery IP resolution.
"""

import json
import os
import secrets
import socket
import sys
import threading
import uuid
from pathlib import Path
from typing import Any, Dict, List, Optional

CONFIG_DIR = Path.home() / ".flick"
CONFIG_FILE = CONFIG_DIR / "config.json"
DEFAULT_DOWNLOADS = Path.home() / "Downloads" / "Flick"

_config_lock = threading.Lock()
_cached_config: Optional[Dict[str, Any]] = None


def get_resource_path(relative_path: str) -> str:
    """Get absolute path to bundled resource (works for dev and PyInstaller onefile)."""
    base_path = getattr(sys, "_MEIPASS", os.path.dirname(os.path.abspath(__file__)))
    return os.path.join(base_path, relative_path)



def get_lan_ip() -> str:
    """
    Determines the active LAN IPv4 address using a dummy UDP connection.
    Avoids virtual adapters (WSL, Hyper-V, Docker) by querying the OS routing table.
    """
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
            s.settimeout(0.5)
            s.connect(("8.8.8.8", 80))
            return s.getsockname()[0]
    except Exception:
        try:
            return socket.gethostbyname(socket.gethostname())
        except Exception:
            return "127.0.0.1"


def get_subnet_broadcast(ip: Optional[str] = None) -> str:
    """Calculates a directed subnet broadcast address based on the current IPv4."""
    if not ip:
        ip = get_lan_ip()
    try:
        parts = ip.split(".")
        if len(parts) == 4:
            return f"{parts[0]}.{parts[1]}.{parts[2]}.255"
    except Exception:
        pass
    return "255.255.255.255"


def _init_default_config() -> Dict[str, Any]:
    """Generates a fresh configuration dictionary for first-time setup."""
    return {
        "pc_id": str(uuid.uuid4()),
        "pc_name": socket.gethostname(),
        "http_port": 45455,
        "udp_port": 45454,
        "auth_token": secrets.token_hex(32),
        "save_directory": str(DEFAULT_DOWNLOADS),
        "clipboard_mode": "SMART_MULTI",
        "play_sound": True,
        "paired_devices": []
    }


def load_config() -> Dict[str, Any]:
    """Loads configuration from ~/.flick/config.json or creates it if missing."""
    global _cached_config
    with _config_lock:
        if not CONFIG_DIR.exists():
            CONFIG_DIR.mkdir(parents=True, exist_ok=True)

        if not DEFAULT_DOWNLOADS.exists():
            DEFAULT_DOWNLOADS.mkdir(parents=True, exist_ok=True)

        if not CONFIG_FILE.exists():
            config = _init_default_config()
            with open(CONFIG_FILE, "w", encoding="utf-8") as f:
                json.dump(config, f, indent=2)
            _cached_config = config
            return dict(_cached_config)

        try:
            with open(CONFIG_FILE, "r", encoding="utf-8") as f:
                config = json.load(f)
            # Ensure required keys exist
            defaults = _init_default_config()
            dirty = False
            for k, v in defaults.items():
                if k not in config:
                    config[k] = v
                    dirty = True
            if dirty:
                with open(CONFIG_FILE, "w", encoding="utf-8") as f:
                    json.dump(config, f, indent=2)
            _cached_config = config
            return dict(_cached_config)
        except Exception:
            config = _init_default_config()
            _cached_config = config
            return dict(_cached_config)


def get_config() -> Dict[str, Any]:
    """Returns a snapshot of the current configuration."""
    global _cached_config
    if _cached_config is None:
        return load_config()
    with _config_lock:
        return dict(_cached_config)


def save_config(new_config: Dict[str, Any]) -> None:
    """Persists updated configuration to disk."""
    global _cached_config
    with _config_lock:
        CONFIG_DIR.mkdir(parents=True, exist_ok=True)
        with open(CONFIG_FILE, "w", encoding="utf-8") as f:
            json.dump(new_config, f, indent=2)
        _cached_config = dict(new_config)


def set_clipboard_mode(mode: str) -> None:
    """Updates the default clipboard mode (SMART_MULTI, FORCE_PATH, FORCE_IMAGE, FORCE_FILE)."""
    valid_modes = {"SMART_MULTI", "FORCE_PATH", "FORCE_IMAGE", "FORCE_FILE"}
    if mode not in valid_modes:
        raise ValueError(f"Invalid mode: {mode}. Must be one of {valid_modes}")
    cfg = get_config()
    cfg["clipboard_mode"] = mode
    save_config(cfg)


def add_paired_device(client_id: str, client_name: str) -> None:
    """Registers a paired Android device client."""
    import time
    cfg = get_config()
    devices: List[Dict[str, Any]] = cfg.get("paired_devices", [])
    # Update if already exists, else append
    for dev in devices:
        if dev.get("client_id") == client_id:
            dev["client_name"] = client_name
            dev["paired_at"] = int(time.time())
            save_config(cfg)
            return
    devices.append({
        "client_id": client_id,
        "client_name": client_name,
        "paired_at": int(time.time())
    })
    cfg["paired_devices"] = devices
    save_config(cfg)


def is_device_paired(client_id: str) -> bool:
    """Checks whether a client_id has been paired previously."""
    cfg = get_config()
    for dev in cfg.get("paired_devices", []):
        if dev.get("client_id") == client_id:
            return True
    return False

`

---

## 📄 Windows Receiver: UDP Discovery Beacon
File Path: windows-receiver/discovery_beacon.py

`python
"""
Flick Windows Receiver - UDP Discovery Beacon
Broadcasts FLICK_DISCOVERY_V1 JSON announcement every 2.0s over UDP port 45454.
Allows Android devices on the LAN to discover the PC without cloud relays or static IPs.
"""

import json
import socket
import sys
import threading
import time
from typing import Optional

def safe_print(*args, **kwargs):
    if sys.stdout is not None:
        try:
            print(*args, **kwargs)
        except Exception:
            pass

from config_manager import get_config, get_lan_ip, get_subnet_broadcast

DISCOVERY_PORT = 45454
BEACON_INTERVAL_SEC = 2.0
PROTOCOL_VERSION = "FLICK_DISCOVERY_V1"
RECEIVER_VERSION = "1.0.0"


class DiscoveryBeacon:
    def __init__(self, port: int = DISCOVERY_PORT, interval: float = BEACON_INTERVAL_SEC):
        self.port = port
        self.interval = interval
        self._running = False
        self._thread: Optional[threading.Thread] = None

    def _broadcast_loop(self) -> None:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
        sock.settimeout(1.0)

        safe_print(f"[DiscoveryBeacon] Started UDP broadcast beacon on port {self.port} (every {self.interval}s)")

        while self._running:
            try:
                cfg = get_config()
                lan_ip = get_lan_ip()
                subnet_broadcast = get_subnet_broadcast(lan_ip)

                payload = {
                    "protocol": PROTOCOL_VERSION,
                    "pc_id": cfg.get("pc_id", ""),
                    "pc_name": cfg.get("pc_name", socket.gethostname()),
                    "ip": lan_ip,
                    "http_port": cfg.get("http_port", 45455),
                    "version": RECEIVER_VERSION,
                    "timestamp": int(time.time())
                }

                data = json.dumps(payload).encode("utf-8")

                # Send to both universal broadcast and subnet broadcast for maximum Wi-Fi compatibility
                targets = {"255.255.255.255", subnet_broadcast}
                for target_ip in targets:
                    try:
                        sock.sendto(data, (target_ip, self.port))
                    except Exception:
                        pass

            except Exception as e:
                safe_print(f"[DiscoveryBeacon] Error sending beacon: {e}")

            # Sleep in small increments for responsive shutdown
            slept = 0.0
            while self._running and slept < self.interval:
                time.sleep(0.2)
                slept += 0.2

        try:
            sock.close()
        except Exception:
            pass
        safe_print("[DiscoveryBeacon] Stopped UDP broadcast beacon.")

    def start(self) -> None:
        if self._running:
            return
        self._running = True
        self._thread = threading.Thread(target=self._broadcast_loop, daemon=True, name="FlickDiscoveryBeacon")
        self._thread.start()

    def stop(self) -> None:
        self._running = False
        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=1.0)


_global_beacon: Optional[DiscoveryBeacon] = None


def start_discovery_beacon(port: int = DISCOVERY_PORT) -> DiscoveryBeacon:
    global _global_beacon
    if _global_beacon is None:
        _global_beacon = DiscoveryBeacon(port=port)
    _global_beacon.start()
    return _global_beacon


def stop_discovery_beacon() -> None:
    global _global_beacon
    if _global_beacon:
        _global_beacon.stop()
        _global_beacon = None

`

---

## 📄 Windows Receiver: Main System Tray & Lifecycle
File Path: windows-receiver/flick_receiver.py

`python
"""
Flick Windows Receiver - Main Entrypoint & System Tray Application
Orchestrates the UDP discovery beacon, threaded HTTP receiver, pystray tray menu, and global hotkeys.
"""

import ctypes
from ctypes import wintypes
import os
import sys
import threading
import time
from typing import Any, Optional
from PIL import Image, ImageDraw
import pystray
import keyboard

if sys.stdout and hasattr(sys.stdout, "reconfigure"):
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:
        pass


def safe_print(*args, **kwargs):
    if sys.stdout is not None:
        try:
            print(*args, **kwargs)
        except Exception:
            pass


from config_manager import (
    get_config,
    get_lan_ip,
    get_resource_path,
    load_config,
    set_clipboard_mode,
)
from discovery_beacon import start_discovery_beacon, stop_discovery_beacon
from http_receiver import set_notification_callback, start_http_receiver, stop_http_receiver
from qr_modal import show_qr_pairing_modal

# Global Win32 Single-Instance Mutex
MUTEX_NAME = "Local\\Flick_Windows_Receiver_SingleInstance_Mutex"
_mutex_handle = None

LOG_DIR = os.path.join(os.path.expanduser("~"), ".flick")
os.makedirs(LOG_DIR, exist_ok=True)
BOOT_LOG = os.path.join(LOG_DIR, "boot.log")


def log_debug(msg: str):
    try:
        with open(BOOT_LOG, "a", encoding="utf-8") as f:
            f.write(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}\n")
    except Exception:
        pass


def acquire_single_instance() -> bool:
    """Ensures only a single instance of Flick Receiver is running at any given time."""
    global _mutex_handle
    kernel32 = ctypes.windll.kernel32
    kernel32.CreateMutexW.argtypes = [ctypes.c_void_p, wintypes.BOOL, wintypes.LPCWSTR]
    kernel32.CreateMutexW.restype = wintypes.HANDLE

    _mutex_handle = kernel32.CreateMutexW(None, False, MUTEX_NAME)
    ERROR_ALREADY_EXISTS = 183
    last_err = kernel32.GetLastError()
    log_debug(f"acquire_single_instance: handle={_mutex_handle}, last_err={last_err}")
    if last_err == ERROR_ALREADY_EXISTS:
        return False
    return True


def create_tray_icon_image() -> Image.Image:
    """Generates a 64x64 M3 Expressive lightning bolt icon with rounded background."""
    size = 64
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Rounded rectangle background (#5B4B9E - Deep Expressive Indigo)
    bg_color = (91, 75, 158, 255)
    draw.rounded_rectangle([2, 2, size - 3, size - 3], radius=16, fill=bg_color)

    # Electric Teal / White Lightning Bolt coordinates
    bolt_points = [
        (34, 10),
        (18, 33),
        (31, 33),
        (26, 54),
        (46, 27),
        (33, 27),
        (40, 10),
    ]
    bolt_color = (255, 255, 255, 255)
    draw.polygon(bolt_points, fill=bolt_color)

    # Accent highlight on the bolt edge
    accent_points = [
        (34, 10),
        (33, 27),
        (46, 27),
    ]
    draw.line(accent_points, fill=(116, 248, 229, 255), width=2)

    return img


def load_app_icon() -> Image.Image:
    """Loads bundled flick.ico or flick.png, or falls back to programmatic icon."""
    ico_path = get_resource_path("flick.ico")
    if os.path.exists(ico_path):
        try:
            return Image.open(ico_path)
        except Exception:
            pass

    png_path = get_resource_path("flick.png")
    if os.path.exists(png_path):
        try:
            return Image.open(png_path)
        except Exception:
            pass

    return create_tray_icon_image()


def setup_global_hotkeys(callback) -> list:
    """Registers global hotkeys Win+Shift+F and Ctrl+Alt+F."""
    registered = []
    for hk in ["windows+shift+f", "ctrl+alt+f"]:
        try:
            keyboard.add_hotkey(hk, callback)
            registered.append(hk)
        except Exception as e:
            safe_print(f"[Hotkey] Could not register '{hk}': {e}")
    return registered


def cleanup_global_hotkeys() -> None:
    """Removes all keyboard hooks on shutdown."""
    try:
        keyboard.clear_all_hotkeys()
    except Exception:
        pass


class FlickTrayApp:
    def __init__(self):
        self.config = load_config()
        self.icon: Optional[pystray.Icon] = None
        self.http_server = None
        self.beacon = None
        self._is_ready = False

    def notify(self, title: str, message: str) -> None:
        """Triggers a native Windows balloon notification / toast from the tray icon."""
        if self.icon and self._is_ready:
            try:
                self.icon.notify(message, title)
            except Exception as e:
                safe_print(f"[Tray] Notify error: {e}")

    def _open_downloads_folder(self, icon: Any = None, item: Any = None) -> None:
        save_dir = self.config.get("save_directory", os.path.expanduser("~/Downloads/Flick"))
        os.makedirs(save_dir, exist_ok=True)
        try:
            os.startfile(save_dir)
        except Exception as e:
            safe_print(f"[Tray] Could not open downloads folder: {e}")

    def _on_mode_selected(self, mode: str):
        def _handler(icon: Any, item: Any) -> None:
            set_clipboard_mode(mode)
            self.config = get_config()
            safe_print(f"[Tray] Clipboard mode changed to: {mode}")
            self.notify("⚡ Clipboard Mode", f"Switched to: {mode}")
        return _handler

    def _is_mode_checked(self, mode: str):
        def _check(item: Any) -> bool:
            return get_config().get("clipboard_mode", "SMART_MULTI") == mode
        return _check

    def _on_pair_clicked(self, icon: Any = None, item: Any = None) -> None:
        show_qr_pairing_modal()

    def _on_quit(self, icon: Any, item: Any) -> None:
        safe_print("[Tray] Shutting down Flick Receiver...")
        self._is_ready = False
        cleanup_global_hotkeys()
        stop_discovery_beacon()
        stop_http_receiver()
        if self.icon:
            self.icon.stop()
        sys.exit(0)

    def build_menu(self) -> pystray.Menu:
        lan_ip = get_lan_ip()
        port = self.config.get("http_port", 45455)
        status_label = f"Flick Receiver • {lan_ip}:{port}"

        mode_menu = pystray.Menu(
            pystray.MenuItem(
                "Smart Multi (File + Path + Image)",
                self._on_mode_selected("SMART_MULTI"),
                checked=self._is_mode_checked("SMART_MULTI"),
                radio=True,
            ),
            pystray.MenuItem(
                "Force Path Only (Terminal / Text)",
                self._on_mode_selected("FORCE_PATH"),
                checked=self._is_mode_checked("FORCE_PATH"),
                radio=True,
            ),
            pystray.MenuItem(
                "Force Image Only (Bitmap + File)",
                self._on_mode_selected("FORCE_IMAGE"),
                checked=self._is_mode_checked("FORCE_IMAGE"),
                radio=True,
            ),
            pystray.MenuItem(
                "Force File Drop Only (CF_HDROP)",
                self._on_mode_selected("FORCE_FILE"),
                checked=self._is_mode_checked("FORCE_FILE"),
                radio=True,
            ),
        )

        return pystray.Menu(
            pystray.MenuItem(status_label, action=None, enabled=False),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem("Pair New Phone (Win+Shift+F)...", self._on_pair_clicked, default=True),
            pystray.MenuItem("Clipboard Mode", mode_menu),
            pystray.MenuItem("Open Flick Downloads Folder", self._open_downloads_folder),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem("Quit Flick", self._on_quit),
        )

    def _setup_services(self, icon: pystray.Icon) -> None:
        """Executed by pystray in a dedicated thread as soon as the Win32 tray HWND is ready."""
        try:
            log_debug("_setup_services: start")
            self._is_ready = True
            icon.visible = True

            # 1. Wire HTTP server notifications to tray
            set_notification_callback(self.notify)

            # 2. Start HTTP streaming server
            try:
                self.http_server = start_http_receiver(port=self.config.get("http_port", 45455))
                log_debug("_setup_services: HTTP server started")
            except Exception as e:
                log_debug(f"[Flick] HTTP Server failed to start: {e}")
                self.notify("⚠️ Flick Port Error", f"Could not bind port {self.config.get('http_port', 45455)}: {e}")

            # 3. Start UDP Discovery Beacon
            try:
                self.beacon = start_discovery_beacon(port=self.config.get("udp_port", 45454))
                log_debug("_setup_services: Beacon started")
            except Exception as e:
                log_debug(f"[Flick] UDP Beacon failed to start: {e}")

            # 4. Register global hotkeys
            hks = setup_global_hotkeys(show_qr_pairing_modal)
            hk_str = " / ".join(hks) if hks else "Win+Shift+F"
            log_debug(f"_setup_services: Hotkeys registered: {hk_str}")

            # 5. Startup Notification
            lan_ip = get_lan_ip()
            port = self.config.get("http_port", 45455)
            self.notify("⚡ Flick Active", f"Ready on {lan_ip}:{port}\nPair Shortcut: {hk_str}")
            safe_print(f"[Flick] Receiver initialized successfully. Hotkeys: {hk_str}")
            log_debug("_setup_services: complete")
        except Exception as e:
            log_debug(f"_setup_services FATAL: {e}")

    def run(self) -> None:
        safe_print("=" * 60)
        safe_print("[Flick] Windows Multi-Format Clipboard Receiver")
        safe_print(f"Host: {self.config.get('pc_name')} | LAN IP: {get_lan_ip()}")
        safe_print(f"HTTP Port: {self.config.get('http_port', 45455)} | UDP Beacon: {self.config.get('udp_port', 45454)}")
        safe_print(f"Save Directory: {self.config.get('save_directory')}")
        safe_print("=" * 60)

        log_debug("run: loading app icon...")
        icon_img = load_app_icon()
        log_debug("run: building menu...")
        menu = self.build_menu()
        log_debug("run: instantiating pystray.Icon...")
        self.icon = pystray.Icon(
            name="Flick",
            icon=icon_img,
            title=f"Flick — {get_lan_ip()}:{self.config.get('http_port', 45455)}",
            menu=menu,
        )
        log_debug("run: calling self.icon.run()...")
        self.icon.run(setup=self._setup_services)
        log_debug("run: self.icon.run() returned!")


if __name__ == "__main__":
    import traceback

    log_debug(f"Process started: exe={sys.executable}, argv={sys.argv}")

    # 1. Single-Instance Check
    if not acquire_single_instance():
        log_debug("Single instance check failed: already running.")
        safe_print("[Flick] Another instance is already running.")
        try:
            show_qr_pairing_modal()
            time.sleep(1.0)
            from qr_modal import _qr_window_open
            while _qr_window_open:
                time.sleep(0.5)
        except Exception as e:
            log_debug(f"Error showing modal: {e}")
            try:
                ctypes.windll.user32.MessageBoxW(
                    0,
                    "Flick is already running in your Windows taskbar tray!\n\nTip: Press Win+Shift+F to open the QR Pairing window.",
                    "Flick Receiver",
                    0x40,  # MB_ICONINFORMATION
                )
            except Exception:
                pass
        sys.exit(0)

    log_dir = os.path.join(os.path.expanduser("~"), ".flick")
    os.makedirs(log_dir, exist_ok=True)
    log_path = os.path.join(log_dir, "flick_receiver.log")

    try:
        log_debug("Instantiating FlickTrayApp...")
        app = FlickTrayApp()
        log_debug("Running FlickTrayApp...")
        app.run()
    except Exception as e:
        log_debug(f"FATAL Exception: {e}")
        with open(log_path, "a", encoding="utf-8") as f:
            f.write(f"\nCrash at {time.strftime('%Y-%m-%d %H:%M:%S')}:\n")
            traceback.print_exc(file=f)
        try:
            ctypes.windll.user32.MessageBoxW(
                0,
                f"Flick encountered an unexpected error:\n\n{e}\n\nSee {log_path} for details.",
                "Flick Error",
                0x10,  # MB_ICONERROR
            )
        except Exception:
            pass
        sys.exit(1)

`

---

## 📄 Windows Receiver: QR Code Pairing Modal
File Path: windows-receiver/qr_modal.py

`python
"""
Flick Windows Receiver - QR Code Pairing Window
Generates and displays a high-contrast QR code for instant Android pairing without typing IPs or tokens.
"""

import json
import threading
import os
import tkinter as tk
from typing import Optional
from PIL import Image, ImageTk
import qrcode

from config_manager import get_config, get_lan_ip, get_resource_path

_qr_window_open = False
_current_root: Optional[tk.Tk] = None


def show_qr_pairing_modal():
    """Opens the Tkinter QR code pairing modal in a dedicated thread."""
    global _qr_window_open, _current_root
    if _qr_window_open and _current_root:
        try:
            _current_root.lift()
            _current_root.attributes("-topmost", True)
            _current_root.focus_force()
        except Exception:
            pass
        return

    def _run_gui():
        global _qr_window_open
        _qr_window_open = True
        try:
            cfg = get_config()
            lan_ip = get_lan_ip()
            payload = {
                "ip": lan_ip,
                "port": cfg.get("http_port", 45455),
                "pc_id": cfg.get("pc_id", ""),
                "pc_name": cfg.get("pc_name", "Windows PC"),
                "token": cfg.get("auth_token", "")
            }
            qr_text = json.dumps(payload)

            # Generate QR image
            qr = qrcode.QRCode(
                version=1,
                error_correction=qrcode.constants.ERROR_CORRECT_M,
                box_size=8,
                border=2,
            )
            qr.add_data(qr_text)
            qr.make(fit=True)
            pil_img = qr.make_image(fill_color="#1C192B", back_color="#FFFFFF").convert("RGB")

            # Setup Tkinter Root
            root = tk.Tk()
            global _current_root
            _current_root = root
            root.title("Flick — Pair New Phone")
            root.resizable(False, False)
            root.attributes("-topmost", True)
            root.configure(bg="#FDF8FF")

            # Try to set window icon
            ico_path = get_resource_path("flick.ico")
            if os.path.exists(ico_path):
                try:
                    root.iconbitmap(ico_path)
                except Exception:
                    pass

            # Center window on screen
            w, h = 380, 480
            sw = root.winfo_screenwidth()
            sh = root.winfo_screenheight()
            x = (sw - w) // 2
            y = (sh - h) // 2
            root.geometry(f"{w}x{h}+{x}+{y}")

            # Header Frame
            header_frame = tk.Frame(root, bg="#FDF8FF", pady=16)
            header_frame.pack(fill="x")

            title_lbl = tk.Label(
                header_frame,
                text="⚡ Pair with Flick",
                font=("Segoe UI", 16, "bold"),
                fg="#5B4B9E",
                bg="#FDF8FF"
            )
            title_lbl.pack()

            sub_lbl = tk.Label(
                header_frame,
                text="Scan with the Flick app on your Android phone",
                font=("Segoe UI", 10),
                fg="#5F5B71",
                bg="#FDF8FF"
            )
            sub_lbl.pack(pady=4)

            # QR Code Display
            tk_img = ImageTk.PhotoImage(pil_img)
            qr_lbl = tk.Label(root, image=tk_img, bg="#FFFFFF", relief="solid", borderwidth=1)
            qr_lbl.image = tk_img  # Keep reference
            qr_lbl.pack(padx=20, pady=10)

            # Device Info
            info_lbl = tk.Label(
                root,
                text=f"{payload['pc_name']} • {payload['ip']}:{payload['port']}",
                font=("Consolas", 10, "bold"),
                fg="#1C192B",
                bg="#EDE7FF",
                padx=12,
                pady=6
            )
            info_lbl.pack(pady=10)

            # Close Button
            close_btn = tk.Button(
                root,
                text="Done",
                command=root.destroy,
                font=("Segoe UI", 10, "bold"),
                bg="#5B4B9E",
                fg="#FFFFFF",
                activebackground="#4B388B",
                activeforeground="#FFFFFF",
                relief="flat",
                padx=24,
                pady=6,
                cursor="hand2"
            )
            close_btn.pack(pady=8)

            def on_close():
                global _qr_window_open, _current_root
                _qr_window_open = False
                _current_root = None
                root.destroy()

            root.protocol("WM_DELETE_WINDOW", on_close)
            root.mainloop()

        except Exception as e:
            print(f"[QRModal] Error displaying QR modal: {e}")
        finally:
            _qr_window_open = False
            _current_root = None

    t = threading.Thread(target=_run_gui, daemon=True, name="FlickQrModal")
    t.start()


if __name__ == "__main__":
    show_qr_pairing_modal()
    import time
    time.sleep(0.5)
    while _qr_window_open:
        time.sleep(0.5)

`

---

## 📄 Android App: Manifest & Permissions
File Path: android-app/app/src/main/AndroidManifest.xml

`xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">

    <uses-permission android:name="android.permission.INTERNET" />
    <uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" />
    <uses-permission android:name="android.permission.ACCESS_WIFI_STATE" />
    <uses-permission android:name="android.permission.VIBRATE" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE_DATA_SYNC" />
    <uses-permission android:name="android.permission.POST_NOTIFICATIONS" />
    <uses-permission android:name="android.permission.CAMERA" />

    <uses-feature
        android:name="android.hardware.camera"
        android:required="false" />

    <application
        android:allowBackup="false"
        android:icon="@drawable/ic_flick_bolt"
        android:label="@string/app_name"
        android:supportsRtl="true"
        android:usesCleartextTraffic="true"
        android:theme="@style/Theme.Flick">

        <!-- 1. M3 Expressive Companion Dashboard -->
        <activity
            android:name=".MainActivity"
            android:exported="true"
            android:theme="@style/Theme.Flick">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>

        <!-- 2. Zero-UI 1-Tap Share Target (<50ms close) -->
        <activity
            android:name=".TeleportShareActivity"
            android:exported="true"
            android:label="@string/share_target_label"
            android:theme="@style/Theme.Flick.TranslucentNoDisplay"
            android:excludeFromRecents="true"
            android:noHistory="true"
            android:taskAffinity="">
            <intent-filter>
                <action android:name="android.intent.action.SEND" />
                <category android:name="android.intent.category.DEFAULT" />
                <data android:mimeType="*/*" />
            </intent-filter>
            <intent-filter>
                <action android:name="android.intent.action.SEND_MULTIPLE" />
                <category android:name="android.intent.category.DEFAULT" />
                <data android:mimeType="*/*" />
            </intent-filter>
        </activity>

        <!-- 3. Chunked Streaming Foreground Service -->
        <service
            android:name=".service.UploadService"
            android:enabled="true"
            android:exported="false"
            android:foregroundServiceType="dataSync" />
    </application>
</manifest>

`

---

## 📄 Android App: Zero-UI Share Target Activity
File Path: android-app/app/src/main/java/com/eksend/flick/TeleportShareActivity.kt

`kotlin
package com.eksend.flick

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.core.content.ContextCompat
import com.eksend.flick.service.HapticHelper
import com.eksend.flick.service.UploadService

class TeleportShareActivity : Activity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        try {
            val intentAction = intent?.action
            val uriList = ArrayList<Uri>()
            var textPayload: String? = null

            when (intentAction) {
                Intent.ACTION_SEND -> {
                    val streamUri = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                        intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
                    } else {
                        @Suppress("DEPRECATION")
                        intent.getParcelableExtra(Intent.EXTRA_STREAM) as? Uri
                    }

                    if (streamUri != null) {
                        uriList.add(streamUri)
                    } else {
                        textPayload = intent.getStringExtra(Intent.EXTRA_TEXT)
                            ?: intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
                    }
                }

                Intent.ACTION_SEND_MULTIPLE -> {
                    val incoming = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                        intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java)
                    } else {
                        @Suppress("DEPRECATION")
                        intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM)
                    }
                    if (incoming != null) {
                        uriList.addAll(incoming)
                    }
                }
            }

            // Security Hardening: Only accept content:// URIs and reject Flick's own authority
            // Prevents malicious co-resident apps from forcing Flick to exfiltrate its private files
            val safeUris = ArrayList(uriList.filter { uri ->
                uri.scheme.equals("content", ignoreCase = true) &&
                !uri.authority.equals(packageName, ignoreCase = true)
            })

            if (safeUris.isNotEmpty() || !textPayload.isNullOrEmpty()) {
                HapticHelper.triggerQuickTick(this)
                val serviceIntent = Intent(this, UploadService::class.java).apply {
                    action = UploadService.ACTION_UPLOAD
                    putParcelableArrayListExtra(UploadService.EXTRA_URIS, safeUris)
                    putExtra(UploadService.EXTRA_TEXT, textPayload)
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }

                // Explicitly attach ClipData to pass temporary URI read permissions to the service
                if (safeUris.isNotEmpty()) {
                    val clipData = ClipData.newUri(contentResolver, "FlickFiles", safeUris[0])
                    for (i in 1 until safeUris.size) {
                        clipData.addItem(ClipData.Item(safeUris[i]))
                    }
                    serviceIntent.clipData = clipData
                }

                ContextCompat.startForegroundService(this, serviceIntent)
            }
        } finally {
            // Dismiss instantly in < 50ms with zero transition animation
            finish()
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                overrideActivityTransition(OVERRIDE_TRANSITION_CLOSE, 0, 0)
            } else {
                @Suppress("DEPRECATION")
                overridePendingTransition(0, 0)
            }
        }
    }
}

`

---

## 📄 Android App: Upload Foreground Service
File Path: android-app/app/src/main/java/com/eksend/flick/service/UploadService.kt

`kotlin
package com.eksend.flick.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.Uri
import android.os.Build
import android.os.IBinder
import android.provider.OpenableColumns
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import com.eksend.flick.R
import com.eksend.flick.data.FlickPreferences
import com.eksend.flick.data.TransferRecord
import com.eksend.flick.network.FlickHttpClient
import com.eksend.flick.network.PcAutoRouter
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class UploadService : Service() {

    private val serviceScope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    companion object {
        const val ACTION_UPLOAD = "com.eksend.flick.action.UPLOAD"
        const val EXTRA_URIS = "extra_uris"
        const val EXTRA_TEXT = "extra_text"
        const val EXTRA_CLIPBOARD_MODE = "extra_clipboard_mode"
        const val EXTRA_TARGET_PC_ID = "extra_target_pc_id"

        private const val CHANNEL_ID = "flick_transfers"
        private const val NOTIFICATION_ID = 45455
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onDestroy() {
        serviceScope.cancel()
        super.onDestroy()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val notification = buildForegroundNotification("Sending to PC...")
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ServiceCompat.startForeground(
                this,
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }

        if (intent != null) {
            handleIntent(intent)
        } else {
            stopSelf()
        }

        return START_NOT_STICKY
    }

    private fun handleIntent(intent: Intent) {
        val prefs = FlickPreferences(applicationContext)
        val pc = prefs.getActivePc()

        if (pc == null) {
            HapticHelper.triggerError(applicationContext, "No PC paired. Open Flick to pair.")
            stopSelf()
            return
        }

        val textPayload = intent.getStringExtra(EXTRA_TEXT)
        val uris: List<Uri> = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableArrayListExtra(EXTRA_URIS, Uri::class.java) ?: emptyList()
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableArrayListExtra(EXTRA_URIS) ?: emptyList()
        }
        val modeOverride = intent.getStringExtra(EXTRA_CLIPBOARD_MODE) ?: prefs.clipboardModeOverride

        serviceScope.launch {
            // 1. Resolve Target IP (< 150ms cached ping -> UDP fallback)
            val targetIp = PcAutoRouter.resolveTargetIp(applicationContext, pc)
            if (targetIp == null) {
                HapticHelper.triggerError(applicationContext, "${pc.pcName} is offline or unreachable on Wi-Fi")
                stopSelf()
                return@launch
            }

            // 2. Handle Plain Text Payload
            if (!textPayload.isNullOrEmpty() && uris.isEmpty()) {
                val start = System.currentTimeMillis()
                val result = FlickHttpClient.sendClipboardText(
                    targetIp = targetIp,
                    port = pc.port,
                    authToken = pc.authToken,
                    text = textPayload
                )
                val elapsed = System.currentTimeMillis() - start
                if (result.isSuccess) {
                    prefs.recordTransfer(
                        TransferRecord(
                            fileName = if (textPayload.length > 30) textPayload.take(30) + "..." else textPayload,
                            mimeType = "text/plain",
                            bytes = textPayload.toByteArray().size.toLong(),
                            elapsedMs = elapsed,
                            success = true
                        )
                    )
                    HapticHelper.triggerSuccess(applicationContext, "⚡ Copied to PC Clipboard")
                } else {
                    HapticHelper.triggerError(applicationContext, "Text teleport failed")
                }
                stopSelf()
                return@launch
            }

            // 3. Handle File URIs Payload
            if (uris.isNotEmpty()) {
                val totalCount = uris.size
                var allSuccess = true

                for ((index, uri) in uris.withIndex()) {
                    val metadata = queryUriMetadata(uri)
                    val start = System.currentTimeMillis()

                    val uploadRes = FlickHttpClient.uploadStream(
                        targetIp = targetIp,
                        port = pc.port,
                        authToken = pc.authToken,
                        fileName = metadata.name,
                        mimeType = metadata.mimeType,
                        clipboardMode = modeOverride,
                        batchIndex = index,
                        batchTotal = totalCount,
                        contentLength = metadata.size
                    ) {
                        contentResolver.openInputStream(uri)
                            ?: throw java.io.IOException("Cannot open input stream for $uri")
                    }

                    val elapsed = System.currentTimeMillis() - start
                    if (uploadRes.isSuccess) {
                        prefs.recordTransfer(
                            TransferRecord(
                                fileName = metadata.name,
                                mimeType = metadata.mimeType,
                                bytes = metadata.size,
                                elapsedMs = elapsed,
                                success = true
                            )
                        )
                    } else {
                        allSuccess = false
                    }
                }

                if (allSuccess) {
                    val msg = if (totalCount == 1) "⚡ Copied to PC Clipboard" else "⚡ $totalCount files copied to PC"
                    HapticHelper.triggerSuccess(applicationContext, msg)
                    kotlinx.coroutines.delay(160)
                } else {
                    HapticHelper.triggerError(applicationContext, "Transfer failed")
                    kotlinx.coroutines.delay(200)
                }
            }

            stopSelf()
        }
    }

    private data class UriMetadata(val name: String, val mimeType: String, val size: Long)

    private fun queryUriMetadata(uri: Uri): UriMetadata {
        var name = "flick_${System.currentTimeMillis()}"
        var size = -1L
        val mimeType = contentResolver.getType(uri) ?: "application/octet-stream"

        try {
            contentResolver.query(uri, null, null, null, null)?.use { cursor ->
                val nameIndex = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                val sizeIndex = cursor.getColumnIndex(OpenableColumns.SIZE)
                if (cursor.moveToFirst()) {
                    if (nameIndex != -1) name = cursor.getString(nameIndex) ?: name
                    if (sizeIndex != -1) size = cursor.getLong(sizeIndex)
                }
            }
        } catch (_: Exception) {}

        return UriMetadata(name, mimeType, size)
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Flick Transfers",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Shows progress during active Flick transfers to PC"
                setShowBadge(false)
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager?.createNotificationChannel(channel)
        }
    }

    private fun buildForegroundNotification(title: String): Notification {
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText("Teleporting to PC Clipboard...")
            .setSmallIcon(R.drawable.ic_flick_bolt)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setOngoing(true)
            .build()
    }
}

`

---

## 📄 Android App: Encrypted Preferences & PC Registry
File Path: android-app/app/src/main/java/com/eksend/flick/data/FlickPreferences.kt

`kotlin
package com.eksend.flick.data

import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

class FlickPreferences(context: Context) {
    private val prefs: SharedPreferences =
        context.applicationContext.getSharedPreferences("flick_prefs", Context.MODE_PRIVATE)

    companion object {
        private const val KEY_CLIENT_ID = "client_id"
        private const val KEY_CLIENT_NAME = "client_name"
        private const val KEY_ACTIVE_PC_ID = "active_pc_id"
        private const val KEY_PAIRED_PCS = "paired_pcs_json"
        private const val KEY_CLIPBOARD_MODE = "clipboard_mode_override"
        private const val KEY_HAPTIC_ENABLED = "haptic_feedback_enabled"
        private const val KEY_AUTO_UPDATE_IP = "auto_update_ip_enabled"
        private const val KEY_RECENT_TRANSFERS = "recent_transfers_json"
        private const val MAX_TRANSFERS = 20
    }

    val clientId: String
        get() {
            var id = prefs.getString(KEY_CLIENT_ID, null)
            if (id == null) {
                id = UUID.randomUUID().toString()
                prefs.edit().putString(KEY_CLIENT_ID, id).apply()
            }
            return id
        }

    val clientName: String
        get() {
            return prefs.getString(KEY_CLIENT_NAME, null)
                ?: "${Build.MANUFACTURER.replaceFirstChar { it.uppercase() }} ${Build.MODEL}"
        }

    var activePcId: String?
        get() = prefs.getString(KEY_ACTIVE_PC_ID, null)
        set(value) = prefs.edit().putString(KEY_ACTIVE_PC_ID, value).apply()

    var clipboardModeOverride: String
        get() = prefs.getString(KEY_CLIPBOARD_MODE, "DEFAULT") ?: "DEFAULT"
        set(value) = prefs.edit().putString(KEY_CLIPBOARD_MODE, value).apply()

    var isHapticFeedbackEnabled: Boolean
        get() = prefs.getBoolean(KEY_HAPTIC_ENABLED, true)
        set(value) = prefs.edit().putBoolean(KEY_HAPTIC_ENABLED, value).apply()

    var isAutoUpdateIpEnabled: Boolean
        get() = prefs.getBoolean(KEY_AUTO_UPDATE_IP, true)
        set(value) = prefs.edit().putBoolean(KEY_AUTO_UPDATE_IP, value).apply()

    fun getPairedPcs(): List<PairedPc> {
        val jsonStr = prefs.getString(KEY_PAIRED_PCS, "[]") ?: "[]"
        val list = mutableListOf<PairedPc>()
        try {
            val arr = JSONArray(jsonStr)
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                list.add(
                    PairedPc(
                        pcId = obj.getString("pcId"),
                        pcName = obj.getString("pcName"),
                        lastKnownIp = obj.getString("lastKnownIp"),
                        port = obj.optInt("port", 45455),
                        authToken = obj.getString("authToken"),
                        lastSeenEpochMs = obj.optLong("lastSeenEpochMs", System.currentTimeMillis())
                    )
                )
            }
        } catch (_: Exception) {}
        return list
    }

    fun savePairedPc(pc: PairedPc) {
        val current = getPairedPcs().toMutableList()
        val index = current.indexOfFirst { it.pcId == pc.pcId }
        if (index >= 0) {
            current[index] = pc
        } else {
            current.add(pc)
        }
        val arr = JSONArray()
        for (item in current) {
            val obj = JSONObject().apply {
                put("pcId", item.pcId)
                put("pcName", item.pcName)
                put("lastKnownIp", item.lastKnownIp)
                put("port", item.port)
                put("authToken", item.authToken)
                put("lastSeenEpochMs", item.lastSeenEpochMs)
            }
            arr.put(obj)
        }
        prefs.edit().putString(KEY_PAIRED_PCS, arr.toString()).apply()

        // If no active PC, set this as active
        if (activePcId == null || activePcId == pc.pcId) {
            activePcId = pc.pcId
        }
    }

    fun removePairedPc(pcId: String) {
        val current = getPairedPcs().filter { it.pcId != pcId }
        val arr = JSONArray()
        for (item in current) {
            val obj = JSONObject().apply {
                put("pcId", item.pcId)
                put("pcName", item.pcName)
                put("lastKnownIp", item.lastKnownIp)
                put("port", item.port)
                put("authToken", item.authToken)
                put("lastSeenEpochMs", item.lastSeenEpochMs)
            }
            arr.put(obj)
        }
        prefs.edit().putString(KEY_PAIRED_PCS, arr.toString()).apply()
        if (activePcId == pcId) {
            activePcId = current.firstOrNull()?.pcId
        }
    }

    fun getActivePc(): PairedPc? {
        val currentList = getPairedPcs()
        val activeId = activePcId
        return currentList.firstOrNull { it.pcId == activeId } ?: currentList.firstOrNull()
    }

    fun updateLastKnownIp(pcId: String, newIp: String) {
        val pcs = getPairedPcs().toMutableList()
        val idx = pcs.indexOfFirst { it.pcId == pcId }
        if (idx >= 0) {
            val old = pcs[idx]
            pcs[idx] = old.copy(lastKnownIp = newIp, lastSeenEpochMs = System.currentTimeMillis())
            val arr = JSONArray()
            for (item in pcs) {
                val obj = JSONObject().apply {
                    put("pcId", item.pcId)
                    put("pcName", item.pcName)
                    put("lastKnownIp", item.lastKnownIp)
                    put("port", item.port)
                    put("authToken", item.authToken)
                    put("lastSeenEpochMs", item.lastSeenEpochMs)
                }
                arr.put(obj)
            }
            prefs.edit().putString(KEY_PAIRED_PCS, arr.toString()).apply()
        }
    }

    fun getRecentTransfers(): List<TransferRecord> {
        val jsonStr = prefs.getString(KEY_RECENT_TRANSFERS, "[]") ?: "[]"
        val list = mutableListOf<TransferRecord>()
        try {
            val arr = JSONArray(jsonStr)
            for (i in 0 until arr.length()) {
                val obj = arr.getJSONObject(i)
                list.add(
                    TransferRecord(
                        id = obj.getString("id"),
                        fileName = obj.getString("fileName"),
                        mimeType = obj.getString("mimeType"),
                        bytes = obj.getLong("bytes"),
                        elapsedMs = obj.getLong("elapsedMs"),
                        success = obj.getBoolean("success"),
                        timestamp = obj.getLong("timestamp")
                    )
                )
            }
        } catch (_: Exception) {}
        return list
    }

    fun recordTransfer(record: TransferRecord) {
        val current = getRecentTransfers().toMutableList()
        current.add(0, record) // newest first
        if (current.size > MAX_TRANSFERS) {
            current.removeAt(current.size - 1)
        }
        val arr = JSONArray()
        for (item in current) {
            val obj = JSONObject().apply {
                put("id", item.id)
                put("fileName", item.fileName)
                put("mimeType", item.mimeType)
                put("bytes", item.bytes)
                put("elapsedMs", item.elapsedMs)
                put("success", item.success)
                put("timestamp", item.timestamp)
            }
            arr.put(obj)
        }
        prefs.edit().putString(KEY_RECENT_TRANSFERS, arr.toString()).apply()
    }
}

`

---

## 📄 Android App: HTTP Streaming Client & Ping
File Path: android-app/app/src/main/java/com/eksend/flick/network/FlickHttpClient.kt

`kotlin
package com.eksend.flick.network

import com.eksend.flick.data.ClipboardMode
import com.eksend.flick.data.PairedPc
import com.eksend.flick.data.PcOnlineState
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import okio.BufferedSink
import okio.source
import org.json.JSONObject
import java.io.InputStream
import java.net.URLEncoder
import java.util.concurrent.TimeUnit

data class UploadResult(
    val savedPath: String,
    val bytesWritten: Long,
    val elapsedMs: Long,
    val clipboardFormats: List<String>
)

object FlickHttpClient {

    private val baseClient = OkHttpClient.Builder()
        .retryOnConnectionFailure(true)
        .build()

    fun ping(
        ip: String,
        port: Int = 45455,
        authToken: String? = null,
        connectTimeoutMs: Long = 300
    ): Result<PcOnlineState> {
        val client = baseClient.newBuilder()
            .connectTimeout(connectTimeoutMs, TimeUnit.MILLISECONDS)
            .readTimeout(500, TimeUnit.MILLISECONDS)
            .build()

        val requestBuilder = Request.Builder()
            .url("http://$ip:$port/ping")
            .get()

        if (!authToken.isNullOrBlank()) {
            requestBuilder.addHeader("X-Flick-Token", authToken)
        }

        val start = System.currentTimeMillis()
        return try {
            client.newCall(requestBuilder.build()).execute().use { resp ->
                val elapsed = System.currentTimeMillis() - start
                if (!resp.isSuccessful) {
                    return Result.failure(Exception("HTTP error ${resp.code}"))
                }
                val bodyStr = resp.body?.string() ?: "{}"
                val json = JSONObject(bodyStr)
                val modeStr = json.optString("clipboard_mode", "SMART_MULTI")
                Result.success(
                    PcOnlineState(
                        isOnline = true,
                        pingMs = elapsed,
                        clipboardMode = ClipboardMode.fromId(modeStr),
                        lastChecked = System.currentTimeMillis()
                    )
                )
            }
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    fun pairWithToken(
        ip: String,
        port: Int = 45455,
        clientId: String,
        clientName: String,
        token: String
    ): Result<PairedPc> {
        val client = baseClient.newBuilder()
            .connectTimeout(3, TimeUnit.SECONDS)
            .readTimeout(5, TimeUnit.SECONDS)
            .build()

        val jsonBody = JSONObject().apply {
            put("client_id", clientId)
            put("client_name", clientName)
            put("token", token)
        }

        val request = Request.Builder()
            .url("http://$ip:$port/pair")
            .post(jsonBody.toString().toRequestBody("application/json".toMediaTypeOrNull()))
            .build()

        return try {
            client.newCall(request).execute().use { resp ->
                if (!resp.isSuccessful) {
                    return Result.failure(Exception("Pairing failed with code ${resp.code}"))
                }
                val json = JSONObject(resp.body?.string() ?: "{}")
                val pcId = json.getString("pc_id")
                val pcName = json.getString("pc_name")
                val authToken = json.getString("token")
                Result.success(
                    PairedPc(
                        pcId = pcId,
                        pcName = pcName,
                        lastKnownIp = ip,
                        port = port,
                        authToken = authToken,
                        lastSeenEpochMs = System.currentTimeMillis()
                    )
                )
            }
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    fun requestLanPair(
        ip: String,
        port: Int = 45455,
        clientId: String,
        clientName: String,
        pairCode: String
    ): Result<PairedPc> {
        // LAN pairing prompt on PC can take up to 30 seconds for user to click Yes
        val client = baseClient.newBuilder()
            .connectTimeout(3, TimeUnit.SECONDS)
            .readTimeout(35, TimeUnit.SECONDS)
            .build()

        val jsonBody = JSONObject().apply {
            put("client_id", clientId)
            put("client_name", clientName)
            put("pair_code", pairCode)
        }

        val request = Request.Builder()
            .url("http://$ip:$port/pair")
            .post(jsonBody.toString().toRequestBody("application/json".toMediaTypeOrNull()))
            .build()

        return try {
            client.newCall(request).execute().use { resp ->
                if (!resp.isSuccessful) {
                    return Result.failure(Exception("Pairing rejected on PC (code ${resp.code})"))
                }
                val json = JSONObject(resp.body?.string() ?: "{}")
                val pcId = json.getString("pc_id")
                val pcName = json.getString("pc_name")
                val authToken = json.getString("token")
                Result.success(
                    PairedPc(
                        pcId = pcId,
                        pcName = pcName,
                        lastKnownIp = ip,
                        port = port,
                        authToken = authToken,
                        lastSeenEpochMs = System.currentTimeMillis()
                    )
                )
            }
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    fun sendClipboardText(
        targetIp: String,
        port: Int = 45455,
        authToken: String,
        text: String
    ): Result<Unit> {
        val client = baseClient.newBuilder()
            .connectTimeout(3, TimeUnit.SECONDS)
            .readTimeout(5, TimeUnit.SECONDS)
            .build()

        val request = Request.Builder()
            .url("http://$targetIp:$port/clipboard")
            .addHeader("X-Flick-Token", authToken)
            .post(text.toRequestBody("text/plain; charset=utf-8".toMediaTypeOrNull()))
            .build()

        return try {
            client.newCall(request).execute().use { resp ->
                if (!resp.isSuccessful) {
                    Result.failure(Exception("Clipboard send failed with code ${resp.code}"))
                } else {
                    Result.success(Unit)
                }
            }
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    fun uploadStream(
        targetIp: String,
        port: Int = 45455,
        authToken: String,
        fileName: String,
        mimeType: String,
        clipboardMode: String = "DEFAULT",
        batchIndex: Int = 0,
        batchTotal: Int = 1,
        contentLength: Long = -1L,
        streamSupplier: () -> InputStream
    ): Result<UploadResult> {
        val client = baseClient.newBuilder()
            .connectTimeout(5, TimeUnit.SECONDS)
            .writeTimeout(60, TimeUnit.SECONDS)
            .readTimeout(60, TimeUnit.SECONDS)
            .build()

        val encodedFileName = URLEncoder.encode(fileName, "UTF-8")

        val body = object : RequestBody() {
            override fun contentType() = (mimeType.ifEmpty { "application/octet-stream" }).toMediaTypeOrNull()

            override fun contentLength(): Long = contentLength

            override fun writeTo(sink: BufferedSink) {
                streamSupplier().use { input ->
                    sink.writeAll(input.source())
                }
            }
        }

        val request = Request.Builder()
            .url("http://$targetIp:$port/upload")
            .addHeader("X-Flick-Token", authToken)
            .addHeader("X-File-Name", encodedFileName)
            .addHeader("X-Mime-Type", mimeType.ifEmpty { "application/octet-stream" })
            .addHeader("X-Clipboard-Mode", clipboardMode)
            .addHeader("X-Batch-Index", batchIndex.toString())
            .addHeader("X-Batch-Total", batchTotal.toString())
            .post(body)
            .build()

        return try {
            client.newCall(request).execute().use { resp ->
                if (!resp.isSuccessful) {
                    return Result.failure(Exception("Upload failed with HTTP ${resp.code}"))
                }
                val bodyStr = resp.body?.string() ?: "{}"
                val json = JSONObject(bodyStr)
                val formatsArr = json.optJSONArray("clipboard_formats")
                val formatsList = mutableListOf<String>()
                if (formatsArr != null) {
                    for (i in 0 until formatsArr.length()) {
                        formatsList.add(formatsArr.getString(i))
                    }
                }
                Result.success(
                    UploadResult(
                        savedPath = json.optString("saved_path"),
                        bytesWritten = json.optLong("bytes_written"),
                        elapsedMs = json.optLong("elapsed_ms"),
                        clipboardFormats = formatsList
                    )
                )
            }
        } catch (e: Exception) {
            Result.failure(e)
        }
    }
}

`

---

## 📄 Android App: UDP Discovery Listener
File Path: android-app/app/src/main/java/com/eksend/flick/network/UdpDiscoveryListener.kt

`kotlin
package com.eksend.flick.network

import com.eksend.flick.data.DiscoveredPc
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress

class UdpDiscoveryListener {

    private val _discoveredPcs = MutableStateFlow<Map<String, DiscoveredPc>>(emptyMap())
    val discoveredPcs: StateFlow<Map<String, DiscoveredPc>> = _discoveredPcs.asStateFlow()

    private var socket: DatagramSocket? = null
    private var job: Job? = null

    fun startListening(scope: CoroutineScope, port: Int = 45454) {
        if (job != null && job?.isActive == true) return

        job = scope.launch(Dispatchers.IO) {
            try {
                val sock = DatagramSocket(null).apply {
                    reuseAddress = true
                    broadcast = true
                    bind(InetSocketAddress(port))
                    soTimeout = 2000
                }
                socket = sock
                val buffer = ByteArray(4096)

                while (isActive) {
                    val packet = DatagramPacket(buffer, buffer.size)
                    try {
                        sock.receive(packet)
                        val text = String(packet.data, 0, packet.length, Charsets.UTF_8)
                        val json = JSONObject(text)
                        if (json.optString("protocol") == "FLICK_DISCOVERY_V1") {
                            val pcId = json.getString("pc_id")
                            val rawPcName = json.getString("pc_name")
                            val pcName = rawPcName.take(32)
                            val httpPort = json.optInt("http_port", 45455)
                            val version = json.optString("version", "1.0.0")

                            // Security Hardening: Enforce actual UDP packet source IP (defeats beacon IP spoofing/redirection)
                            val packetSourceIp = packet.address?.hostAddress ?: ""
                            if (packetSourceIp.isBlank() || httpPort !in 1024..65535) continue

                            val discovered = DiscoveredPc(
                                pcId = pcId,
                                pcName = pcName,
                                ip = packetSourceIp,
                                port = httpPort,
                                version = version,
                                timestamp = System.currentTimeMillis()
                            )

                            val updated = _discoveredPcs.value.toMutableMap()
                            // Limit map size to 32 entries to prevent memory flooding
                            if (updated.size >= 32 && !updated.containsKey(pcId)) {
                                val oldestKey = updated.minByOrNull { it.value.timestamp }?.key
                                if (oldestKey != null) updated.remove(oldestKey)
                            }
                            updated[pcId] = discovered
                            _discoveredPcs.value = updated
                        }
                    } catch (_: java.net.SocketTimeoutException) {
                        // Regular timeout to allow checking isActive
                    } catch (_: Exception) {
                        // Transient network or JSON parse errors
                    }
                }
            } catch (_: Exception) {
            } finally {
                socket?.close()
                socket = null
            }
        }
    }

    fun stopListening() {
        job?.cancel()
        job = null
        try {
            socket?.close()
        } catch (_: Exception) {}
        socket = null
    }
}

`

---

## 📄 Android App: PC Auto Router
File Path: android-app/app/src/main/java/com/eksend/flick/network/PcAutoRouter.kt

`kotlin
package com.eksend.flick.network

import android.content.Context
import com.eksend.flick.data.FlickPreferences
import com.eksend.flick.data.PairedPc
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress

object PcAutoRouter {

    suspend fun resolveTargetIp(context: Context, pc: PairedPc): String? = withContext(Dispatchers.IO) {
        // Step 1: Fast Path (< 150ms cached IP ping)
        val fastPingResult = FlickHttpClient.ping(
            ip = pc.lastKnownIp,
            port = pc.port,
            authToken = pc.authToken,
            connectTimeoutMs = 150
        )
        if (fastPingResult.isSuccess) {
            return@withContext pc.lastKnownIp
        }

        // Step 2: UDP Discovery Beacon Fallback (<= 2200ms)
        val prefs = FlickPreferences(context)
        if (prefs.isAutoUpdateIpEnabled) {
            var discoveredIp: String? = null
            var socket: DatagramSocket? = null
            try {
                socket = DatagramSocket(null).apply {
                    reuseAddress = true
                    broadcast = true
                    bind(InetSocketAddress(45454))
                    soTimeout = 2200
                }
                val buffer = ByteArray(2048)
                val packet = DatagramPacket(buffer, buffer.size)
                val endTime = System.currentTimeMillis() + 2200

                while (System.currentTimeMillis() < endTime) {
                    try {
                        socket.receive(packet)
                        val text = String(packet.data, 0, packet.length, Charsets.UTF_8)
                        val json = JSONObject(text)
                        if (json.optString("protocol") == "FLICK_DISCOVERY_V1" && json.optString("pc_id") == pc.pcId) {
                            val ip = packet.address?.hostAddress ?: ""
                            if (ip.isNotEmpty()) {
                                discoveredIp = ip
                                break
                            }
                        }
                    } catch (_: java.net.SocketTimeoutException) {
                        break
                    }
                }
            } catch (_: Exception) {
            } finally {
                socket?.close()
            }

            if (discoveredIp != null) {
                // Verify new IP with quick ping
                val verifyResult = FlickHttpClient.ping(
                    ip = discoveredIp,
                    port = pc.port,
                    authToken = pc.authToken,
                    connectTimeoutMs = 300
                )
                if (verifyResult.isSuccess) {
                    prefs.updateLastKnownIp(pc.pcId, discoveredIp)
                    return@withContext discoveredIp
                }
            }
        }

        // Offline or unreachable
        return@withContext null
    }
}

`

---
