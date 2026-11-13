"""
Flick Windows Receiver - Win32 Solid Theme Enforcer, High-DPI Engine & Teenage Engineering Fonts
Defeats third-party translucency mods (such as Windhawk Translucent Windows)
and guarantees crisp, 100% opaque, high-contrast, razor-sharp hardware rendering
with authentic Work Sans, Space Mono, and Silkscreen typography.
"""

import ctypes
from ctypes import wintypes
import math
import os
import sys
import tkinter as tk
from tkinter import font as tkfont
from typing import Dict, Optional, Tuple
from PIL import Image, ImageDraw, ImageTk

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32
dwmapi = ctypes.WinDLL("dwmapi")

# -------------------------------------------------------------
# 1. HIGH-DPI AWARENESS INITIALIZATION
# -------------------------------------------------------------
def enable_high_dpi_awareness() -> None:
    """
    Enables Windows Per-Monitor V2 DPI awareness.
    Without this, Windows DWM bilinearly stretches 96 DPI Tkinter windows on 125%/150% scaled displays,
    making all typography, buttons, and borders look blurry ('jhapsha').
    """
    try:
        # Per-Monitor V2 (Windows 10 Creators Update 1703+ / Windows 11)
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = ctypes.c_void_p(-4)
        user32.SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
    except Exception:
        try:
            # Fallback to Shcore Per-Monitor V1 (Windows 8.1+)
            ctypes.windll.shcore.SetProcessDpiAwareness(2)
        except Exception:
            try:
                # Legacy System DPI Aware fallback
                user32.SetProcessDPIAware()
            except Exception:
                pass


# Execute DPI awareness immediately upon import before any GUI initialization
enable_high_dpi_awareness()


def get_dpi_scale(root: tk.Tk) -> float:
    """Returns the display DPI scaling factor (e.g. 1.25 for 125% scaling, 1.5 for 150%)."""
    try:
        root.update_idletasks()
        dpi = float(root.winfo_fpixels("1i"))
        scale = dpi / 96.0
        return max(1.0, scale)
    except Exception:
        return 1.0


# -------------------------------------------------------------
# 2. EMBEDDED TEENAGE ENGINEERING FONTS (Work Sans & Space Mono)
# -------------------------------------------------------------
_fonts_loaded = False
_sans_family = "Work Sans"
_mono_family = "Space Mono"
_pixel_family = "Silkscreen"
_screw_cache: Dict[Tuple[int, int], ImageTk.PhotoImage] = {}


def load_embedded_fonts() -> None:
    """
    Registers bundled TTF fonts (Work Sans, Space Mono, Silkscreen) into the Win32 GDI font table
    using FR_PRIVATE (0x10) so they become immediately available to Tkinter and Win32 controls
    without requiring Windows administrator rights or permanent registry changes.
    """
    global _fonts_loaded, _sans_family, _mono_family, _pixel_family
    if _fonts_loaded:
        return
    _fonts_loaded = True

    candidate_dirs = [
        getattr(sys, "_MEIPASS", ""),
        os.path.dirname(os.path.abspath(__file__)),
        os.path.join(os.path.dirname(os.path.abspath(__file__)), "fonts"),
    ]

    for d in candidate_dirs:
        if not d:
            continue
        fonts_sub = os.path.join(d, "fonts") if not d.endswith("fonts") else d
        if os.path.exists(fonts_sub):
            for fname in os.listdir(fonts_sub):
                if fname.lower().endswith((".ttf", ".otf")):
                    fpath = os.path.join(fonts_sub, fname)
                    try:
                        gdi32.AddFontResourceExW(fpath, 0x10, 0)
                    except Exception:
                        pass

    # Verify availability in Tkinter font families (with fallback)
    try:
        temp_root = tk.Tk()
        temp_root.withdraw()
        families = set(tkfont.families(temp_root))
        temp_root.destroy()

        if "Work Sans" in families:
            _sans_family = "Work Sans"
        elif "Segoe UI" in families:
            _sans_family = "Segoe UI"
        else:
            _sans_family = "Arial"

        if "Space Mono" in families:
            _mono_family = "Space Mono"
        elif "Cascadia Code" in families:
            _mono_family = "Cascadia Code"
        elif "Consolas" in families:
            _mono_family = "Consolas"
        else:
            _mono_family = "Courier New"

        if "Silkscreen" in families:
            _pixel_family = "Silkscreen"
        else:
            _pixel_family = _mono_family
    except Exception:
        pass


# Run font registration immediately
load_embedded_fonts()


def get_sans(size: int = 10, weight: str = "normal") -> Tuple[str, int, str]:
    """Returns the primary Swiss Grotesk font (Work Sans) with exact size and weight."""
    return (_sans_family, size, weight)


def get_mono(size: int = 9, weight: str = "normal") -> Tuple[str, int, str]:
    """Returns the technical monospace font (Space Mono) with exact size and weight."""
    return (_mono_family, size, weight)


def get_pixel(size: int = 8, weight: str = "normal") -> Tuple[str, int, str]:
    """Returns the CRT/segment telemetry font (Silkscreen or Space Mono)."""
    return (_pixel_family, size, weight)


def get_screw_tk(size: int = 12, angle_deg: int = 35) -> ImageTk.PhotoImage:
    """
    Renders an authentic Teenage Engineering machined slotted screw head
    with radial bevel, specular highlight, and rotated screwdriver slot.
    """
    key = (size, angle_deg)
    if key in _screw_cache:
        return _screw_cache[key]

    scale = 4
    w = size * scale
    img = Image.new("RGBA", (w, w), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # Outer screw rim (dark gunmetal)
    d.ellipse([0, 0, w - 1, w - 1], fill=(52, 56, 64, 255), outline=(22, 24, 28, 255), width=max(1, scale))
    # Inner screw face bevel
    inset = max(1, int(scale * 0.8))
    d.ellipse([inset, inset, w - 1 - inset, w - 1 - inset], fill=(32, 35, 40, 255))
    # Top-left specular crescent highlight
    d.arc([inset, inset, w - 1 - inset, w - 1 - inset], start=200, end=320, fill=(100, 108, 120, 200), width=scale)

    # Rotated screwdriver slot
    rad = math.radians(angle_deg)
    cx, cy = w / 2.0, w / 2.0
    slot_len = w * 0.36
    dx = math.cos(rad) * slot_len
    dy = math.sin(rad) * slot_len
    slot_w = max(2, int(scale * 1.5))
    d.line([(cx - dx, cy - dy), (cx + dx, cy + dy)], fill=(12, 13, 16, 255), width=slot_w)

    res = img.resize((size, size), Image.Resampling.LANCZOS)
    photo = ImageTk.PhotoImage(res)
    _screw_cache[key] = photo
    return photo


# -------------------------------------------------------------
# 2. WIN32 CONSTANTS & STRUCTURES
# -------------------------------------------------------------
GWL_STYLE = -16
GWL_EXSTYLE = -20
WS_MINIMIZEBOX = 0x00020000
WS_MAXIMIZEBOX = 0x00010000
WS_THICKFRAME = 0x00040000
WS_EX_LAYERED = 0x00080000

SWP_FRAMECHANGED = 0x0020
SWP_NOMOVE = 0x0002
SWP_NOSIZE = 0x0001
SWP_NOZORDER = 0x0004
SWP_FLAGS = SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER

DWMWA_USE_IMMERSIVE_DARK_MODE = 20
DWMWA_SYSTEMBACKDROP_TYPE = 38
DWMSBT_NONE = 1


class MARGINS(ctypes.Structure):
    _fields_ = [
        ("cxLeftWidth", ctypes.c_int),
        ("cxRightWidth", ctypes.c_int),
        ("cyTopHeight", ctypes.c_int),
        ("cyBottomHeight", ctypes.c_int),
    ]


class DWM_BLURBEHIND(ctypes.Structure):
    _fields_ = [
        ("dwFlags", ctypes.c_uint32),
        ("fEnable", ctypes.c_int),
        ("hRgnBlur", ctypes.c_void_p),
        ("fTransitionOnMaximized", ctypes.c_int),
    ]


class ACCENT_POLICY(ctypes.Structure):
    _fields_ = [
        ("AccentState", ctypes.c_int),
        ("AccentFlags", ctypes.c_int),
        ("GradientColor", ctypes.c_int),
        ("AnimationId", ctypes.c_int),
    ]


class WINCOMPATTRDATA(ctypes.Structure):
    _fields_ = [
        ("Attrib", ctypes.c_int),
        ("pvData", ctypes.c_void_p),
        ("cbData", ctypes.c_size_t),
    ]


def get_toplevel_hwnd(root: tk.Tk) -> int:
    """Returns the true top-level Win32 HWND (GA_ROOTOWNER), not Tkinter's internal child frame."""
    try:
        root.update_idletasks()
        child_hwnd = root.winfo_id()
        top_hwnd = user32.GetAncestor(child_hwnd, 3)  # GA_ROOTOWNER
        return top_hwnd if top_hwnd else child_hwnd
    except Exception:
        return 0


def enforce_solid_dark_theme(root: tk.Tk, strip_caption_buttons: bool = True) -> None:
    """
    Applies aggressive Win32 and DWM overrides to guarantee 100% solid, non-translucent window rendering.
    
    Windhawk's Translucent Windows mod hooks NtUserCreateWindowEx and DwmExtendFrameIntoClientArea.
    By stripping WS_MINIMIZEBOX/MAXIMIZEBOX/THICKFRAME, Windhawk's IsWindowEligible() returns FALSE,
    disabling its hook and allowing us to cancel frame extension, blur-behind, and composition accents.
    """
    def _apply():
        try:
            top_hwnd = get_toplevel_hwnd(root)
            if not top_hwnd:
                return

            # 1. Modify Window Styles to disqualify from Windhawk translucency injection
            style = user32.GetWindowLongW(top_hwnd, GWL_STYLE)
            if strip_caption_buttons:
                style &= ~(WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_THICKFRAME)
                user32.SetWindowLongW(top_hwnd, GWL_STYLE, style)

            # 2. Strip WS_EX_LAYERED so alpha blending is completely removed
            ex_style = user32.GetWindowLongW(top_hwnd, GWL_EXSTYLE)
            if ex_style & WS_EX_LAYERED:
                user32.SetWindowLongW(top_hwnd, GWL_EXSTYLE, ex_style & ~WS_EX_LAYERED)

            user32.SetWindowPos(top_hwnd, 0, 0, 0, 0, 0, SWP_FLAGS)

            # 3. Restore DWM Glass Frame to {0, 0, 0, 0} (solid client area)
            zero_margins = MARGINS(0, 0, 0, 0)
            dwmapi.DwmExtendFrameIntoClientArea(top_hwnd, ctypes.byref(zero_margins))

            # 4. Explicitly disable DWM Blur Behind
            bb = DWM_BLURBEHIND()
            bb.dwFlags = 1  # DWM_BB_ENABLE
            bb.fEnable = 0
            dwmapi.DwmEnableBlurBehindWindow(top_hwnd, ctypes.byref(bb))

            # 5. Disable SetWindowCompositionAttribute accent policy (ACCENT_STATE_DISABLED = 0)
            policy = ACCENT_POLICY()
            policy.AccentState = 0
            policy.AccentFlags = 0
            policy.GradientColor = 0
            policy.AnimationId = 0

            data = WINCOMPATTRDATA()
            data.Attrib = 19  # WCA_ACCENT_POLICY
            data.pvData = ctypes.cast(ctypes.byref(policy), ctypes.c_void_p)
            data.cbData = ctypes.sizeof(policy)

            set_win_comp = getattr(user32, "SetWindowCompositionAttribute", None)
            if set_win_comp:
                set_win_comp(top_hwnd, ctypes.byref(data))

            # 6. Disable Backdrop Material (DWMSBT_NONE = 1)
            backdrop = ctypes.c_uint(DWMSBT_NONE)
            dwmapi.DwmSetWindowAttribute(
                top_hwnd, DWMWA_SYSTEMBACKDROP_TYPE, ctypes.byref(backdrop), ctypes.sizeof(backdrop)
            )

            # 7. Force Win32 Immersive Dark Mode for title bar & borders
            dark_mode = ctypes.c_int(1)
            dwmapi.DwmSetWindowAttribute(
                top_hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, ctypes.byref(dark_mode), ctypes.sizeof(dark_mode)
            )

        except Exception:
            pass

    # Execute immediately
    _apply()

    # Re-apply after window is mapped and idle to beat any late-firing hooks
    try:
        root.bind("<Map>", lambda e: _apply())
        root.after(30, _apply)
        root.after(150, _apply)
        root.after(400, _apply)
    except Exception:
        pass
