# 🧠 Flick — Engineering Memory & System Ledger (`MEMORY.md`)

**Last Updated:** 2026-11-08T06:17:00+06:00  
**Active Project:** Flick (Local High-Speed Wi-Fi Companion Ecosystem)  
**Root Path:** `D:\Sandbox\EkSend`  
**Target Hardware:** 
- **PC:** `PARVEZRAHMAN` (Windows 11, LAN IP: `192.168.0.200`, Receiver Port: `45455`, UDP Beacon: `45454`)
- **Android Device:** Connected wirelessly via ADB at `192.168.0.197:35135`

---

## 1. Project Identity & Architecture Overview

Flick is a zero-latency, local-network PC-to-Android companion suite designed to bypass cloud latency and telemetry. It provides instant bi-directional file streaming, clipboard beaming, and automated Windows clipboard injection directly into the OS workflow (supporting `CF_HDROP` Explorer file drops, `CF_UNICODETEXT`, and `CF_DIB` image bitmaps).

### System Topology
- **Android Client (`android-app`):** Kotlin + Jetpack Compose edge-to-edge UI, OkHttp HTTP/1.1 chunked streaming (with `X-Batch-Id` multi-file batching), UDP broadcast discovery listener, and hardware-haptic feedback.
- **Windows Receiver (`windows-receiver-rust`):** High-performance, pure Rust native PE64 daemon (~2.01 MB static binary, ~3.4 MB private working set RAM). Zero Python runtime dependency. Implements pure-Rust TLS 1.3 engine with `rustls` (ring), UDP presence beacon (`FLICK_DISCOVERY_V2`), serialized Win32 multi-format clipboard worker (`CF_HDROP`, `CF_UNICODETEXT`, EXIF-aware `CF_DIB`, plus `Win + V` `CF_DIB` history stacking at `700ms` intervals with `CanIncludeInClipboardHistory = 0` on the final batch snapshot), native tray icon with silent toasts (`NIIF_NOSOUND`), and ephemeral Teenage Engineering GDI dashboard (540x680) with zero-RAM-when-closed working set trimming. Drop-in binary at `D:\Sandbox\EkSend\Flick.exe`.

---

## 2. Design System: Authentic Teenage Engineering (TE TX-1 / RX-1)

Flick adheres to a strict **Teenage Engineering Hardware Aesthetic** modeled after physical field instruments (OP-1, TP-7, EP-133, and OB-4). All generic "AI slop" (floating purple cards, checklist circles, flat monochromatic bento boxes) is forbidden.

### Core Color Tokens
| Token | Hex | Role |
| :--- | :--- | :--- |
| `COLOR_BG` | `#0A0B0E` | Deep machined obsidian chassis |
| `COLOR_SURFACE` | `#131519` | Layered gunmetal panel surface (distinct from bg) |
| `COLOR_SURFACE_VARIANT` | `#1A1D23` | Recessed telemetry wells & secondary sockets |
| `COLOR_OUTLINE` | `#22252C` | Machined module borders & chassis seams |
| `COLOR_ORANGE` | `#FF4400` | Signature International Safety Orange |
| `COLOR_ORANGE_BRIGHT` | `#FF5500` | Keycap upper highlight gradient |
| `COLOR_ORANGE_DARK` | `#E63800` | Keycap lower depression gradient |
| `COLOR_ORANGE_LIP` | `#7A1E00` / `#992600` | 3D mechanical button bottom shadow lip |
| `COLOR_KEYCAP_DARK` | `#353944` → `#22252D` | Machined dark graphite tactile keycap gradient |
| `COLOR_KEYCAP_LIP` | `#08090C` | Dark keycap bottom shadow lip |

### 3D Keycap Physics & Travel
- **Dual-Layer Architecture:** Outer socket acts as the mechanical well exposing the bottom shadow lip (6dp idle). The inner plunger animates with `pressOffsetY` (5dp downward travel on click) and compresses the shadow lip (down to 1dp).
- **Buttons with 3D Travel:**
  - `F1 TRANSMIT FILES` (`BentoActionTile.kt`, Safety Orange, white document icon, subtitle `PHOTOS // 4K // DOC`).
  - `F2 CLIPBOARD` (`BentoActionTile.kt`, Machined Graphite, Safety Orange bolt icon, subtitle `INSTANT PC PASTE`).
  - `INJECTION FORMAT MATRIX` (`TactileModeKeycap` in `DeviceSettingsScreen.kt`, 2x2 grid with `01 MULTI`, `02 PATH`, `03 BITMAP`, `04 FILE`).

### TP-7 Motorized Tape Spool
- Implemented in `ScallopedDeviceBadge.kt`.
- Features brushed aluminum outer disc, concentric grooves, 3 precision spoke lines, and a center Safety Orange drive spindle (`⚡`).
- Rotates dynamically during network sync and file transfers; serves as a tactile ping button.

### Typography Rules
- **Headings & Titles:** Swiss Bold Sans (`FontWeight.Bold / Black`, `FontFamily.Default`), e.g., `FLICK TX-1`, `TRANSMIT FILES`, `CLIPBOARD`, `CALIBRATION SYS-01`.
- **Telemetry & Ports:** Strict technical monospace (`FontFamily.Monospace`, 8-10sp), e.g., `LINK: CONNECTED // TLS 1.3`, `SPEED: |||||`, `RTT: 34 MS`.
- **Anti-Pattern Guard:** Never use unstyled monospace text dumps for settings forms or list descriptions.

---

## 3. Navigation & Edge-to-Edge System Architecture

- **Predictive Back Navigation:** Jetpack Compose apps without Jetpack Navigation (using `AnimatedContent` over state enums) require explicit `BackHandler` hooks.
- **Rules Enforced:**
  - `MainActivity.kt`: Declares `BackHandler(enabled = currentScreen != Screen.HOME) { currentScreen = Screen.HOME }`.
  - `DeviceSettingsScreen.kt` and `PairingScannerScreen.kt`: Contain internal `BackHandler { onBack() }` so system swipe gestures and back buttons seamlessly return to Home without terminating the process.
- **Monet Dynamic Theming Disabled:** Android 12+ dynamic wallpaper extraction (`dynamicColor = false`) is permanently disabled in `Theme.kt` to preserve the signature high-contrast Safety Orange and Machined Obsidian palette.

---

## 4. Hardware Deployment & Workspace Artifacts

### Active Binaries
- **Android APK:** `D:\Sandbox\EkSend\Flick-v2.1.2.apk` (also mirrored at `android-app/app/build/outputs/apk/debug/app-debug.apk`).
- **Windows Executable (Rust PE64 Native):** `D:\Sandbox\EkSend\Flick.exe` (2.01 MB, 3.4 MB RAM) and `windows-receiver\dist\Flick.exe` (replaces legacy 28.2 MB Python binary).

### ADB Wireless Deployment Workflow
```powershell
# 1. Connect over Wi-Fi
adb connect 192.168.0.197:35135

# 2. Build APK
cd android-app; .\gradlew assembleDebug

# 3. Deploy & Launch
adb -s 192.168.0.197:35135 install -r Flick-v2.1.2.apk
adb -s 192.168.0.197:35135 shell am start -n com.eksend.flick/.MainActivity
```

---

## 5. User Preferences & Critical Directives

1. **No AI Slop:** Always ground UI in genuine physical hardware aesthetics (Teenage Engineering). Never revert to generic bootstrap/tailwind bento cards, checklist checkmarks, or flat feature lists.
2. **Runnable Code First:** Never leave stubs or pseudo-code; verify all code builds with Gradle/PyInstaller and installs cleanly to hardware.
3. **Contrast Integrity:** Maintain strict contrast between `#0A0B0E` (chassis background) and `#131519` (panels/cards).
4. **Header Cleanliness:** No `// ` prefixes on section titles, no audio `VU:` misnomers (use `SPEED:`), and no unneeded decorative dots next to brand titles.
