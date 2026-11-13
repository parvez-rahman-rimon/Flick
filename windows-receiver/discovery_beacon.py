"""
Flick Windows Receiver - UDP Discovery Beacon
Broadcasts FLICK_DISCOVERY_V2 JSON announcement every 2.0s over UDP port 45454.
Carries rolling HMAC-SHA256 authenticator tags (AuthTag = HMAC(pair_secret, floor(time / 30) + pc_id))
defeating rogue LAN beacon spoofing and packet replay.
"""

import hashlib
import hmac
import json
import socket
import sys
import threading
import time
from typing import Dict, Optional

def safe_print(*args, **kwargs):
    if sys.stdout is not None:
        try:
            print(*args, **kwargs)
        except Exception:
            pass

from config_manager import get_config, get_lan_ip, get_subnet_broadcast

DISCOVERY_PORT = 45454
BEACON_INTERVAL_SEC = 2.0
PROTOCOL_VERSION = "FLICK_DISCOVERY_V2"
RECEIVER_VERSION = "2.0.0"


def compute_rolling_auth_tag(pair_secret: str, pc_id: str, epoch_time: Optional[int] = None) -> str:
    """Computes HMAC-SHA256(pair_secret, floor(epoch / 30) + pc_id)."""
    t = int(epoch_time if epoch_time is not None else time.time())
    time_step = t // 30
    msg = f"{time_step}{pc_id}".encode("utf-8")
    return hmac.new(pair_secret.encode("utf-8"), msg, hashlib.sha256).hexdigest().lower()


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
                pc_id = cfg.get("pc_id", "")
                epoch_now = int(time.time())

                # Compute rolling HMAC authenticator tags for each paired client
                auth_tags: Dict[str, str] = {}
                primary_tag = ""
                for dev in cfg.get("paired_devices", []):
                    cid = dev.get("client_id")
                    p_sec = dev.get("pair_secret")
                    if cid and p_sec:
                        tag = compute_rolling_auth_tag(p_sec, pc_id, epoch_now)
                        auth_tags[cid] = tag
                        if not primary_tag:
                            primary_tag = tag

                payload = {
                    "protocol": PROTOCOL_VERSION,
                    "pc_id": pc_id,
                    "pc_name": cfg.get("pc_name", socket.gethostname()),
                    "ip": lan_ip,
                    "http_port": cfg.get("http_port", 45455),
                    "https_port": cfg.get("http_port", 45455),
                    "version": RECEIVER_VERSION,
                    "timestamp": epoch_now,
                    "auth_tag": primary_tag,
                    "auth_tags": auth_tags,
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
