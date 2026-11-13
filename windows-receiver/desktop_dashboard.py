"""
Flick Windows Receiver - Desktop Dashboard
Teenage Engineering (TE) Hardware Redesign // FLICK RX-1 Field Transceiver
Machined obsidian chassis, International Safety Orange (#FF4400), CRT telemetry,
Work Sans Swiss Grotesk & Space Mono typography, TP-7 motorized spool, and tactile controls.
"""

import ctypes
import math
import os
import secrets
import sys
import threading
import time
import tkinter as tk
from tkinter import filedialog, ttk
from typing import Any, Callable, Dict, List, Optional, Tuple
import winreg

from PIL import Image, ImageTk
import qrcode

import config_manager
from config_manager import (
    clear_recent_pc_transfers,
    get_config,
    get_lan_ip,
    get_recent_pc_transfers,
    get_resource_path,
    remove_paired_device,
    save_config,
    set_clipboard_mode,
    set_save_directory,
)
import crypto_manager
import http_receiver
import qr_modal
from win32_theme import (
    enforce_solid_dark_theme,
    get_dpi_scale,
    get_mono,
    get_pixel,
    get_sans,
    get_screw_tk,
    get_toplevel_hwnd,
    load_embedded_fonts,
)

_dashboard_open = False
_current_dashboard_root: Optional[tk.Tk] = None
_on_quit_handler: Optional[Callable[[], None]] = None

RUN_REG_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"

# -----------------------------------------------------------------------------
# TEENAGE ENGINEERING (TE) HARDWARE PALETTE (FLICK RX-1 FIELD COMPANION)
# -----------------------------------------------------------------------------
COLOR_BG = "#0B0C0E"              # Obsidian matte chassis
COLOR_CHASSIS_BORDER = "#1E2024"  # Machined outer seam
COLOR_CARD_BG = "#131518"         # Machined surface plate
COLOR_CARD_BORDER = "#25282E"     # Plate bevel outline
COLOR_CARD_INNER = "#0D0E10"      # Recessed CRT telemetry & well
COLOR_CARD_INNER_BORDER = "#20232A"

COLOR_ORANGE = "#FF4400"          # International Safety Orange (Brand Core)
COLOR_ORANGE_BRIGHT = "#FF5500"   # Active keycap upper gradient
COLOR_ORANGE_DARK = "#E63D00"     # Active keycap lower gradient
COLOR_ORANGE_LIP = "#882200"      # 3D mechanical button bottom shadow lip
COLOR_ORANGE_HOVER = "#FF5511"    # Hover glow
COLOR_ORANGE_LIGHT = "#FFAA77"    # Muted readable amber/orange
COLOR_ORANGE_BADGE_BG = "#331206" # Recessed amber container

COLOR_KEYCAP_BG = "#22252B"       # Unselected dark slate keycap
COLOR_KEYCAP_LIP = "#111316"      # Keycap bottom shadow lip
COLOR_KEYCAP_BORDER = "#353942"   # Keycap edge highlight
COLOR_KEYCAP_HOVER = "#2A2E36"

COLOR_TEXT_WHITE = "#FFFFFF"      # Crisp white for headers & keycaps
COLOR_TEXT_MAIN = "#EDEDED"       # Clean off-white
COLOR_TEXT_MUTED = "#8E95A2"      # Monospace technical telemetry
COLOR_TEXT_DIM = "#565C68"        # Metadata / timestamps / channel tags

COLOR_DANGER = "#FF4545"          # Red alert for destructive actions
COLOR_DANGER_LIP = "#551111"      # Red button shadow lip
COLOR_DANGER_BG = "#261111"       # Recessed red background


def is_startup_enabled() -> bool:
    """Checks if Flick is configured to launch on Windows user login."""
    try:
        key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_REG_KEY, 0, winreg.KEY_READ)
        val, _ = winreg.QueryValueEx(key, "Flick")
        winreg.CloseKey(key)
        return bool(val)
    except Exception:
        return False


def set_startup_enabled(enabled: bool) -> None:
    """Adds or removes Flick from Windows user login startup registry."""
    try:
        key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_REG_KEY, 0, winreg.KEY_SET_VALUE)
        if enabled:
            if getattr(sys, "frozen", False):
                exe_path = sys.executable
                winreg.SetValueEx(key, "Flick", 0, winreg.REG_SZ, f'"{exe_path}" --minimized')
            else:
                pyw = r"C:\Users\Parvez\AppData\Local\Python\pythoncore-3.14-64\pythonw.exe"
                script = os.path.abspath(sys.argv[0])
                winreg.SetValueEx(key, "Flick", 0, winreg.REG_SZ, f'"{pyw}" "{script}" --minimized')
        else:
            try:
                winreg.DeleteValue(key, "Flick")
            except FileNotFoundError:
                pass
        winreg.CloseKey(key)
    except Exception as e:
        print(f"[Dashboard] Failed to update startup registry: {e}")


def show_desktop_dashboard() -> None:
    """Focuses or un-hides the existing Flick Desktop Dashboard."""
    global _current_dashboard_root
    if _current_dashboard_root:
        try:
            def _restore():
                try:
                    _current_dashboard_root.deiconify()
                    _current_dashboard_root.lift()
                    _current_dashboard_root.attributes("-topmost", True)
                    _current_dashboard_root.after_idle(lambda: _current_dashboard_root.attributes("-topmost", False))
                    _current_dashboard_root.focus_force()
                    try:
                        top_hwnd = get_toplevel_hwnd(_current_dashboard_root)
                        if top_hwnd:
                            user32 = ctypes.windll.user32
                            user32.ShowWindow(top_hwnd, 9)  # SW_RESTORE
                            user32.SetForegroundWindow(top_hwnd)
                            user32.SwitchToThisWindow(top_hwnd, True)
                    except Exception:
                        pass
                except Exception:
                    pass
            _current_dashboard_root.after(0, _restore)
            return
        except Exception:
            pass

    t = threading.Thread(target=_create_and_run_window, daemon=True, name="FlickDashboardThread")
    t.start()


def create_dashboard_window(on_quit: Optional[Callable[[], None]] = None, start_minimized: bool = False) -> tk.Tk:
    """Creates the Dashboard Tkinter instance for main thread event loops."""
    global _on_quit_handler
    _on_quit_handler = on_quit
    return _build_dashboard_ui(start_minimized=start_minimized)


def _create_and_run_window():
    root = _build_dashboard_ui(start_minimized=False)
    root.mainloop()


# -----------------------------------------------------------------------------
# TACTILE HARDWARE WIDGETS (3D Buttons, Tape Spool, VU-Meter, Perforated Grill)
# -----------------------------------------------------------------------------
class TactileKeycap(tk.Frame):
    """
    3D Mechanical Push-Button Keycap inspired by Teenage Engineering OP-1 / EP-133.
    Features top bevel highlight, tactile bottom shadow lip, and physical 3px travel on click.
    """
    def __init__(
        self,
        parent: tk.Widget,
        title: str,
        subtitle: str = "",
        f_tag: str = "",
        command: Optional[Callable[[], None]] = None,
        is_orange: bool = True,
        height_px: int = 56,
        **kwargs
    ):
        super().__init__(parent, bg=parent["bg"], **kwargs)
        self.command = command
        self.is_orange = is_orange
        self.lip_height = 4

        self.btn_bg = COLOR_ORANGE if is_orange else COLOR_KEYCAP_BG
        self.hover_bg = COLOR_ORANGE_HOVER if is_orange else COLOR_KEYCAP_HOVER
        self.lip_bg = COLOR_ORANGE_LIP if is_orange else COLOR_KEYCAP_LIP
        self.fg_color = COLOR_TEXT_WHITE if is_orange else COLOR_TEXT_WHITE

        # Container
        self.outer = tk.Frame(self, bg=self.lip_bg, bd=0)
        self.outer.pack(fill="both", expand=True)

        self.cap = tk.Frame(self.outer, bg=self.btn_bg, padx=12, pady=8)
        self.cap.pack(fill="both", expand=True, padx=0, pady=(0, self.lip_height))

        # Top row: F-key tag and symbol
        if f_tag:
            tag_box = tk.Frame(self.cap, bg=COLOR_ORANGE_BADGE_BG if is_orange else "#14161A", padx=4, pady=1)
            tag_box.pack(anchor="w", pady=(0, 2))
            tk.Label(
                tag_box,
                text=f_tag,
                font=get_mono(8, "bold"),
                fg="#FFFFFF" if is_orange else COLOR_ORANGE,
                bg=tag_box["bg"]
            ).pack()

        # Title
        self.lbl_title = tk.Label(
            self.cap,
            text=title,
            font=get_sans(10, "bold"),
            fg=self.fg_color,
            bg=self.btn_bg,
            anchor="w"
        )
        self.lbl_title.pack(fill="x")

        # Subtitle
        if subtitle:
            self.lbl_sub = tk.Label(
                self.cap,
                text=subtitle,
                font=get_mono(7),
                fg=COLOR_TEXT_WHITE if is_orange else COLOR_TEXT_MUTED,
                bg=self.btn_bg,
                anchor="w"
            )
            self.lbl_sub.pack(fill="x")

        # Bind tactile physical click
        for w in (self.cap, self.lbl_title) + ((self.lbl_sub,) if subtitle else ()):
            w.bind("<ButtonPress-1>", self._on_press)
            w.bind("<ButtonRelease-1>", self._on_release)
            w.bind("<Enter>", self._on_enter)
            w.bind("<Leave>", self._on_leave)
            w.configure(cursor="hand2")

    def _on_enter(self, event):
        self.cap.configure(bg=self.hover_bg)
        self.lbl_title.configure(bg=self.hover_bg)
        if hasattr(self, "lbl_sub"):
            self.lbl_sub.configure(bg=self.hover_bg)

    def _on_leave(self, event):
        self.cap.configure(bg=self.btn_bg)
        self.lbl_title.configure(bg=self.btn_bg)
        if hasattr(self, "lbl_sub"):
            self.lbl_sub.configure(bg=self.btn_bg)

    def _on_press(self, event):
        self.cap.pack_configure(pady=(2, self.lip_height - 2))

    def _on_release(self, event):
        self.cap.pack_configure(pady=(0, self.lip_height))
        if self.command:
            self.command()


class MotorizedTapeSpool(tk.Canvas):
    """
    TP-7 Style Motorized Virtual Tape Spool Canvas.
    Renders rotating 3-spoke reel with International Safety Orange hub and animated flight spokes.
    """
    def __init__(self, parent: tk.Widget, size: int = 46, bg: str = COLOR_CARD_INNER):
        super().__init__(parent, width=size, height=size, bg=bg, highlightthickness=0)
        self.size = size
        self.angle = 0.0
        self.is_active = True
        self.draw_reel()
        self._animate()

    def draw_reel(self):
        self.delete("all")
        s = self.size
        cx, cy = s / 2.0, s / 2.0
        r = s / 2.0 - 2

        # Outer chassis circle
        self.create_oval(cx - r, cy - r, cx + r, cy + r, fill="#1A1C1F", outline="#2F333C", width=1)

        # 3 Spoke lines
        for deg in [0, 60, 120]:
            rad = math.radians(self.angle + deg)
            x1 = cx + math.cos(rad) * (r * 0.85)
            y1 = cy + math.sin(rad) * (r * 0.85)
            x2 = cx - math.cos(rad) * (r * 0.85)
            y2 = cy - math.sin(rad) * (r * 0.85)
            self.create_line(x1, y1, x2, y2, fill="#444955", width=2)

        # Center Orange Motor Hub
        hr = r * 0.36
        self.create_oval(cx - hr, cy - hr, cx + hr, cy + hr, fill=COLOR_ORANGE, outline="#FFAA77", width=1)
        # Center spindle hole
        self.create_oval(cx - 2, cy - 2, cx + 2, cy + 2, fill="#0D0E10", outline="")

    def _animate(self):
        if not self.winfo_exists():
            return
        if self.is_active:
            self.angle = (self.angle + 8) % 360
            self.draw_reel()
        self.after(60, self._animate)


def _build_dashboard_ui(start_minimized: bool = False) -> tk.Tk:
    global _dashboard_open, _current_dashboard_root
    _dashboard_open = True

    # Register Work Sans & Space Mono TTF fonts
    load_embedded_fonts()

    root = tk.Tk()
    _current_dashboard_root = root
    root.title("FLICK // RX-1 FIELD RECEIVER")
    root.configure(bg=COLOR_BG)

    # Detect native display DPI scale
    scale = get_dpi_scale(root)
    w, h = int(600 * scale), int(840 * scale)
    min_w, min_h = int(520 * scale), int(680 * scale)
    root.minsize(min_w, min_h)

    # Window Icon
    ico_path = get_resource_path("flick.ico")
    if os.path.exists(ico_path):
        try:
            root.iconbitmap(ico_path)
        except Exception:
            pass

    # Center Window on Screen
    try:
        sw = root.winfo_screenwidth()
        sh = root.winfo_screenheight()
        x = (sw - w) // 2
        y = (sh - h) // 2
        root.geometry(f"{w}x{h}+{x}+{y}")
    except Exception:
        root.geometry(f"{w}x{h}")

    # Enforce solid dark theme neutralizing Windhawk translucency hooks
    enforce_solid_dark_theme(root, strip_caption_buttons=False)

    # Outer Scrollable Container
    canvas = tk.Canvas(root, bg=COLOR_BG, highlightthickness=0)
    scrollbar = tk.Scrollbar(root, orient="vertical", command=canvas.yview)
    scroll_frame = tk.Frame(canvas, bg=COLOR_BG)

    scroll_frame.bind(
        "<Configure>",
        lambda e: canvas.configure(scrollregion=canvas.bbox("all"))
    )
    canvas_window = canvas.create_window((0, 0), window=scroll_frame, anchor="nw")

    def _on_canvas_configure(event):
        canvas.itemconfig(canvas_window, width=event.width)

    canvas.bind("<Configure>", _on_canvas_configure)
    canvas.configure(yscrollcommand=scrollbar.set)

    canvas.pack(side="left", fill="both", expand=True)
    scrollbar.pack(side="right", fill="y")

    # Smooth mousewheel scroll support
    def _on_mousewheel(event):
        canvas.yview_scroll(int(-1 * (event.delta / 120)), "units")
    canvas.bind_all("<MouseWheel>", _on_mousewheel)

    # -------------------------------------------------------------
    # 1. TOP HARDWARE STUDIO HEADER (Teenage Engineering Hardware Division)
    # -------------------------------------------------------------
    header = tk.Frame(scroll_frame, bg=COLOR_BG, padx=24, pady=18)
    header.pack(fill="x")

    # Hardware Reference Sub-bar
    meta_row = tk.Frame(header, bg=COLOR_BG)
    meta_row.pack(fill="x", pady=(0, 4))

    # Glowing orange hardware LED dot
    led_canvas = tk.Canvas(meta_row, width=12, height=12, bg=COLOR_BG, highlightthickness=0)
    led_canvas.pack(side="left", padx=(0, 6))
    led_canvas.create_oval(2, 2, 10, 10, fill=COLOR_ORANGE, outline="#FFAA77", width=1)

    tk.Label(
        meta_row,
        text="FLICK LABS // HARDWARE DIVISION",
        font=get_mono(8, "bold"),
        fg=COLOR_ORANGE,
        bg=COLOR_BG
    ).pack(side="left")

    tk.Label(
        meta_row,
        text=" • REF: TE-RX1-2026",
        font=get_mono(8),
        fg=COLOR_TEXT_DIM,
        bg=COLOR_BG
    ).pack(side="left")

    # Perforated speaker grill (6 recessed dots in a 2x3 matrix)
    grill = tk.Frame(meta_row, bg=COLOR_BG)
    grill.pack(side="right")
    for r in range(2):
        row_f = tk.Frame(grill, bg=COLOR_BG)
        row_f.pack()
        for c in range(4):
            dot = tk.Frame(row_f, width=3, height=3, bg="#1E2128")
            dot.pack(side="left", padx=1, pady=1)

    # Main Headline
    title_row = tk.Frame(header, bg=COLOR_BG)
    title_row.pack(fill="x", pady=(2, 2))

    tk.Label(
        title_row,
        text="FLICK // ",
        font=get_sans(20, "bold"),
        fg=COLOR_TEXT_WHITE,
        bg=COLOR_BG
    ).pack(side="left")

    tk.Label(
        title_row,
        text="RX-1 FIELD RECEIVER",
        font=get_sans(20, "bold"),
        fg=COLOR_ORANGE,
        bg=COLOR_BG
    ).pack(side="left")

    # Muted Technical Subtitle
    tk.Label(
        header,
        text="TACTILE DIRECT PEER DATA TRANSCEIVER  •  ZERO-TRUST mTLS 1.3 PINNED",
        font=get_mono(8),
        fg=COLOR_TEXT_MUTED,
        bg=COLOR_BG
    ).pack(anchor="w", pady=(0, 6))

    # -------------------------------------------------------------
    # 2. MACHINED CRT / LCD TELEMETRY SCREEN (Recessed Telemetry Panel)
    # -------------------------------------------------------------
    crt_card = tk.Frame(
        scroll_frame,
        bg=COLOR_CARD_INNER,
        highlightbackground=COLOR_CARD_INNER_BORDER,
        highlightthickness=1,
        padx=18,
        pady=16
    )
    crt_card.pack(fill="x", padx=24, pady=8)

    crt_top_row = tk.Frame(crt_card, bg=COLOR_CARD_INNER)
    crt_top_row.pack(fill="x")

    # Left Telemetry: Status tag + Machine Name + IP
    crt_left = tk.Frame(crt_top_row, bg=COLOR_CARD_INNER)
    crt_left.pack(side="left", fill="both", expand=True)

    # Live connection indicator
    status_indicator_row = tk.Frame(crt_left, bg=COLOR_CARD_INNER)
    status_indicator_row.pack(anchor="w")

    status_dot = tk.Canvas(status_indicator_row, width=10, height=10, bg=COLOR_CARD_INNER, highlightthickness=0)
    status_dot.pack(side="left", padx=(0, 5))
    status_dot.create_oval(1, 1, 9, 9, fill=COLOR_ORANGE, outline="")

    status_tag_lbl = tk.Label(
        status_indicator_row,
        text="ARMED // 45455-TCP // mTLS 1.3",
        font=get_mono(8, "bold"),
        fg=COLOR_ORANGE,
        bg=COLOR_CARD_INNER
    )
    status_tag_lbl.pack(side="left")

    # Big Bold Hostname (Clean Swiss Grotesk)
    cfg = get_config()
    lan_ip = get_lan_ip()
    port = cfg.get("http_port", 45455)
    pc_name = cfg.get("pc_name", "WINDOWS-PC").upper()

    tk.Label(
        crt_left,
        text=pc_name,
        font=get_sans(18, "bold"),
        fg=COLOR_TEXT_WHITE,
        bg=COLOR_CARD_INNER,
        anchor="w"
    ).pack(anchor="w", pady=(2, 1))

    # Monospace IP & Channel
    tk.Label(
        crt_left,
        text=f"IP: {lan_ip} : {port}  •  DIRECT LAN",
        font=get_mono(9),
        fg=COLOR_TEXT_MUTED,
        bg=COLOR_CARD_INNER,
        anchor="w"
    ).pack(anchor="w")

    # Right: Motorized Virtual Spool (TP-7 Style)
    spool_container = tk.Frame(crt_top_row, bg=COLOR_CARD_INNER)
    spool_container.pack(side="right", padx=(10, 0))

    tape_spool = MotorizedTapeSpool(spool_container, size=48, bg=COLOR_CARD_INNER)
    tape_spool.pack()

    spool_speed_lbl = tk.Label(
        spool_container,
        text="0.0 MB/S",
        font=get_pixel(8),
        fg=COLOR_TEXT_MUTED,
        bg=COLOR_CARD_INNER
    )
    spool_speed_lbl.pack(pady=(2, 0))

    # Divider Line
    tk.Frame(crt_card, bg="#1C1F25", height=1).pack(fill="x", pady=12)

    # Lower Telemetry Bar: Speed Meter & RTT Latency
    telem_bar = tk.Frame(crt_card, bg=COLOR_CARD_INNER)
    telem_bar.pack(fill="x")

    # Multi-segment LED Throughput Meter
    speed_meter_box = tk.Frame(telem_bar, bg=COLOR_CARD_INNER)
    speed_meter_box.pack(side="left")

    tk.Label(
        speed_meter_box,
        text="SPEED:",
        font=get_mono(8, "bold"),
        fg=COLOR_TEXT_MUTED,
        bg=COLOR_CARD_INNER
    ).pack(side="left", padx=(0, 6))

    # Discrete LED bars (3 orange, 2 amber, 1 yellow)
    led_colors = [COLOR_ORANGE, COLOR_ORANGE, COLOR_ORANGE, "#FF8800", "#FFE600", "#FFFFFF"]
    for i, col in enumerate(led_colors):
        bar = tk.Frame(speed_meter_box, width=5, height=13, bg=col if i < 4 else "#1C1F25")
        bar.pack(side="left", padx=1)

    # RTT Latency Tag on Right
    rtt_badge = tk.Frame(telem_bar, bg="#181B20", highlightbackground=COLOR_ORANGE, highlightthickness=1, padx=6, pady=2)
    rtt_badge.pack(side="right")

    tk.Label(
        rtt_badge,
        text="RTT: 006 MS",
        font=get_mono(8, "bold"),
        fg=COLOR_ORANGE,
        bg=rtt_badge["bg"]
    ).pack()

    # -------------------------------------------------------------
    # 3. PHYSICAL 3-WAY ROCKER SWITCH (Clipboard Target Injection)
    # -------------------------------------------------------------
    rocker_card = tk.Frame(
        scroll_frame,
        bg=COLOR_CARD_BG,
        highlightbackground=COLOR_CARD_BORDER,
        highlightthickness=1,
        padx=18,
        pady=14
    )
    rocker_card.pack(fill="x", padx=24, pady=8)

    rocker_header = tk.Frame(rocker_card, bg=COLOR_CARD_BG)
    rocker_header.pack(fill="x", pady=(0, 8))

    tk.Label(
        rocker_header,
        text="CLIPBOARD TARGET INJECTION",
        font=get_mono(9, "bold"),
        fg=COLOR_TEXT_WHITE,
        bg=COLOR_CARD_BG
    ).pack(side="left")

    tk.Label(
        rocker_header,
        text="CH-01",
        font=get_mono(9, "bold"),
        fg=COLOR_ORANGE,
        bg=COLOR_CARD_BG
    ).pack(side="right")

    # Recessed tray for 3 buttons
    rocker_tray = tk.Frame(rocker_card, bg=COLOR_CARD_INNER, highlightbackground=COLOR_CARD_INNER_BORDER, highlightthickness=1, padx=4, pady=4)
    rocker_tray.pack(fill="x")

    current_mode_var = tk.StringVar(value=cfg.get("clipboard_mode", "SMART_MULTI"))

    rocker_buttons: Dict[str, tk.Frame] = {}

    def _select_mode(mode_key: str):
        set_clipboard_mode(mode_key)
        current_mode_var.set(mode_key)
        _render_rocker_states()

    def _render_rocker_states():
        active = current_mode_var.get()
        for key, f in rocker_buttons.items():
            is_active = (key == active)
            btn_bg = COLOR_ORANGE if is_active else COLOR_CARD_INNER
            lip_bg = COLOR_ORANGE_LIP if is_active else COLOR_CARD_INNER
            fg = COLOR_TEXT_WHITE if is_active else COLOR_TEXT_MUTED
            f.configure(bg=lip_bg)
            for child in f.winfo_children():
                if isinstance(child, tk.Label):
                    child.configure(bg=btn_bg, fg=fg)

    modes = [
        ("SMART_MULTI", "01 MULTI"),
        ("FORCE_PATH", "02 PATH"),
        ("FORCE_IMAGE", "03 BITMAP"),
    ]

    for mode_key, mode_title in modes:
        btn_wrapper = tk.Frame(rocker_tray, bg=COLOR_ORANGE_LIP, pady=0)
        btn_wrapper.pack(side="left", fill="both", expand=True, padx=2)

        lbl = tk.Label(
            btn_wrapper,
            text=mode_title,
            font=get_mono(9, "bold"),
            bg=COLOR_ORANGE,
            fg=COLOR_TEXT_WHITE,
            pady=8,
            cursor="hand2"
        )
        lbl.pack(fill="both", expand=True, pady=(0, 3))

        def _make_handler(mk=mode_key):
            return lambda e: _select_mode(mk)

        lbl.bind("<Button-1>", _make_handler(mode_key))
        rocker_buttons[mode_key] = btn_wrapper

    _render_rocker_states()

    # -------------------------------------------------------------
    # 4. CHUNKY 3D PUSH-BUTTONS (Master Hardware Actions)
    # -------------------------------------------------------------
    action_deck = tk.Frame(scroll_frame, bg=COLOR_BG)
    action_deck.pack(fill="x", padx=24, pady=8)

    save_dir_str = cfg.get("save_directory", "")

    def _open_folder_action():
        p = save_dir_str
        if os.path.exists(p):
            os.startfile(p)
        else:
            os.makedirs(p, exist_ok=True)
            os.startfile(p)

    def _open_qr_modal_action():
        qr_modal.show_qr_pairing_modal()

    # F1 Master Orange Keycap (Open Vault)
    f1_key = TactileKeycap(
        action_deck,
        title="OPEN STORAGE FOLDER",
        subtitle="RECEIVED PAYLOADS // DISK",
        f_tag="F1 ▲",
        command=_open_folder_action,
        is_orange=True
    )
    f1_key.pack(side="left", fill="both", expand=True, padx=(0, 6))

    # F2 Slate Keycap (Launch Standalone QR Matrix)
    f2_key = TactileKeycap(
        action_deck,
        title="POPUP PAIRING RETICLE",
        subtitle="ZERO-TRUST MATRIX // RETICLE",
        f_tag="F2 ⚡",
        command=_open_qr_modal_action,
        is_orange=False
    )
    f2_key.pack(side="right", fill="both", expand=True, padx=(6, 0))

    # -------------------------------------------------------------
    # 5. EMBEDDED OPTICAL MATRIX CARD (Live QR Reticle with Screws)
    # -------------------------------------------------------------
    pair_card = tk.Frame(
        scroll_frame,
        bg=COLOR_CARD_BG,
        highlightbackground=COLOR_CARD_BORDER,
        highlightthickness=1,
        padx=18,
        pady=16
    )
    pair_card.pack(fill="x", padx=24, pady=8)

    # Screw Accents
    top_screw_row = tk.Frame(pair_card, bg=COLOR_CARD_BG)
    top_screw_row.pack(fill="x", pady=(0, 6))

    screw_l = tk.Label(top_screw_row, image=get_screw_tk(12, 35), bg=COLOR_CARD_BG)
    screw_l.pack(side="left")

    tk.Label(
        top_screw_row,
        text="OPTICAL MATRIX // ZERO-TRUST PAIRING",
        font=get_sans(11, "bold"),
        fg=COLOR_TEXT_WHITE,
        bg=COLOR_CARD_BG
    ).pack(side="left", padx=8)

    screw_r = tk.Label(top_screw_row, image=get_screw_tk(12, 110), bg=COLOR_CARD_BG)
    screw_r.pack(side="right")

    tk.Label(
        pair_card,
        text="Point Flick on Android at the optical reticle below to pair via ECDSA SPKI public key pinning:",
        font=get_sans(9),
        fg=COLOR_TEXT_MUTED,
        bg=COLOR_CARD_BG
    ).pack(anchor="w", pady=(0, 10))

    qr_box = tk.Frame(pair_card, bg=COLOR_CARD_INNER, highlightbackground=COLOR_CARD_INNER_BORDER, highlightthickness=1, padx=14, pady=14)
    qr_box.pack(fill="x")

    qr_img_label = tk.Label(qr_box, bg="#FFFFFF", relief="flat")
    qr_img_label.pack(side="left", padx=(0, 16))

    qr_info_col = tk.Frame(qr_box, bg=COLOR_CARD_INNER)
    qr_info_col.pack(side="left", fill="both", expand=True)

    tk.Label(
        qr_info_col,
        text="🟠 OPTICAL MATRIX ARMED",
        font=get_mono(9, "bold"),
        fg=COLOR_ORANGE,
        bg=COLOR_CARD_INNER
    ).pack(anchor="w")

    pin_preview_var = tk.StringVar(value="PIN: Calculating...")
    tk.Label(
        qr_info_col,
        textvariable=pin_preview_var,
        font=get_mono(8),
        fg=COLOR_TEXT_MUTED,
        bg=COLOR_CARD_INNER
    ).pack(anchor="w", pady=(3, 3))

    timer_var = tk.StringVar(value="TTL: 05M 00S")
    tk.Label(
        qr_info_col,
        textvariable=timer_var,
        font=get_mono(9, "bold"),
        fg=COLOR_ORANGE_LIGHT,
        bg=COLOR_CARD_INNER
    ).pack(anchor="w", pady=(0, 8))

    _qr_photo_ref = [None]
    _qr_expiry_ts = [0.0]

    def _refresh_qr():
        secret = secrets.token_hex(16)
        duration = 300.0
        http_receiver.set_active_pair_secret(secret, duration_sec=duration)
        _qr_expiry_ts[0] = time.time() + duration

        current_cfg = get_config()
        curr_ip = get_lan_ip()
        curr_port = current_cfg.get("http_port", 45455)
        curr_id = current_cfg.get("pc_id", "")
        cert, _, spki_pin = crypto_manager.get_or_create_pc_credentials(curr_id)

        pin_preview_var.set(f"SPKI PIN: {spki_pin[:8]}...{spki_pin[-6:]}")

        qr_payload = f"flick://pair?v=2&ip={curr_ip}&p={curr_port}&pin={spki_pin}&s={secret}&id={curr_id}"

        box_sz = 4 if scale >= 1.25 else 3
        qrobj = qrcode.QRCode(version=1, box_size=box_sz, border=2)
        qrobj.add_data(qr_payload)
        qrobj.make(fit=True)
        pil_img = qrobj.make_image(fill_color="#000000", back_color="#FFFFFF").convert("RGB")

        tk_img = ImageTk.PhotoImage(pil_img)
        _qr_photo_ref[0] = tk_img
        qr_img_label.configure(image=tk_img)

    TactileKeycap(
        qr_info_col,
        title="REFRESH CODE",
        f_tag="F3 🔄",
        command=_refresh_qr,
        is_orange=False,
        height_px=42
    ).pack(anchor="w")

    _refresh_qr()

    # -------------------------------------------------------------
    # 6. STORAGE VAULT / SAVE DIRECTORY
    # -------------------------------------------------------------
    storage_card = tk.Frame(
        scroll_frame,
        bg=COLOR_CARD_BG,
        highlightbackground=COLOR_CARD_BORDER,
        highlightthickness=1,
        padx=18,
        pady=16
    )
    storage_card.pack(fill="x", padx=24, pady=8)

    tk.Label(
        storage_card,
        text="STORAGE VAULT // SAVE LOCATION",
        font=get_sans(11, "bold"),
        fg=COLOR_TEXT_WHITE,
        bg=COLOR_CARD_BG
    ).pack(anchor="w")

    save_path_var = tk.StringVar(value=save_dir_str)

    path_lbl = tk.Label(
        storage_card,
        textvariable=save_path_var,
        font=get_mono(9),
        fg=COLOR_ORANGE_LIGHT,
        bg=COLOR_CARD_INNER,
        highlightbackground=COLOR_CARD_INNER_BORDER,
        highlightthickness=1,
        padx=10,
        pady=8,
        anchor="w",
        wraplength=480,
        justify="left"
    )
    path_lbl.pack(fill="x", pady=(8, 10))

    btn_row = tk.Frame(storage_card, bg=COLOR_CARD_BG)
    btn_row.pack(anchor="w")

    def _change_folder():
        new_dir = filedialog.askdirectory(initialdir=save_path_var.get(), title="Select Flick Storage Folder")
        if new_dir:
            set_save_directory(new_dir)
            save_path_var.set(new_dir)

    TactileKeycap(
        btn_row,
        title="OPEN FOLDER",
        f_tag="F1",
        command=_open_folder_action,
        is_orange=True
    ).pack(side="left", padx=(0, 8))

    TactileKeycap(
        btn_row,
        title="CHANGE LOCATION",
        f_tag="F4",
        command=_change_folder,
        is_orange=False
    ).pack(side="left")

    # -------------------------------------------------------------
    # 7. TRACK RECORDER // RECENT TRANSFERS (Teenage Engineering Tape Log)
    # -------------------------------------------------------------
    transfers_card = tk.Frame(
        scroll_frame,
        bg=COLOR_CARD_BG,
        highlightbackground=COLOR_CARD_BORDER,
        highlightthickness=1,
        padx=18,
        pady=16
    )
    transfers_card.pack(fill="x", padx=24, pady=8)

    tr_title_row = tk.Frame(transfers_card, bg=COLOR_CARD_BG)
    tr_title_row.pack(fill="x", pady=(0, 8))

    tk.Label(
        tr_title_row,
        text="TRACK RECORDER // SESSION HISTORY",
        font=get_sans(11, "bold"),
        fg=COLOR_TEXT_WHITE,
        bg=COLOR_CARD_BG
    ).pack(side="left")

    tk.Label(
        tr_title_row,
        text="CH-01",
        font=get_mono(9, "bold"),
        fg=COLOR_ORANGE,
        bg=COLOR_CARD_BG
    ).pack(side="left", padx=8)

    def _clear_transfers():
        clear_recent_pc_transfers()
        _refresh_transfers(force=True)

    tk.Button(
        tr_title_row,
        text="CLEAR TAPE",
        font=get_mono(8),
        bg="#1A1C20",
        fg=COLOR_TEXT_MUTED,
        activebackground="#262930",
        activeforeground="#FFFFFF",
        relief="flat",
        padx=8,
        pady=2,
        cursor="hand2",
        command=_clear_transfers
    ).pack(side="right")

    transfers_container = tk.Frame(transfers_card, bg=COLOR_CARD_BG)
    transfers_container.pack(fill="x")

    def _format_size(b: int) -> str:
        if b <= 0:
            return "0 B"
        kb = b / 1024.0
        mb = kb / 1024.0
        if mb >= 1.0:
            return f"{mb:.1f} MB"
        if kb >= 1.0:
            return f"{kb:.1f} KB"
        return f"{b} B"

    _last_transfers_fp = [None]

    def _refresh_transfers(force: bool = False):
        records = get_recent_pc_transfers()
        fp = tuple((r.get("timestamp"), r.get("file_name"), r.get("file_size")) for r in records[:8])

        if not force and fp == _last_transfers_fp[0]:
            return
        _last_transfers_fp[0] = fp

        try:
            scroll_pos = canvas.yview()
        except Exception:
            scroll_pos = None

        for w in transfers_container.winfo_children():
            w.destroy()

        if not records:
            tk.Label(
                transfers_container,
                text="TAPE EMPTY // STANDBY FOR INCOMING FLICK STREAM",
                font=get_mono(8),
                fg=COLOR_TEXT_DIM,
                bg=COLOR_CARD_BG,
                pady=10
            ).pack(anchor="w")
        else:
            save_dir = get_config().get("save_directory", "")
            for idx, rec in enumerate(records[:8], start=1):
                fname = rec.get("file_name", "Unknown")
                fsize = rec.get("file_size", 0)
                mode = rec.get("mode", "FILE")
                ts = rec.get("timestamp", 0)
                time_str = time.strftime("%H:%M:%S", time.localtime(ts)) if ts else ""

                row = tk.Frame(
                    transfers_container,
                    bg=COLOR_CARD_INNER,
                    highlightbackground=COLOR_CARD_INNER_BORDER,
                    highlightthickness=1,
                    padx=10,
                    pady=8
                )
                row.pack(fill="x", pady=3)

                # Track index e.g. #01
                tk.Label(
                    row,
                    text=f"#{idx:02d}",
                    font=get_mono(8, "bold"),
                    fg=COLOR_ORANGE,
                    bg=COLOR_CARD_INNER
                ).pack(side="left", padx=(0, 8))

                col = tk.Frame(row, bg=COLOR_CARD_INNER)
                col.pack(side="left", fill="both", expand=True)

                tk.Label(
                    col,
                    text=fname.upper(),
                    font=get_mono(9, "bold"),
                    fg=COLOR_TEXT_WHITE,
                    bg=COLOR_CARD_INNER,
                    anchor="w"
                ).pack(anchor="w")

                tk.Label(
                    col,
                    text=f"{_format_size(fsize)}  •  {time_str}  •  {mode}",
                    font=get_mono(8),
                    fg=COLOR_TEXT_DIM,
                    bg=COLOR_CARD_INNER,
                    anchor="w"
                ).pack(anchor="w")

                file_path = os.path.join(save_dir, fname)
                if mode == "FILE" and os.path.exists(file_path):
                    def _make_open_file(fp=file_path):
                        def _open():
                            try:
                                os.startfile(fp)
                            except Exception:
                                pass
                        return _open

                    tk.Button(
                        row,
                        text="PLAY // OPEN",
                        font=get_mono(8, "bold"),
                        bg=COLOR_ORANGE_BADGE_BG,
                        fg=COLOR_ORANGE_LIGHT,
                        activebackground=COLOR_ORANGE,
                        activeforeground="#FFFFFF",
                        relief="flat",
                        padx=8,
                        pady=2,
                        cursor="hand2",
                        command=_make_open_file()
                    ).pack(side="right")

        if scroll_pos:
            root.after_idle(lambda: canvas.yview_moveto(scroll_pos[0]))

    _refresh_transfers(force=True)

    # -------------------------------------------------------------
    # 8. PAIRED NODES // HARDWARE KEYSTORE (Zero Stutter Caching)
    # -------------------------------------------------------------
    devices_card = tk.Frame(
        scroll_frame,
        bg=COLOR_CARD_BG,
        highlightbackground=COLOR_CARD_BORDER,
        highlightthickness=1,
        padx=18,
        pady=16
    )
    devices_card.pack(fill="x", padx=24, pady=8)

    dev_title_row = tk.Frame(devices_card, bg=COLOR_CARD_BG)
    dev_title_row.pack(fill="x", pady=(0, 8))

    tk.Label(
        dev_title_row,
        text="PAIRED NODES // KEYSTORE",
        font=get_sans(11, "bold"),
        fg=COLOR_TEXT_WHITE,
        bg=COLOR_CARD_BG
    ).pack(side="left")

    tk.Label(
        dev_title_row,
        text="SLOT-01",
        font=get_mono(9, "bold"),
        fg=COLOR_ORANGE,
        bg=COLOR_CARD_BG
    ).pack(side="left", padx=8)

    devices_container = tk.Frame(devices_card, bg=COLOR_CARD_BG)
    devices_container.pack(fill="x")

    _last_devices_fp = [None]

    def _refresh_paired_devices(force: bool = False):
        current_cfg = get_config()
        devices = current_cfg.get("paired_devices", [])
        fp = tuple((d.get("client_id"), d.get("paired_at"), d.get("client_name")) for d in devices)

        if not force and fp == _last_devices_fp[0]:
            return
        _last_devices_fp[0] = fp

        try:
            scroll_pos = canvas.yview()
        except Exception:
            scroll_pos = None

        for w in devices_container.winfo_children():
            w.destroy()

        if not devices:
            tk.Label(
                devices_container,
                text="NO NODES PAIRED // SCAN MATRIX ABOVE TO PAIR CLIENT",
                font=get_mono(8),
                fg=COLOR_TEXT_DIM,
                bg=COLOR_CARD_BG,
                pady=8
            ).pack(anchor="w")
        else:
            for dev in devices:
                dev_id = dev.get("client_id", "unknown")
                dev_name = dev.get("client_name", "Android Phone")
                spki = dev.get("client_spki_pin", "")
                spki_short = f"{spki[:8]}...{spki[-6:]}" if spki else "N/A"
                paired_ts = dev.get("paired_at", 0)
                paired_str = time.strftime("%Y-%m-%d %H:%M", time.localtime(paired_ts)) if paired_ts else "ACTIVE"

                item_frame = tk.Frame(
                    devices_container,
                    bg=COLOR_CARD_INNER,
                    highlightbackground=COLOR_CARD_INNER_BORDER,
                    highlightthickness=1,
                    padx=12,
                    pady=10
                )
                item_frame.pack(fill="x", pady=4)

                info_col = tk.Frame(item_frame, bg=COLOR_CARD_INNER)
                info_col.pack(side="left", fill="both", expand=True)

                tk.Label(
                    info_col,
                    text=f"NODE: {dev_name.upper()}",
                    font=get_mono(9, "bold"),
                    fg=COLOR_ORANGE_LIGHT,
                    bg=COLOR_CARD_INNER
                ).pack(anchor="w")

                tk.Label(
                    info_col,
                    text=f"SPKI: {spki_short}  •  LINK: {paired_str}",
                    font=get_mono(8),
                    fg=COLOR_TEXT_DIM,
                    bg=COLOR_CARD_INNER
                ).pack(anchor="w")

                def _make_unpair_cmd(cid=dev_id):
                    def _unpair():
                        remove_paired_device(cid)
                        _refresh_paired_devices(force=True)
                    return _unpair

                unpair_btn = tk.Button(
                    item_frame,
                    text="DISENGAGE",
                    font=get_mono(8, "bold"),
                    bg=COLOR_DANGER_BG,
                    fg=COLOR_DANGER,
                    activebackground="#451212",
                    activeforeground="#FFFFFF",
                    relief="flat",
                    padx=10,
                    pady=3,
                    cursor="hand2",
                    command=_make_unpair_cmd()
                )
                unpair_btn.pack(side="right", padx=(8, 0))

        if scroll_pos:
            root.after_idle(lambda: canvas.yview_moveto(scroll_pos[0]))

    _refresh_paired_devices(force=True)

    # -------------------------------------------------------------
    # 9. CHASSIS CALIBRATION & SYSTEM CONTROLS
    # -------------------------------------------------------------
    sys_card = tk.Frame(
        scroll_frame,
        bg=COLOR_CARD_BG,
        highlightbackground=COLOR_CARD_BORDER,
        highlightthickness=1,
        padx=18,
        pady=16
    )
    sys_card.pack(fill="x", padx=24, pady=(8, 24))

    tk.Label(
        sys_card,
        text="SYSTEM CONTROLS // DAEMON CONFIG",
        font=get_sans(11, "bold"),
        fg=COLOR_TEXT_WHITE,
        bg=COLOR_CARD_BG
    ).pack(anchor="w", pady=(0, 8))

    startup_var = tk.BooleanVar(value=is_startup_enabled())

    def _toggle_startup():
        set_startup_enabled(startup_var.get())

    cb = tk.Checkbutton(
        sys_card,
        text="Start Flick automatically on Windows startup",
        variable=startup_var,
        font=get_sans(9),
        fg=COLOR_TEXT_WHITE,
        bg=COLOR_CARD_BG,
        selectcolor=COLOR_CARD_INNER,
        activebackground=COLOR_CARD_BG,
        activeforeground=COLOR_ORANGE_LIGHT,
        command=_toggle_startup
    )
    cb.pack(anchor="w", pady=(0, 14))

    ctrl_row = tk.Frame(sys_card, bg=COLOR_CARD_BG)
    ctrl_row.pack(fill="x")

    def _minimize_to_tray():
        root.withdraw()

    tk.Button(
        ctrl_row,
        text="HIDE TO TRAY",
        font=get_mono(8, "bold"),
        bg="#1D2026",
        fg=COLOR_ORANGE_LIGHT,
        activebackground="#2A2F38",
        activeforeground="#FFFFFF",
        relief="flat",
        padx=14,
        pady=6,
        cursor="hand2",
        command=_minimize_to_tray
    ).pack(side="left", padx=(0, 8))

    def _quit_app():
        global _dashboard_open, _current_dashboard_root
        _dashboard_open = False
        _current_dashboard_root = None
        root.destroy()
        if _on_quit_handler:
            _on_quit_handler()
        else:
            sys.exit(0)

    tk.Button(
        ctrl_row,
        text="DISENGAGE & QUIT",
        font=get_mono(8, "bold"),
        bg=COLOR_DANGER_BG,
        fg=COLOR_DANGER,
        activebackground="#451212",
        activeforeground="#FFFFFF",
        relief="flat",
        padx=14,
        pady=6,
        cursor="hand2",
        command=_quit_app
    ).pack(side="right")

    # -------------------------------------------------------------
    # 10. AUTO-REFRESH TIMER
    # -------------------------------------------------------------
    last_poll_ts = [0.0]

    def _poll_updates():
        if not _dashboard_open or not root.winfo_exists():
            return
        try:
            now = time.time()
            rem = int(_qr_expiry_ts[0] - now)
            if rem > 0:
                timer_var.set(f"TTL: {rem // 60:02d}M {rem % 60:02d}S")
            else:
                timer_var.set("TTL: REFRESHING...")
                _refresh_qr()

            if now - last_poll_ts[0] >= 2.5:
                last_poll_ts[0] = now
                _refresh_paired_devices(force=False)
                _refresh_transfers(force=False)
        except Exception:
            pass
        root.after(1000, _poll_updates)

    root.after(1000, _poll_updates)

    def _on_close():
        root.withdraw()

    root.protocol("WM_DELETE_WINDOW", _on_close)

    if start_minimized:
        root.withdraw()
    else:
        root.deiconify()
        root.lift()
        root.attributes("-topmost", True)
        root.after(200, lambda: root.attributes("-topmost", False))
        root.focus_force()

    return root


if __name__ == "__main__":
    r = _build_dashboard_ui(start_minimized=False)
    r.mainloop()
