"""
Flick Windows Receiver - Threaded HTTP Streaming Receiver
Serves endpoints on TCP port 45455:
  GET  /ping      - Health check, latency ping, auth verification
  POST /pair      - QR-based or LAN-prompt pairing handshake
  POST /upload    - High-throughput binary file streaming & clipboard injection
  POST /clipboard - Instant plain text / URL teleport to Windows clipboard
"""

import ctypes
import hashlib
import hmac
import json
import os
import re
import shutil
import ssl
import tempfile
import threading
import time
import unicodedata
import urllib.parse
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

import crypto_manager
from cryptography import x509
from cryptography.hazmat.primitives import serialization

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
from config_manager import add_paired_device, get_config, get_lan_ip, is_cert_paired, record_pc_transfer

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

# Ephemeral pairing secret state for Out-of-Band QR handshake
PAIR_SECRET_FILE = Path.home() / ".flick" / "pair_secret.json"
_pair_secret_lock = threading.Lock()
_active_pair_secret: Optional[str] = None
_active_pair_secret_expiry: float = 0.0


def set_active_pair_secret(secret: str, duration_sec: float = 300.0) -> None:
    """Sets the ephemeral 128-bit pairing secret displayed in the QR code (valid for duration_sec)."""
    global _active_pair_secret, _active_pair_secret_expiry
    now = time.time()
    with _pair_secret_lock:
        _active_pair_secret = secret
        _active_pair_secret_expiry = now + duration_sec
        try:
            PAIR_SECRET_FILE.parent.mkdir(parents=True, exist_ok=True)
            with open(PAIR_SECRET_FILE, "w", encoding="utf-8") as f:
                json.dump({"secret": secret, "expiry": now + duration_sec}, f)
        except Exception as e:
            safe_print(f"[HTTP] Failed to write pair_secret.json: {e}")
        safe_print(f"[HTTP] Set active ephemeral pair secret (valid {duration_sec}s)")


def get_active_pair_secret() -> Tuple[Optional[str], float]:
    """Retrieves active ephemeral pair secret from memory or ~/.flick/pair_secret.json."""
    global _active_pair_secret, _active_pair_secret_expiry
    with _pair_secret_lock:
        if _active_pair_secret and time.time() <= _active_pair_secret_expiry:
            return _active_pair_secret, _active_pair_secret_expiry
        if PAIR_SECRET_FILE.exists():
            try:
                with open(PAIR_SECRET_FILE, "r", encoding="utf-8") as f:
                    data = json.load(f)
                s = data.get("secret")
                exp = float(data.get("expiry", 0))
                if s and time.time() <= exp:
                    _active_pair_secret = s
                    _active_pair_secret_expiry = exp
                    return s, exp
            except Exception:
                pass
        return None, 0.0


def clear_active_pair_secret() -> None:
    """Invalidates active pair secret after successful handshake."""
    global _active_pair_secret, _active_pair_secret_expiry
    with _pair_secret_lock:
        _active_pair_secret = None
        _active_pair_secret_expiry = 0.0
        try:
            if PAIR_SECRET_FILE.exists():
                PAIR_SECRET_FILE.unlink()
        except Exception:
            pass


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
    Synthesizes a sleek, gentle dual-tone arpeggio in memory (PCM 16-bit 44.1kHz).
    Uses C5 (523Hz) transitioning to G5 (784Hz) with smooth exponential decay.
    """
    sample_rate = 44100
    duration = 0.38
    num_samples = int(sample_rate * duration)

    buf = io.BytesIO()
    with wave.open(buf, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(sample_rate)

        frames = bytearray()
        for i in range(num_samples):
            t = i / sample_rate
            if t < 0.09:
                freq = 523.25
                decay = math.exp(-t * 8.0)
                attack = min(1.0, t / 0.015)
            else:
                t2 = t - 0.09
                freq = 783.99
                decay = math.exp(-t2 * 7.0)
                attack = min(1.0, t2 / 0.015)

            sample = math.sin(2 * math.pi * freq * t) + 0.25 * math.sin(4 * math.pi * freq * t)
            val = int(sample * attack * decay * 14000)
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

    def _get_client_cert_der(self) -> Optional[bytes]:
        """Retrieves raw X.509 DER certificate bytes from the underlying TLS connection."""
        try:
            if hasattr(self.connection, "getpeercert"):
                return self.connection.getpeercert(binary_form=True)
        except Exception:
            pass
        return None

    def _get_client_pins(self) -> Tuple[str, str]:
        """Computes (spki_sha256, cert_sha256) of the connected client certificate."""
        der = self._get_client_cert_der()
        if not der:
            return "", ""
        try:
            cert_obj = x509.load_der_x509_certificate(der)
            spki_bytes = cert_obj.public_key().public_bytes(
                serialization.Encoding.DER,
                serialization.PublicFormat.SubjectPublicKeyInfo,
            )
            spki_pin = hashlib.sha256(spki_bytes).hexdigest().lower()
            cert_pin = hashlib.sha256(der).hexdigest().lower()
            return spki_pin, cert_pin
        except Exception:
            return "", ""

    def _verify_auth(self) -> bool:
        """
        Verifies that the connected client is authenticated:
        1. Checks whether the client's public key (SPKI or Cert SHA-256) matches a paired device.
        2. Fallback: validates X-Flick-Token header for legacy/local testing.
        """
        spki_pin, cert_pin = self._get_client_pins()
        if spki_pin and is_cert_paired(spki_pin, cert_pin):
            return True

        # Fallback for localhost testing or legacy bearer token
        provided = self.headers.get("X-Flick-Token", "")
        cfg = get_config()
        expected = cfg.get("auth_token", "")
        if provided and expected and hmac.compare_digest(provided, expected):
            return True

        return False

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
                "version": "2.0.1"
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

        # Local IPC endpoints from localhost only
        if self.client_address[0] in ("127.0.0.1", "::1"):
            if parsed_url.path == "/v2/open-dashboard":
                import desktop_dashboard
                threading.Thread(target=desktop_dashboard.show_desktop_dashboard, daemon=True).start()
                self._send_json_response(200, {"status": "ok", "message": "Dashboard opened"})
                return

            if parsed_url.path == "/v2/open-pair":
                import qr_modal
                threading.Thread(target=qr_modal.show_qr_pairing_modal, daemon=True).start()
                self._send_json_response(200, {"status": "ok", "message": "Pair QR opened"})
                return

        if parsed_url.path == "/v2/pair":
            self._handle_v2_pair()
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

    def _handle_v2_pair(self) -> None:
        """
        Zero-Trust Mutual TLS 1.3 Pairing Endpoint:
        Verifies proof of possession: proof = HMAC-SHA256(pair_secret, client_der_cert).
        On success, permanently registers the client's public key fingerprint in config.
        """
        global _active_pair_secret, _active_pair_secret_expiry
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

        client_der = self._get_client_cert_der()
        if not client_der:
            self._send_json_response(400, {"status": "error", "message": "Client certificate missing from mTLS handshake"})
            return

        active_secret, expiry = get_active_pair_secret()
        now = time.time()
        if not active_secret or now > expiry:
            self._send_json_response(403, {
                "status": "rejected",
                "message": "Pairing session expired or not active. Please open QR code on PC to pair."
            })
            return

        client_id = str(req.get("client_id", "unknown-client"))[:64]
        raw_client_name = str(req.get("client_name", "Android Device"))
        client_name = _BAD_FILENAME_CHARS.sub("", raw_client_name).strip()[:32] or "Android Device"
        provided_proof = str(req.get("proof", "")).lower().strip()

        try:
            cert_obj = x509.load_der_x509_certificate(client_der)
            spki_bytes = cert_obj.public_key().public_bytes(
                serialization.Encoding.DER,
                serialization.PublicFormat.SubjectPublicKeyInfo,
            )
            # Accept HMAC over full DER certificate or SPKI bytes
            expected_der_proof = hmac.new(active_secret.encode("utf-8"), client_der, hashlib.sha256).hexdigest().lower()
            expected_spki_proof = hmac.new(active_secret.encode("utf-8"), spki_bytes, hashlib.sha256).hexdigest().lower()

            if not (hmac.compare_digest(provided_proof, expected_der_proof) or hmac.compare_digest(provided_proof, expected_spki_proof)):
                safe_print(f"[Pairing] Invalid HMAC proof of possession from {client_name}")
                self._send_json_response(403, {"status": "rejected", "message": "Invalid certificate proof of possession"})
                return

            client_spki_pin = hashlib.sha256(spki_bytes).hexdigest().lower()
            client_cert_pin = hashlib.sha256(client_der).hexdigest().lower()

            # Register paired client permanently
            add_paired_device(
                client_id=client_id,
                client_name=client_name,
                client_spki_pin=client_spki_pin,
                client_cert_pin=client_cert_pin,
                pair_secret=active_secret
            )

            # Consume single-use ephemeral secret
            clear_active_pair_secret()

            cfg = get_config()
            play_chime()
            trigger_notification("⚡ Flick Paired", f"Securely paired with {client_name}")
            safe_print(f"[Pairing] Successfully paired with {client_name} (SPKI: {client_spki_pin[:16]}...)")

            self._send_json_response(200, {
                "status": "paired",
                "pc_id": cfg.get("pc_id", ""),
                "pc_name": cfg.get("pc_name", "")
            })
        except Exception as e:
            safe_print(f"[Pairing] Error during v2 pairing: {e}")
            self._send_json_response(500, {"status": "error", "message": f"Pairing error: {e}"})

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
            record_pc_transfer(os.path.basename(dest_path), bytes_written, self.client_address[0], "FILE")

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
                trigger_notification("File Quarantined", "Saved with .blocked extension for security")
            elif len(files_to_inject) == 1:
                base = os.path.basename(files_to_inject[0])
                trigger_notification("Flick Received", f"'{base}' ready to paste")
            else:
                trigger_notification("Flick Received", f"{len(files_to_inject)} files copied to clipboard")

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
        preview_text = text[:40].replace("\n", " ") + ("..." if len(text) > 40 else "")
        record_pc_transfer(preview_text, len(body_bytes), self.client_address[0], "TEXT")
        cfg = get_config()
        if cfg.get("play_sound", True):
            play_chime()

        # REDACTED: Omit raw text preview from Windows notification toast (prevents OTP/password leak on lock screen)
        trigger_notification("Flick Clipboard", f"Text copied to clipboard ({len(text)} chars)")

        self._send_json_response(200, {
            "status": "ok",
            "characters_copied": len(text),
            "clipboard_formats": formats
        })


class SecureThreadingHTTPServer(ThreadingHTTPServer):
    """Threading HTTPServer with per-connection TLS 1.3 mTLS socket wrapping."""
    def __init__(
        self,
        server_address: Tuple[str, int],
        RequestHandlerClass,
        ssl_context: Optional[ssl.SSLContext] = None
    ):
        super().__init__(server_address, RequestHandlerClass)
        self.ssl_context = ssl_context

    def finish_request(self, request, client_address) -> None:
        """Wraps incoming client TCP connection with TLS 1.3 mTLS context inside the worker thread."""
        if self.ssl_context is not None:
            try:
                ssl_sock = self.ssl_context.wrap_socket(request, server_side=True)
            except Exception as e:
                safe_print(f"[TLS] Handshake failed from {client_address}: {e}")
                try:
                    request.close()
                except Exception:
                    pass
                return
            try:
                self.RequestHandlerClass(ssl_sock, client_address, self)
            finally:
                try:
                    ssl_sock.close()
                except Exception:
                    pass
        else:
            super().finish_request(request, client_address)


class ThreadedHttpReceiver:
    def __init__(self, port: int = SERVER_PORT, ssl_context: Optional[ssl.SSLContext] = None):
        self.port = port
        self.ssl_context = ssl_context
        self.server: Optional[SecureThreadingHTTPServer] = None
        self._thread: Optional[threading.Thread] = None

    def start(self) -> None:
        try:
            if self.ssl_context is None:
                cfg = get_config()
                cert, priv_pem, _ = crypto_manager.get_or_create_pc_credentials(cfg.get("pc_id", ""))
                self.ssl_context = crypto_manager.build_server_ssl_context(cert, priv_pem)

            self.server = SecureThreadingHTTPServer(
                ("0.0.0.0", self.port),
                FlickRequestHandler,
                ssl_context=self.ssl_context
            )
            self._thread = threading.Thread(target=self.server.serve_forever, daemon=True, name="FlickHttpServer")
            self._thread.start()
            scheme = "HTTPS (mTLS 1.3)" if self.ssl_context else "HTTP"
            safe_print(f"[HttpReceiver] {scheme} Receiver server listening on 0.0.0.0:{self.port}")
        except OSError as e:
            if getattr(e, "winerror", None) == 10048 or "already in use" in str(e).lower():
                safe_print(f"[HttpReceiver] Port {self.port} already active (Rust daemon). Reusing running server.")
                return
            safe_print(f"[HttpReceiver] Port {self.port} bind error: {e}")
            raise

    def stop(self) -> None:
        if self.server:
            try:
                self.server.shutdown()
                self.server.server_close()
            except Exception:
                pass
            self.server = None
        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=1.0)
            self._thread = None
        safe_print("[HttpReceiver] HTTP Receiver server stopped.")


_global_receiver: Optional[ThreadedHttpReceiver] = None


def start_http_receiver(port: int = SERVER_PORT, ssl_context: Optional[ssl.SSLContext] = None) -> ThreadedHttpReceiver:
    global _global_receiver
    if _global_receiver is None:
        _global_receiver = ThreadedHttpReceiver(port=port, ssl_context=ssl_context)
    _global_receiver.start()
    return _global_receiver


def stop_http_receiver() -> None:
    global _global_receiver
    if _global_receiver:
        _global_receiver.stop()
        _global_receiver = None
