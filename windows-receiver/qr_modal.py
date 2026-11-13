"""
Flick Windows Receiver - QR Code Pairing Window
Teenage Engineering (TE) Optical Reticle // FLICK RX-1 Pairing Modal
Machined obsidian chassis, International Safety Orange (#FF4400),
Work Sans & Space Mono typography, hardware screw accents, and 3D tactile controls.
"""

import os
import secrets
import threading
import tkinter as tk
from typing import Optional
from PIL import Image, ImageTk
import qrcode

import crypto_manager
from config_manager import get_config, get_lan_ip, get_resource_path
import http_receiver
from win32_theme import (
    enforce_solid_dark_theme,
    get_dpi_scale,
    get_mono,
    get_sans,
    get_screw_tk,
    load_embedded_fonts,
)

_qr_window_open = False
_current_root: Optional[tk.Tk] = None

# Hardware Color Tokens
COLOR_BG = "#0B0C0E"
COLOR_CARD_BG = "#131518"
COLOR_CARD_BORDER = "#25282E"
COLOR_INNER_BG = "#0D0E10"
COLOR_ORANGE = "#FF4400"
COLOR_ORANGE_LIP = "#882200"
COLOR_ORANGE_LIGHT = "#FFAA77"
COLOR_TEXT_WHITE = "#FFFFFF"
COLOR_TEXT_MUTED = "#8E95A2"
COLOR_TEXT_DIM = "#565C68"


def show_qr_pairing_modal():
    """Opens the Tkinter QR code pairing modal in a dedicated thread with ephemeral v2 mTLS pairing schema."""
    global _qr_window_open, _current_root
    if _qr_window_open and _current_root:
        try:
            _current_root.deiconify()
            _current_root.lift()
            _current_root.attributes("-topmost", True)
            _current_root.after_idle(lambda: _current_root.attributes("-topmost", False))
            _current_root.focus_force()
        except Exception:
            pass
        return

    def _run_gui():
        global _qr_window_open, _current_root
        _qr_window_open = True
        try:
            load_embedded_fonts()

            cfg = get_config()
            lan_ip = get_lan_ip()
            pc_id = cfg.get("pc_id", "")
            port = cfg.get("http_port", 45455)
            pc_name = cfg.get("pc_name", "WINDOWS-PC").upper()

            # 1. Fetch PC certificate and compute 64-character SPKI SHA-256 fingerprint
            cert, _, spki_pin = crypto_manager.get_or_create_pc_credentials(pc_id)

            # 2. Generate 128-bit ephemeral pairing secret (32 hex characters, valid 5 minutes)
            pair_secret = secrets.token_hex(16)
            http_receiver.set_active_pair_secret(pair_secret, duration_sec=300.0)

            # 3. Construct Zero-Trust v2 URI schema
            qr_text = f"flick://pair?v=2&ip={lan_ip}&p={port}&pin={spki_pin}&s={pair_secret}&id={pc_id}"

            # Setup Tkinter Root
            root = tk.Tk()
            _current_root = root
            root.title("FLICK // OPTICAL PAIRING RETICLE")
            root.resizable(False, False)
            root.configure(bg=COLOR_BG)

            # Window icon
            ico_path = get_resource_path("flick.ico")
            if os.path.exists(ico_path):
                try:
                    root.iconbitmap(ico_path)
                except Exception:
                    pass

            scale = get_dpi_scale(root)

            # QR Code Box
            box_size = 5 if scale >= 1.2 else 4
            qr = qrcode.QRCode(
                version=1,
                error_correction=qrcode.constants.ERROR_CORRECT_M,
                box_size=box_size,
                border=2,
            )
            qr.add_data(qr_text)
            qr.make(fit=True)
            pil_img = qr.make_image(fill_color="#000000", back_color="#FFFFFF").convert("RGB")

            w = int(390 * scale)
            h = int(580 * scale)
            sw = root.winfo_screenwidth()
            sh = root.winfo_screenheight()
            x = (sw - w) // 2
            y = (sh - h) // 2
            root.geometry(f"{w}x{h}+{x}+{y}")

            enforce_solid_dark_theme(root, strip_caption_buttons=True)

            pad_x = int(20 * scale)

            # Header Frame with Screws
            header_frame = tk.Frame(root, bg=COLOR_BG, padx=pad_x, pady=int(14 * scale))
            header_frame.pack(fill="x")

            screw_row = tk.Frame(header_frame, bg=COLOR_BG)
            screw_row.pack(fill="x")

            tk.Label(screw_row, image=get_screw_tk(12, 40), bg=COLOR_BG).pack(side="left")
            tk.Label(
                screw_row,
                text="FLICK LABS // OPTICAL RETICLE",
                font=get_mono(8, "bold"),
                fg=COLOR_ORANGE,
                bg=COLOR_BG
            ).pack(side="left", padx=6)
            tk.Label(screw_row, image=get_screw_tk(12, 130), bg=COLOR_BG).pack(side="right")

            tk.Label(
                header_frame,
                text="PAIR WITH FLICK",
                font=get_sans(15, "bold"),
                fg=COLOR_TEXT_WHITE,
                bg=COLOR_BG
            ).pack(anchor="w", pady=(4, 2))

            tk.Label(
                header_frame,
                text="Scan optical matrix below using Flick on Android:",
                font=get_sans(9),
                fg=COLOR_TEXT_MUTED,
                bg=COLOR_BG
            ).pack(anchor="w")

            # Solid White Card Container for QR Code
            qr_card = tk.Frame(root, bg="#FFFFFF", padx=int(10 * scale), pady=int(10 * scale), relief="flat")
            qr_card.pack(padx=pad_x, pady=int(4 * scale))

            tk_img = ImageTk.PhotoImage(pil_img)
            qr_lbl = tk.Label(qr_card, image=tk_img, bg="#FFFFFF", borderwidth=0)
            qr_lbl.image = tk_img
            qr_lbl.pack()

            # Technical Telemetry Frame
            pin_short = f"{spki_pin[:8]}...{spki_pin[-6:]}"
            info_frame = tk.Frame(
                root,
                bg=COLOR_INNER_BG,
                highlightbackground="#22252B",
                highlightthickness=1,
                padx=int(14 * scale),
                pady=int(10 * scale)
            )
            info_frame.pack(fill="x", padx=pad_x, pady=int(10 * scale))

            tk.Label(
                info_frame,
                text=f"NODE: {pc_name}  •  {lan_ip}:{port}",
                font=get_mono(9, "bold"),
                fg=COLOR_ORANGE_LIGHT,
                bg=COLOR_INNER_BG
            ).pack(anchor="w")

            tk.Label(
                info_frame,
                text=f"SPKI PIN: {pin_short}  (TTL: 5m)",
                font=get_mono(8),
                fg=COLOR_TEXT_MUTED,
                bg=COLOR_INNER_BG
            ).pack(anchor="w", pady=(3, 0))

            # Tactile Close Button
            btn_frame = tk.Frame(root, bg=COLOR_BG)
            btn_frame.pack(pady=int(10 * scale))

            btn_outer = tk.Frame(btn_frame, bg=COLOR_ORANGE_LIP, bd=0)
            btn_outer.pack()

            btn_cap = tk.Label(
                btn_outer,
                text="DONE // CLOSE RETICLE",
                font=get_sans(10, "bold"),
                bg=COLOR_ORANGE,
                fg=COLOR_TEXT_WHITE,
                padx=int(28 * scale),
                pady=int(8 * scale),
                cursor="hand2"
            )
            btn_cap.pack(pady=(0, 4))

            def _on_btn_press(e):
                btn_cap.pack_configure(pady=(2, 2))

            def _on_btn_release(e):
                btn_cap.pack_configure(pady=(0, 4))
                root.destroy()

            btn_cap.bind("<ButtonPress-1>", _on_btn_press)
            btn_cap.bind("<ButtonRelease-1>", _on_btn_release)

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
    time.sleep(30)
