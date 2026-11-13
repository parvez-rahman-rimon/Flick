# 🚀 Flick — Comprehensive Feature Matrix & Product Roadmap (`FEATURES.md`)

**Document Version:** 1.0.1  
**Target Workspace:** `D:\Sandbox\EkSend`

---

## 1. Feature Inventory Matrix

### ✅ Phase 1: v1.0.1 Core MVP & Hardened Security (Implemented & Verified)

| Feature ID | Feature Name | Component | Technical Mechanism & Acceptance Criteria |
| :--- | :--- | :--- | :--- |
| **F-101** | **Zero-UI 1-Tap Share Target** | Android (`TeleportShareActivity`) | • Configured with `@android:style/Theme.NoDisplay`, `excludeFromRecents="true"`, `noHistory="true"`.<br>• Handles `ACTION_SEND` (`*/*`) and `ACTION_SEND_MULTIPLE` (`*/*`).<br>• Extracts `EXTRA_STREAM` URI(s) or `EXTRA_TEXT`, starts `UploadService`, and calls `finish()` in `< 50ms`. |
| **F-102** | **Chunked Binary Upload Service** | Android (`UploadService`) | • Foreground Service (`foregroundServiceType="dataSync"`) reading `ContentResolver.openInputStream(uri)` in `64 KB` chunks.<br>• Streams raw bytes via OkHttp `RequestBody` to `POST http://{ip}:45455/upload`.<br>• Automatically stops itself (`stopSelf()`) as soon as queue completes. |
| **F-103** | **Tactile Haptic & Toast Confirmation** | Android (`HapticHelper`) | • Fires a `50ms` `EFFECT_CLICK` vibration and `"⚡ Copied to PC Clipboard"` toast immediately upon HTTP `200 OK` from Windows. |
| **F-104** | **Fast-Path + UDP Auto-Routing** | Android (`PcAutoRouter`) | • Pings cached `lastKnownIp:45455/ping` with a `150ms` timeout.<br>• On timeout/DHCP change, listens on UDP `45454` (`FLICK_DISCOVERY_V1`) for up to `2.2s`, updates cached IP automatically, and completes upload without user intervention. |
| **F-105** | **Win32 Multi-Format Clipboard Injector** | Windows (`clipboard_engine.py`) | • Single atomic `OpenClipboard` session with 10-attempt contention retry.<br>• Injects `CF_HDROP` (`DROPFILES` UTF-16LE struct), `CF_UNICODETEXT` (absolute file path), and `CF_DIB` (EXIF-transposed 24-bit BMP stripped of 14-byte header) simultaneously in `SMART_MULTI` mode. |
| **F-106** | **4-Mode Clipboard Switcher** | Windows Tray & Android Settings | • Supports switching between: `SMART_MULTI` (File + Path + Image), `FORCE_PATH` (Text path only), `FORCE_IMAGE` (Bitmap + File only), and `FORCE_FILE` (`CF_HDROP` only).<br>• Configurable both from Windows System Tray menu and Android Device Settings screen. |
| **F-107** | **Windows System Tray Companion** | Windows (`flick_receiver.py` & `Flick.exe`) | • Runs silently in Windows notification area using `pystray`.<br>• Context menu provides: Active Status (`IP:Port`), Clipboard Mode radio toggles, `"Pair New Phone (Show QR)"`, `"Open Flick Downloads Folder"`, and `"Exit"`. |
| **F-108** | **QR Code + LAN Discovery Pairing** | Windows + Android (`PairingScannerScreen` & `DiscoveredPairCard`) | • Windows Tray generates a Tkinter QR code window containing `ip`, `port`, `pc_id`, `pc_name`, and `auth_token`.<br>• Android app can pair either by scanning the QR code (CameraX + ZXing/MLKit) or tapping the split-action `"Pair request"` banner discovered via UDP beacon. |
| **F-109** | **M3 Expressive Companion Dashboard** | Android (`MainActivity` + `HomeScreen`) | • Implements tintotint's *"KDE Connect of the Future"* M3 Expressive design (`ScallopedDeviceBadge`, `DiscoveredPairCard`, 2-column Bento action tiles, bottom paired status pill).<br>• Uses Android 12+ `dynamicLightColorScheme` / `dynamicDarkColorScheme` with Lavender-Indigo & Electric Teal fallback palette. |
| **F-110** | **In-App Quick Send (`Files` & `Clipboard`)** | Android (`HomeScreen`) | • Tapping the **"Files"** Bento tile opens the system multi-file picker (`GetMultipleContents`) and teleports chosen files to PC.<br>• Tapping the **"Clipboard"** Bento tile sends the phone's current clipboard text (or prompt text) to the PC clipboard via `POST /clipboard`. |
| **F-111** | **Standalone Windows Executable (`Flick.exe`)** | Windows (`build_exe.py` / PyInstaller) | • Standalone, zero-dependency 21.8 MB binary with branded `flick.ico`.<br>• Includes single-instance Win32 Named Mutex and global pairing hotkeys `Win+Shift+F` and `Ctrl+Alt+F`. |
| **F-112** | **NTFS Mark-of-the-Web & Dangerous File Quarantine** | Windows (`http_receiver.py`) | • 37 high-risk executable, script, and shortcut extensions quarantined with `.blocked` and excluded from `CF_HDROP`.<br>• Stamped with alternate data stream `:Zone.Identifier` (`ZoneId=3`) to enforce Windows SmartScreen. |
| **F-113** | **Terminal Command Injection Shield** | Windows (`http_receiver.py`) | • Sanitizes incoming filenames via `safe_filename()`: strips trailing dots/spaces, replaces shell metacharacters `&;^%$()[]` with `_`, and handles DOS reserved names (`CON`, `PRN`, `NUL`). |
| **F-114** | **Windows 10/11 Clipboard History & Cloud Sync Privacy Opt-Out** | Windows (`clipboard_engine.py`) | • Injects `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory=0`, and `CanUploadToCloudClipboard=0` so sensitive OTPs/tokens are never synced to Microsoft cloud or stored in `Win+V`. |
| **F-115** | **Single-Instance Win32 Mutex & Hotkey Re-Trigger** | Windows (`flick_receiver.py`) | • Win32 Named Mutex `Local\Flick_Windows_Receiver_SingleInstance_Mutex` prevents port collision; secondary launch lifts the QR code modal and exits. |
| **F-116** | **Android Content URI Boundary & Packet-Bound Discovery** | Android (`TeleportShareActivity` & `UdpDiscoveryListener`) | • Restricts intents strictly to `content://` and rejects own package authority to prevent sandbox escape.<br>• Binds discovery IP directly to UDP packet source address to eliminate beacon spoofing. |

---

### ✅ Phase 2: v2.0 Zero-Trust Mutual TLS 1.3 & Hardware Keystore (Implemented & Verified)

| Feature ID | Feature Name | Status | Technical Mechanism & Verification |
| :--- | :--- | :--- | :--- |
| **F-201** | **Mutual TLS 1.3 (mTLS) with P-256 Key Pinning** | **Verified (10/10 E2E)** | • Fully eliminated static bearer token from network transit.<br>• Negotiates TLS 1.3 using ECDSA P-256 certs (`TLS_AES_256_GCM_SHA384`).<br>• Android pins PC SPKI SHA-256 fingerprint from QR; PC validates Android client certificate against DPAPI paired store. |
| **F-202** | **Android Keystore StrongBox & Win32 DPAPI Key Storage** | **Verified** | • Android private key generated inside hardware enclave (`AndroidKeyStore` with `setIsStrongBoxBacked(true)` and TEE fallback).<br>• Windows PC private key encrypted on disk with Win32 DPAPI (`CryptProtectData`) at `~/.flick/pc_key.enc`. |
| **F-203** | **Rotating UDP HMAC-SHA256 Beacon Authentication** | **Verified** | • UDP 45454 beacons (`FLICK_DISCOVERY_V2`) carry rolling tag: `HMAC-SHA256(pair_secret, floor(time / 30) + pc_id)`.<br>• Android client verifies tag with $\pm 1$ time-step window (90s tolerance) before making network calls, rejecting spoofed beacons. |

---

### 🔮 Phase 3: Future Roadmap (Ecosystem Expansion)

| Feature ID | Feature Name | Target Release | Architectural Specification & Acceptance Criteria |
| :--- | :--- | :--- | :--- |
| **F-204** | **Android Direct Share Targets (`ShortcutManager`)** | v2.1 | • Publishes dynamic `ShortcutInfoCompat` targets so paired PCs appear directly in the top row of the Android Share Sheet for true 1-tap teleport without opening chooser. |
| **F-205** | **Two-Way Teleport (Windows $\rightarrow$ Android)** | v2.1 | • Global Windows hotkey (`Ctrl+Shift+F`) captures current Windows clipboard or Explorer file selection and pushes to Android client via local HTTP listener or WebSocket. |
| **F-206** | **Persistent Transfer History Ledger & Re-Copy** | v2.1 | • Room database on Android storing persistent transfer history with thumbnail caching and 1-tap re-send.<br>• Search and filter by file type and target PC. |
| **F-207** | **Remote Input & Presentation Plugins** | v2.2 | • Inspired by KDE Connect: Touchpad control, Presentation slide remote, Media Player control, and remote command runner. |
| **F-208** | **mDNS / Zeroconf + Tailscale Mesh Auto-Sync** | v2.2 | • Supplements UDP broadcast with `_flick._tcp.local.` mDNS records and Tailscale MagicDNS resolution for remote/cross-subnet pairing. |

---

## 2. Clipboard Paste Behavior Matrix (Verification Reference)

When `SMART_MULTI` mode is active on the Windows Receiver and an image (`photo.jpg`) is shared from Android:

| Target Windows Application | Format Consumed on `Ctrl + V` | Resulting User Experience |
| :--- | :--- | :--- |
| **Antigravity IDE Chat / Cursor / Claude Desktop** | `CF_HDROP` / `CF_DIB` | Attaches the photo directly into the AI prompt input box! |
| **Windows Terminal / PowerShell / CMD** | `CF_UNICODETEXT` | Pastes `C:\Users\<User>\Downloads\Flick\photo.jpg` directly into the command line! |
| **VS Code Editor (Code Tab)** | `CF_UNICODETEXT` | Pastes the file path string into the source file! |
| **VS Code Explorer Sidebar** | `CF_HDROP` | Copies `photo.jpg` into the selected project directory! |
| **Google Chrome / Edge (GitHub, Gmail, WhatsApp Web, Discord)** | `CF_HDROP` / `CF_DIB` | Uploads/attaches `photo.jpg` directly into the web app! |
| **Windows File Explorer** | `CF_HDROP` | Pastes a copy of `photo.jpg` into the currently open folder! |
| **MS Paint / Photoshop / Figma** | `CF_DIB` | Pastes the uncompressed, EXIF-rotated bitmap image onto the canvas! |

> [!TIP]
> **What if a user wants to paste the file path string inside a rich-text box (like Notion, Word, or an AI Chat box) instead of attaching the image?**
> They simply toggle **`FORCE_PATH`** in the Windows Tray menu (or in the Flick Android app's quick toggle tile), which restricts the clipboard injection to `CF_UNICODETEXT` only!

---

## 3. Real-World Polish & Enhancements (v1.0.1 Update)

| Enhancement | Component | Details |
| :--- | :--- | :--- |
| **Standalone Flick.exe** | Windows (`build_exe.py` / PyInstaller) | Zero-dependency 21.8 MB portable executable bundled with multi-resolution `flick.ico`. Eliminates the need for pre-installed Python, pip, or virtual environments on target PCs. |
| **Pystray Win32 Message Pump** | Windows (`flick_receiver.py`) | Restructured event loop: system tray icon runs on the primary thread with background daemons initialized inside the `setup` callback, preventing tray invisibility in subshell and non-interactive sessions. |
| **Single-Instance Win32 Mutex** | Windows (`flick_receiver.py`) | Created `Local\Flick_Windows_Receiver_SingleInstance_Mutex`. Launching `Flick.exe` when an instance is already running immediately brings the QR pairing dialog to the foreground without socket conflicts. |
| **Global Pairing Hotkeys** | Windows (`flick_receiver.py`) | Registered Windows global hotkeys `Win+Shift+F` and `Ctrl+Alt+F` to pop up the QR pairing modal instantly on-screen without searching the taskbar tray. |
| **Interactive Mode B Pairing Prompt** | Windows (`http_receiver.py`) | LAN discovery pair requests trigger an interactive Win32 `MessageBoxW` top-most prompt with a 15-second per-IP cooldown and a single-active-prompt lock to prevent notification fatigue or spam. |
| **Terminal Injection & MOTW Quarantine** | Windows (`http_receiver.py`) | All filenames sanitized against shell metacharacters (`&;^%$()[]`); 37 executable/script extensions quarantined as `.blocked` and omitted from `CF_HDROP`; NTFS `Zone.Identifier` (ZoneId=3) applied to all writes. |
| **Win+V & Cloud Clipboard Shield** | Windows (`clipboard_engine.py`) | Set `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory=0`, and `CanUploadToCloudClipboard=0` to keep teleports private and unlogged. |
| **Tactile Dual-Pulse Haptics** | Android (`HapticHelper.kt`) | Upgraded with `VibrationAttributes.USAGE_NOTIFICATION` to bypass OEM background haptic muting (Nothing OS / Android 16). Delivers a 35ms touch tick on share tap and a crisp 45ms+65ms completion pulse. |
| **Warm Harmonic Chime** | Windows (`http_receiver.py`) | Synthesizes a 450ms dual-sine chime (E5 + G#5) with 25ms soft attack, exponential decay tail, and low volume amplitude (~12% / -18 dB) for relaxing, non-jarring audio feedback. |



