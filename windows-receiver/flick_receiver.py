"""
Flick Windows Receiver - Main Entrypoint & Desktop Application
Runs the mTLS 1.3 HTTP server, UDP discovery beacon, system tray icon, and Desktop Dashboard GUI.
"""

import ctypes
from ctypes import wintypes
import os
import ssl
import sys
import threading
import time
from typing import Any, Optional
from PIL import Image, ImageDraw
import pystray

from win32_theme import enable_high_dpi_awareness
enable_high_dpi_awareness()

# -------------------------------------------------------------
# APP USER MODEL ID (AUMID) & SILENT TOAST MONKEYPATCH
# -------------------------------------------------------------
AUMID = "Flick.WindowsReceiver"
try:
    ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(AUMID)
except Exception:
    pass

try:
    import winreg
    reg_path = r"Software\Classes\AppUserModelId\Flick.WindowsReceiver"
    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, reg_path) as key:
        winreg.SetValueEx(key, "DisplayName", 0, winreg.REG_SZ, "Flick")
        ico_file = os.path.abspath(os.path.join(os.path.dirname(__file__), "flick.ico"))
        if os.path.exists(ico_file):
            winreg.SetValueEx(key, "IconUri", 0, winreg.REG_SZ, ico_file)
except Exception:
    pass

try:
    import pystray._win32 as _w32
    from win32com.propsys import propsys, pscon

    # 1. Silences Windows default loud system notification chime (NIIF_NOSOUND = 0x10)
    # This guarantees only Flick's gentle soft chime plays without doubling.
    def _silent_notify(self, message, title=None):
        self._message(
            _w32.win32.NIM_MODIFY,
            _w32.win32.NIF_INFO,
            szInfo=message,
            szInfoTitle=title or self.title or '',
            dwInfoFlags=0x10 | 0x01
        )
    _w32.Icon._notify = _silent_notify

    # 2. Associates tray notification window with AUMID so toast displays "Flick" instead of "Python"
    _orig_create_win = _w32.Icon._create_window
    def _aumid_create_win(self, atom):
        hwnd = _orig_create_win(self, atom)
        try:
            store = propsys.SHGetPropertyStoreForWindow(hwnd, propsys.IID_IPropertyStore)
            val = propsys.PROPVARIANTType(AUMID)
            store.SetValue(pscon.PKEY_AppUserModel_ID, val)
            store.Commit()
        except Exception:
            pass
        return hwnd
    _w32.Icon._create_window = _aumid_create_win
except Exception:
    pass

if sys.stdout and hasattr(sys.stdout, "reconfigure"):
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:
        pass


def safe_print(*args, **kwargs):
    if sys.stdout is not None:
        try:
            print(*args, **kwargs, flush=True)
        except Exception:
            pass


from config_manager import (
    get_config,
    get_lan_ip,
    get_resource_path,
    load_config,
    set_clipboard_mode,
)
import crypto_manager
from discovery_beacon import start_discovery_beacon, stop_discovery_beacon
from http_receiver import set_notification_callback, start_http_receiver, stop_http_receiver
import desktop_dashboard

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
    """Generates a 64x64 Concept 4.1 '3 Primitives' icon (Phone + PC + Orange Flick)."""
    size = 64
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Substrate rounded rectangle (#0C0D10 - Matte Dark Chassis)
    bg_color = (12, 13, 16, 255)
    draw.rounded_rectangle([2, 2, size - 3, size - 3], radius=14, fill=bg_color)

    # 1. Phone: Vertical portrait pill (Left spine of 'F')
    draw.rounded_rectangle([17, 14, 24, 50], radius=4, fill=(232, 237, 242, 255))

    # 2. PC Screen: Horizontal landscape pill (Top bar of 'F')
    draw.rounded_rectangle([27, 14, 48, 22], radius=4, fill=(232, 237, 242, 255))

    # 3. The Flick: Airborne Signal Orange pill (Middle bar of 'F')
    draw.rounded_rectangle([29, 30, 43, 38], radius=4, fill=(255, 68, 0, 255))
    draw.ellipse([38, 33, 41, 35], fill=(255, 255, 255, 255))

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


class FlickTrayApp:
    def __init__(self):
        self.config = load_config()
        self.icon: Optional[pystray.Icon] = None
        self.http_server = None
        self.beacon = None
        self._is_ready = False
        self._init_tray()

    def _init_tray(self):
        icon_img = load_app_icon()
        menu = self.build_menu()
        self.icon = pystray.Icon(
            name="Flick",
            icon=icon_img,
            title=f"Flick — {get_lan_ip()}:{self.config.get('http_port', 45455)}",
            menu=menu,
        )

    def notify(self, title: str, message: str) -> None:
        """Triggers a native Windows notification toast from the tray icon."""
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

    def _on_dashboard_clicked(self, icon: Any = None, item: Any = None) -> None:
        desktop_dashboard.show_desktop_dashboard()

    def shutdown(self) -> None:
        """Stops background services and terminates cleanly."""
        safe_print("[Flick] Shutting down services...")
        self._is_ready = False
        stop_discovery_beacon()
        stop_http_receiver()
        if self.icon:
            try:
                self.icon.stop()
            except Exception:
                pass

    def _on_quit(self, icon: Any, item: Any) -> None:
        self.shutdown()
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
            pystray.MenuItem("⚡ Open Dashboard", self._on_dashboard_clicked, default=True),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem(status_label, action=None, enabled=False),
            pystray.MenuItem("Clipboard Mode", mode_menu),
            pystray.MenuItem("Open Saved Files Folder", self._open_downloads_folder),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem("Quit Flick", self._on_quit),
        )


def signal_existing_instance(cmd: str = "open-dashboard") -> bool:
    """Signals an already-running Flick instance via localhost HTTPS endpoint."""
    try:
        import urllib.request
        ctx = ssl.create_default_context()
        ctx.check_hostname = False
        ctx.verify_mode = ssl.CERT_NONE
        data = b"{}"
        req = urllib.request.Request(
            f"https://127.0.0.1:45455/v2/{cmd}",
            data=data,
            headers={"Content-Type": "application/json", "Content-Length": str(len(data))},
            method="POST"
        )
        with urllib.request.urlopen(req, context=ctx, timeout=2.5) as resp:
            return resp.status == 200
    except Exception as e:
        log_debug(f"signal_existing_instance({cmd}) failed: {e}")
        return False


def main():
    import traceback

    log_debug(f"Process started: exe={sys.executable}, argv={sys.argv}")

    # 1. Single-Instance Check
    if not acquire_single_instance():
        log_debug("Single instance check failed: already running. Signaling existing instance...")
        safe_print("[Flick] Flick is already running. Opening Dashboard...")
        signal_existing_instance("open-dashboard")
        sys.exit(0)

    # 2. Initialize Core Services
    cfg = load_config()
    http_port = cfg.get("http_port", 45455)
    udp_port = cfg.get("udp_port", 45454)

    try:
        cert, _, spki_pin = crypto_manager.get_or_create_pc_credentials(cfg.get("pc_id", ""))
        log_debug(f"Credentials initialized: SPKI PIN={spki_pin}")
    except Exception as e:
        log_debug(f"Failed to initialize crypto: {e}")

    try:
        http_server = start_http_receiver(port=http_port)
        log_debug("HTTPS mTLS 1.3 server started")
    except Exception as e:
        log_debug(f"HTTP Server failed: {e}")

    try:
        beacon = start_discovery_beacon(port=udp_port)
        log_debug("UDP Discovery Beacon started")
    except Exception as e:
        log_debug(f"UDP Beacon failed: {e}")

    # 3. Setup Tray Icon (Detached thread)
    tray_app = FlickTrayApp()
    tray_app._is_ready = True
    set_notification_callback(tray_app.notify)

    try:
        tray_app.icon.run_detached()
        log_debug("Tray icon started in detached background thread")
    except Exception as e:
        log_debug(f"Tray icon detached run failed: {e}")

    # 4. Launch Desktop Dashboard on Main Thread
    start_minimized = ("--minimized" in sys.argv or "-m" in sys.argv)
    log_debug(f"Creating dashboard window (minimized={start_minimized})...")
    
    root = desktop_dashboard.create_dashboard_window(
        on_quit=tray_app.shutdown,
        start_minimized=start_minimized
    )

    lan_ip = get_lan_ip()
    safe_print("=" * 60)
    safe_print("[Flick] Windows Multi-Format Clipboard Receiver (v2.0 mTLS 1.3)")
    safe_print(f"Host: {cfg.get('pc_name')} | LAN IP: {lan_ip}")
    safe_print(f"HTTPS Port: {http_port} | UDP Beacon: {udp_port}")
    safe_print(f"Save Directory: {cfg.get('save_directory')}")
    safe_print("=" * 60)

    try:
        root.mainloop()
    except Exception as e:
        log_debug(f"Mainloop exception: {e}")
    finally:
        tray_app.shutdown()


if __name__ == "__main__":
    main()
