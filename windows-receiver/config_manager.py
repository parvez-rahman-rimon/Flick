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


def add_paired_device(
    client_id: str,
    client_name: str,
    client_spki_pin: str = "",
    client_cert_pin: str = "",
    pair_secret: str = ""
) -> None:
    """Registers or updates a paired Android client with hardware public key pinning and pair secret."""
    import time
    cfg = get_config()
    devices: List[Dict[str, Any]] = cfg.get("paired_devices", [])
    
    # Update if already exists, else append
    for dev in devices:
        if dev.get("client_id") == client_id or (client_spki_pin and dev.get("client_spki_pin") == client_spki_pin):
            dev["client_id"] = client_id
            dev["client_name"] = client_name
            if client_spki_pin:
                dev["client_spki_pin"] = client_spki_pin.lower()
            if client_cert_pin:
                dev["client_cert_pin"] = client_cert_pin.lower()
            if pair_secret:
                dev["pair_secret"] = pair_secret
            dev["paired_at"] = int(time.time())
            save_config(cfg)
            return

    new_device: Dict[str, Any] = {
        "client_id": client_id,
        "client_name": client_name,
        "client_spki_pin": client_spki_pin.lower() if client_spki_pin else "",
        "client_cert_pin": client_cert_pin.lower() if client_cert_pin else "",
        "pair_secret": pair_secret,
        "paired_at": int(time.time())
    }
    devices.append(new_device)
    cfg["paired_devices"] = devices
    save_config(cfg)


def is_device_paired(client_id: str) -> bool:
    """Checks whether a client_id has been paired previously."""
    cfg = get_config()
    for dev in cfg.get("paired_devices", []):
        if dev.get("client_id") == client_id:
            return True
    return False


def is_cert_paired(spki_pin: str, cert_pin: str = "") -> bool:
    """Checks whether an incoming client certificate's SPKI or Cert SHA-256 is in paired devices."""
    if not spki_pin and not cert_pin:
        return False
    cfg = get_config()
    spki_lower = spki_pin.lower() if spki_pin else ""
    cert_lower = cert_pin.lower() if cert_pin else ""
    for dev in cfg.get("paired_devices", []):
        dev_spki = dev.get("client_spki_pin", "").lower()
        dev_cert = dev.get("client_cert_pin", "").lower()
        if spki_lower and dev_spki and dev_spki == spki_lower:
            return True
        if cert_lower and dev_cert and dev_cert == cert_lower:
            return True
    return False


def get_paired_device(client_id: str = "", spki_pin: str = "", cert_pin: str = "") -> Optional[Dict[str, Any]]:
    """Finds a paired device record matching client_id, spki_pin, or cert_pin."""
    cfg = get_config()
    spki_lower = spki_pin.lower() if spki_pin else ""
    cert_lower = cert_pin.lower() if cert_pin else ""
    for dev in cfg.get("paired_devices", []):
        if client_id and dev.get("client_id") == client_id:
            return dict(dev)
        if spki_lower and dev.get("client_spki_pin", "").lower() == spki_lower:
            return dict(dev)
        if cert_lower and dev.get("client_cert_pin", "").lower() == cert_lower:
            return dict(dev)
    return None


def remove_paired_device(client_id: str) -> None:
    """Removes a paired device by client_id or SPKI pin from configuration."""
    cfg = get_config()
    devices = [
        d for d in cfg.get("paired_devices", [])
        if d.get("client_id") != client_id and d.get("client_spki_pin") != client_id
    ]
    cfg["paired_devices"] = devices
    save_config(cfg)


def set_save_directory(new_dir: str) -> None:
    """Updates the downloads destination folder in configuration."""
    cfg = get_config()
    cfg["save_directory"] = str(new_dir)
    save_config(cfg)


TRANSFERS_FILE = CONFIG_DIR / "pc_recent_transfers.json"
_transfers_lock = threading.Lock()


def record_pc_transfer(file_name: str, file_size: int, client_ip: str, mode: str = "FILE") -> None:
    """Records an incoming file or clipboard transfer on PC."""
    import time
    with _transfers_lock:
        transfers = get_recent_pc_transfers()
        rec = {
            "id": str(uuid.uuid4()),
            "file_name": file_name,
            "file_size": file_size,
            "client_ip": client_ip,
            "mode": mode,
            "timestamp": int(time.time())
        }
        transfers.insert(0, rec)
        transfers = transfers[:30]  # keep last 30
        try:
            with open(TRANSFERS_FILE, "w", encoding="utf-8") as f:
                json.dump(transfers, f, indent=2)
        except Exception:
            pass


def get_recent_pc_transfers() -> List[Dict[str, Any]]:
    """Returns the list of recently received files or clipboard transfers on PC."""
    if not TRANSFERS_FILE.exists():
        return []
    try:
        with open(TRANSFERS_FILE, "r", encoding="utf-8") as f:
            data = json.load(f)
            if isinstance(data, list):
                return data
    except Exception:
        pass
    return []


def clear_recent_pc_transfers() -> None:
    """Clears all transfer records on PC."""
    with _transfers_lock:
        try:
            with open(TRANSFERS_FILE, "w", encoding="utf-8") as f:
                json.dump([], f)
        except Exception:
            pass

