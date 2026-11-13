"""
Flick Windows Receiver - Standalone Executable Build Script
Bundles the complete Flick Windows Receiver into a zero-dependency standalone Flick.exe.
"""

import os
import shutil
import subprocess
import sys
from PIL import Image, ImageDraw


def ensure_icons(dir_path: str):
    """Ensures high-resolution flick.ico and flick.png exist with the Concept 4.1 '3 Primitives' design."""
    ico_path = os.path.join(dir_path, "flick.ico")
    png_path = os.path.join(dir_path, "flick.png")

    if not os.path.exists(ico_path) or not os.path.exists(png_path):
        print("[Build] Generating Concept 4.1 '3 Primitives' (Phone + PC + Orange Flick) icons...")

        def make_icon(s):
            scale = 4
            w = s * scale
            img = Image.new("RGBA", (w, w), (0, 0, 0, 0))
            d = ImageDraw.Draw(img)
            pad = int(w * 0.04)
            rad = int(w * 0.22)
            d.rounded_rectangle([pad, pad, w - pad, w - pad], radius=rad, fill=(12, 13, 16, 255))
            f = w / 64.0
            # 1. Phone vertical pill: x=17..24, y=14..50, rx=4
            d.rounded_rectangle([int(17 * f), int(14 * f), int(24 * f), int(50 * f)], radius=max(2, int(4 * f)), fill=(232, 237, 242, 255))
            # 2. PC horizontal pill: x=27..48, y=14..22, rx=4
            d.rounded_rectangle([int(27 * f), int(14 * f), int(48 * f), int(22 * f)], radius=max(2, int(4 * f)), fill=(232, 237, 242, 255))
            # 3. Orange Flick pill: x=29..43, y=30..38, rx=4
            d.rounded_rectangle([int(29 * f), int(30 * f), int(43 * f), int(38 * f)], radius=max(2, int(4 * f)), fill=(255, 68, 0, 255))
            d.ellipse([int(38 * f), int(33 * f), int(41 * f), int(35 * f)], fill=(255, 255, 255, 255))
            return img.resize((s, s), Image.Resampling.LANCZOS)

        sizes = [16, 24, 32, 48, 64, 128, 256]
        imgs = [make_icon(s) for s in sizes]
        imgs[-1].save(ico_path, format="ICO", append_images=imgs[:-1])
        imgs[-1].save(png_path, format="PNG")
        print(f"[Build] Icons saved: {ico_path}, {png_path}")


def build_executable():
    receiver_dir = os.path.dirname(os.path.abspath(__file__))
    os.chdir(receiver_dir)

    ensure_icons(receiver_dir)

    ico_path = os.path.join(receiver_dir, "flick.ico")
    png_path = os.path.join(receiver_dir, "flick.png")

    print("=" * 60)
    print("[Build] Packaging Flick Windows Receiver into Standalone Flick.exe")
    print(f"Working Directory: {receiver_dir}")
    print("=" * 60)

    import PyInstaller.__main__

    fonts_dir = os.path.join(receiver_dir, "fonts")

    args = [
        "flick_receiver.py",
        "--name=Flick",
        "--onefile",
        "--windowed",
        "--noconsole",
        f"--icon={ico_path}",
        f"--add-data={ico_path};.",
        f"--add-data={png_path};.",
        f"--add-data={fonts_dir};fonts",
        "--hidden-import=pystray._win32",
        "--hidden-import=win32clipboard",
        "--hidden-import=win32con",
        "--hidden-import=win32gui",
        "--hidden-import=win32api",
        "--hidden-import=win32crypt",
        "--hidden-import=crypto_manager",
        "--hidden-import=desktop_dashboard",
        "--collect-submodules=PIL",
        "--collect-submodules=qrcode",
        "--collect-all=cryptography",
        "--clean",
        "-y",
    ]

    PyInstaller.__main__.run(args)

    dist_exe = os.path.join(receiver_dir, "dist", "Flick.exe")
    if not os.path.exists(dist_exe):
        print("[Build] Error: dist/Flick.exe was not created!")
        sys.exit(1)

    size_mb = os.path.getsize(dist_exe) / (1024 * 1024)
    print(f"\n[Build] SUCCESS! Generated: {dist_exe} ({size_mb:.2f} MB)")

    # Copy to windows-receiver/Flick.exe and root workspace Flick.exe
    local_copy = os.path.join(receiver_dir, "Flick.exe")
    shutil.copy2(dist_exe, local_copy)
    print(f"[Build] Copied to: {local_copy}")

    root_dir = os.path.abspath(os.path.join(receiver_dir, ".."))
    root_copy = os.path.join(root_dir, "Flick.exe")
    try:
        shutil.copy2(dist_exe, root_copy)
        print(f"[Build] Copied to workspace root: {root_copy}")
    except Exception as e:
        print(f"[Build] Note: Could not copy to root: {e}")

    print("=" * 60)
    print("[Build] Flick.exe is ready for one-click deployment without Python!")
    print("=" * 60)


if __name__ == "__main__":
    build_executable()
