# ⚡ Flick — Product Requirements Document (`PRD.md`)

**Document Version:** 1.0.0  
**Status:** Approved for Autonomous Engineering Execution  
**Target Workspace:** `D:\Sandbox\EkSend`  
**Platforms:** Android 8.0+ (API 26–35, Kotlin + Jetpack Compose M3 Expressive) & Windows 10/11 (Python 3.11+ Win32 Daemon)

---

## 1. Executive Summary & Problem Statement

### 1.1 The Friction Problem
Developers, designers, and power users frequently capture photos, screenshots, videos, or files on their Android phone and need them immediately inside a Windows desktop workflow—such as pasting an image into an AI coding assistant (Antigravity, Cursor, Claude), attaching a screenshot to a GitHub PR, or pasting a local file path into a terminal.

Existing cross-device transfer utilities fail the **"Zero-Step Reflex Test"**:
1. **Google Quick Share / Nearby Share:** Tapping Share opens a modal bottom sheet, scans for 2–5 seconds, requires tapping the target PC, requires accepting or waiting for a Windows notification, saves the file to `Downloads`, and still forces the user to open File Explorer to copy either the image or its file path.
2. **LocalSend:** Requires opening the LocalSend UI on both devices or picking the target device inside the LocalSend Activity after sharing, plus manual file retrieval on PC.
3. **KDE Connect:** While it supports Direct Share targets, Android's `ChooserTargetService` / ShortcutManager often delays rendering Direct Share icons by 1–2 seconds or pushes them off the primary row. Furthermore, KDE Connect's Android UI suffers from legacy design debt, upfront permission walls, and lacks automatic **multi-format Windows clipboard injection** (`CF_HDROP` + `CF_UNICODETEXT` + `CF_DIB` simultaneously).
4. **Cloud Messengers (Telegram / WhatsApp / Discord Self-Chat):** Re-compresses media, routes private data through external cloud servers, requires internet connectivity, and still requires manual right-click copy on the PC.

### 1.2 The "Flick" Solution
**Flick** is a LAN-first, zero-cloud, 1-tap file & clipboard teleporter coupled with a **Material 3 Expressive** companion app inspired by tintotint's *"KDE Connect of the Future"* redesign:
- **Zero-UI Share Target (`TeleportShareActivity`):** When the user selects a photo, video, or file in any Android app and taps **Flick** in the Android Share Sheet, **no UI window opens**. The invisible `Theme.NoDisplay` activity hands the `content://` stream to a background engine and closes in `< 50ms`.
- **Sub-Second LAN Streaming:** Streams raw bytes directly to the paired Windows PC over Wi-Fi / LAN / Tailscale.
- **Physical Haptic Confirmation:** The exact millisecond the Windows PC confirms receipt and clipboard injection (`HTTP 200 OK`), the phone fires a crisp `50ms` tactile click (`VibrationEffect.EFFECT_CLICK`).
- **Simultaneous Multi-Format Windows Clipboard Injection:** Without touching the mouse on Windows, the user immediately presses `Ctrl + V`:
  - Inside **Terminal / PowerShell / Code Editor** $\rightarrow$ Pastes the exact absolute Windows file path (`C:\Users\<User>\Downloads\Flick\IMG_20261107.jpg`).
  - Inside **Antigravity Chat / Browser / Discord / Slack / File Explorer** $\rightarrow$ Pastes/attaches the actual file or photo (`CF_HDROP`).
  - Inside **Paint / Figma / Image Editors** $\rightarrow$ Pastes the raw uncompressed bitmap pixels (`CF_DIB`).

---

## 2. User Personas & Core Workflows

### 2.1 Primary Persona: The AI-Native Developer & Creator
- **Environment:** Windows 10/11 workstation + Android smartphone on the same Wi-Fi LAN (or connected via mobile hotspot / Tailscale).
- **Daily Habit:** Takes whiteboard photos, mobile UI bug screenshots, or screen recordings and wants to press `Ctrl + V` inside their IDE or terminal within 1 second of tapping Share on their phone.

### 2.2 Core User Journeys

#### Journey A: The Hero 1-Tap Share Flow (Daily 95% Use Case)
1. User selects 1 (or multiple) photos/videos/files in Android Gallery or File Manager and taps **Share**.
2. User taps the **Flick** app icon in the main Share Sheet (pinned for instant thumb reach).
3. Flick's `TeleportShareActivity` (`Theme.NoDisplay`) intercepts the intent, extracts the URI(s), grants URI read permissions to `UploadService`, and calls `finish()` in `< 50ms`. The user stays right in their Gallery app.
4. `UploadService` pings the cached PC IP (`150ms` timeout) or resolves the live UDP beacon, streams the raw binary payload to `POST /upload`, receives `200 OK`, fires a `50ms` haptic click + brief toast (`"⚡ Copied to PC"`), and terminates.
5. User presses `Ctrl + V` on Windows. Done.

#### Journey B: First-Time Frictionless Pairing (1-Time Setup)
1. User starts the Windows Tray Receiver (`flick_receiver.py`).
2. User right-clicks the Windows System Tray icon $\rightarrow$ clicks **"Pair New Phone (Show QR)"**.
3. A clean popup window displays a QR code encoding `{"ip": "192.168.1.100", "port": 45455, "token": "<32-byte-hex>", "name": "DESKTOP-PC", "id": "<uuid>"}`.
4. User opens the **Flick** Android app $\rightarrow$ taps **"Pair PC via QR"** $\rightarrow$ grants Camera permission progressively $\rightarrow$ scans the QR code (or taps **"Accept"** on an auto-discovered LAN PC card on the Home Dashboard).
5. The PC and phone are permanently paired. Even if the PC's DHCP IP address changes tomorrow, the phone automatically updates the IP via the authenticated UDP discovery beacon.

#### Journey C: Companion App & Clipboard Mode Switching
1. User opens the **Flick** Android app to view the M3 Expressive **Home Dashboard** (inspired by KDE Connect's future mockup: scalloped M3 badge, live connection status, RTT ping latency, and quick action Bento tiles).
2. User can tap **"Send Files"** to pick files from inside the app, tap **"Send Clipboard"** to push current phone text to PC, or open **Device Settings** to override the default PC clipboard injection mode (`SMART_MULTI`, `FORCE_IMAGE`, `FORCE_PATH`, `FORCE_FILE`).

---

## 3. Scope Demarcation: Phase 1 (MVP) vs. Phase 2 (Roadmap)

| Capability | Phase 1 (v1.0 MVP — Current Build) | Phase 2 (v2.0 Future Roadmap) |
| :--- | :--- | :--- |
| **Headless 1-Tap Share Target (`ACTION_SEND` / `ACTION_SEND_MULTIPLE`)** | ✅ Included (`Theme.NoDisplay`, `<50ms` close) | ✅ Enhanced with Direct Share shortcuts |
| **Windows System Tray Receiver & UDP Beacon** | ✅ Included (Port `45454` UDP, `45455` HTTP) | ✅ mDNS/Zeroconf + Windows Service installer |
| **Win32 Multi-Format Clipboard Engine** | ✅ Included (`CF_HDROP`, `CF_UNICODETEXT`, `CF_DIB`) | ✅ Clipboard history ring-buffer on Windows |
| **Clipboard Mode Override (Tray + Android Header)** | ✅ Included (`SMART_MULTI`, `FORCE_PATH`, `FORCE_IMAGE`, `FORCE_FILE`) | ✅ Per-application smart rules on Windows |
| **M3 Expressive Companion UI (Dynamic Color)** | ✅ Included (Home Dashboard, Device Card, Settings, QR Pairing) | ✅ Tablet/Foldable 2-pane adaptive layout |
| **In-App Quick Actions (`Files` picker & `Clipboard` push)** | ✅ Included on Device Card (`ActivityResultContracts.GetMultipleContents`) | ✅ Full in-app media gallery & folder browser |
| **Transfer History Ledger** | ⚪ In-memory recent status on Home Card | 🔮 Persistent Room DB history with thumbnails & re-send |
| **Bidirectional Transfer (PC $\rightarrow$ Android)** | ⚪ Out of scope for v1.0 | 🔮 Drag-and-drop onto Windows Tray icon to send to Phone |
| **Remote Control Plugins (Touchpad, Media, Terminal)** | ⚪ Visual roadmap placeholders / disabled cards | 🔮 Full KDE Connect-style modular plugin architecture |

---

## 4. Strict Non-Functional Requirements & Success Metrics

### 4.1 Performance & Latency Budgets
- **Share Sheet Dismissal Time:** `< 50ms` from tap to `TeleportShareActivity.finish()`. Zero visible window animation or black flash.
- **Fast-Path Connection Resolution:** `< 150ms` TCP connect timeout to the last-known cached IP before falling back to the passive UDP beacon cache.
- **End-to-End Transfer Latency (5 MB Photo on 5GHz Wi-Fi):** `< 450ms` total from Share Sheet tap to Windows Clipboard availability and Android haptic click.
- **Streaming Memory Footprint (Android & Windows):** `O(1)` memory complexity relative to file size. Must use `64 KB` (`65536` bytes) chunked streaming buffers so a `2 GB` 4K video transfer never exceeds `35 MB` RAM on either device.
- **Android Idle RAM Footprint:** `0 MB` when not actively transferring. `UploadService` must call `stopSelf()` immediately after the queue drains.

### 4.2 Reliability & Edge-Case Constraints
- **DHCP IP Drift:** If the router assigns the PC a new local IP address, the Android client must transparently discover the new IP via the UDP broadcast beacon (`port 45454`) matched by `pc_id` without requiring re-pairing.
- **Filename Collision Safety:** Saving `image.png` when `image.png` already exists in `~/Downloads/Flick/` must never overwrite user data; it must atomically resolve to `image_1730970000123.png` (or sequential `image (1).png`).
- **Win32 Clipboard Lock Contention:** Other Windows apps (e.g., Clipboard Managers, RDP, Electron apps) frequently lock the clipboard for `5–50ms`. The Windows receiver must implement an exponential backoff retry loop (`10` attempts over `250ms`) around `win32clipboard.OpenClipboard()`.

### 4.3 Security & Privacy Boundaries
- **Zero Cloud / Zero Telemetry:** 100% of discovery and transfer traffic stays on the local subnet (`192.168.x.x`, `10.x.x.x`, `172.16.x.x`, or `100.x.x.x` Tailscale). No external internet requests are ever made.
- **Token-Authenticated Uploads:** Every `POST /upload` request must carry `X-Flick-Token: <shared_secret>`. Requests missing or failing constant-time token comparison (`hmac.compare_digest`) are rejected immediately with `HTTP 401 Unauthorized` before reading the request body.
- **Path Traversal Prevention:** Incoming `X-File-Name` headers must be sanitized via `os.path.basename()` and stripped of illegal Windows characters (`<>:"/\|?*` and control bytes `0x00–0x1F`) so a malicious client can never write outside `~/Downloads/Flick/`.
- **Progressive Permissions (Android):**
  - `INTERNET` and `VIBRATE`: Normal permissions (granted at install).
  - `FOREGROUND_SERVICE` and `FOREGROUND_SERVICE_DATA_SYNC`: Required for Android 14+ background streaming reliability.
  - `CAMERA`: Requested **only** when the user taps "Scan QR Code".
  - `POST_NOTIFICATIONS` (Android 13+): Requested **only** after pairing or when starting a transfer, with graceful degradation if denied (transfer and Toast/Haptics still work).

