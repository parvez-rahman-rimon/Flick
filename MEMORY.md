# 🧠 Flick — Engineering Memory & System Ledger (`MEMORY.md`)

**Last Updated:** 2026-11-19T20:25:00+06:00  
**Active Project:** Flick — Frictionless File Transfer (Local High-Speed Companion Ecosystem)  
**Root Path:** `D:\Sandbox\EkSend`  
**Git Branch:** `feature/flick-turbo-link`  
**Target Hardware:** 
- **PC:** `PARVEZRAHMAN` (Windows 11, Qualcomm QCA9377 802.11ac Wi-Fi, IP: `192.168.0.197`, Ports: TCP `45455`, UDP `45454`, Local IPC `45459`, Probe: `45457`)
- **Android Device:** Nothing A001 / CMF Phone wirelessly connected via ADB at `192.168.0.199:43947` (or `5555`)
- **Network Environment:** Tenda AC5 AC1200 Dual-Band Router (5GHz Wi-Fi, Single-Core MIPS CPU, 100 Mbps Fast Ethernet ports) + Offline Portable Hotspot Direct (`ap0`/`swlan0`)

---

## 1. Project Identity & Architecture Overview

Flick is a zero-latency, local-network PC-to-Android companion suite designed to bypass cloud latency and telemetry. It provides instant bi-directional file streaming, clipboard beaming, and automated Windows clipboard injection directly into the OS workflow (supporting `CF_HDROP` Explorer file drops, `CF_UNICODETEXT`, and `CF_DIB` image bitmaps).

### Brand & Interaction Identity
- **Product Title:** FLICK — Frictionless File Transfer
- **Revised Edition Hotkey Elimination:** All Win32 global keyboard shortcuts (e.g. `Win+Shift+F`, `Ctrl+Alt+F`) have been permanently removed. The Windows receiver runs with zero OS hotkey registration to eliminate OS/IDE hotkey conflicts and maintain a lightweight, non-intrusive background presence. Pairing is performed via System Tray or zero-config UDP discovery.

### System Topology
- **Android Client (`android-app`):** Kotlin + Jetpack Compose edge-to-edge UI, OkHttp HTTP/1.1 chunked streaming (512KB buffer, direct `ParcelFileDescriptor` streaming, low-latency `WifiLock`), UDP discovery listener, hardware-haptic feedback, and Spatial Flick Deck gesture canvas.
- **Windows Receiver (`windows-receiver-rust`):** High-performance, pure Rust native PE64 daemon (~2.01 MB static binary, ~3.4 MB private working set RAM). Zero Python runtime dependency. Implements pure-Rust TLS 1.3 engine with `rustls` (ring), UDP presence beacon (`FLICK_DISCOVERY_V2`), 2MB buffered disk writer, serialized Win32 multi-format clipboard worker (`CF_HDROP`, `CF_UNICODETEXT`, `CF_DIB`), native tray icon with silent toasts (`NIIF_NOSOUND`), CRT translucent progress HUD (10 FPS throttled with `AtomicBool` coalescing), and Teenage Engineering GDI dashboard. Deployed binary at `D:\Sandbox\EkSend\Flick.exe`.

---

## 2. Design System: Authentic Teenage Engineering (TE TX-1 / RX-1)

Flick adheres to a strict **Teenage Engineering Hardware Aesthetic** modeled after physical field instruments (OP-1, TP-7, EP-133, and OB-4). All generic "AI slop" (floating purple cards, checklist circles, flat monochromatic bento boxes, cheap emojis) is strictly forbidden.

### Core Visual Tokens
- **Chassis Background:** `#0A0B0E` (Deep machined obsidian)
- **Panel Surface:** `#131519` (Layered gunmetal)
- **Recessed Telemetry Wells:** `#1A1D23`
- **Machined Seams:** `#22252C`
- **Signature Accent:** `#FF4400` (International Safety Orange)
- **Keycap Gradients:** `#FF5500` → `#E63800` (3D plunger with shadow lip)
- **No Emojis:** Zero `❌` or `✅` emojis in toasts, notifications, or HUD. Use Swiss technical typography (`FontWeight.Bold / Black`, monospace telemetry).

---

## 3. Milestones & Recent Engineering Fixes (Nov 19, 2026)

1. **Spatial Flick Deck Canvas (`SpatialFlickDeck.kt`):**
   - 3D perspective flight trajectory when flicking cards.
   - High-definition video first-frame and PDF page-1 thumbnail extraction.
   - Single item badge hidden when count is 1.
   - Snappy flick gesture detection: upward velocity `< -300 px/s` or displacement `< -35f` for effortless light flicks ("olpo swipe").
2. **Windows Rust Stack Overflow Crash Fix (`http_server.rs`):**
   - Eliminated `0xc00000fd` crash caused by 256KB array allocated on a 256KB thread stack.
   - Streaming buffer moved safely to heap (`vec![0u8; CHUNK_SIZE]`).
   - Connection thread stack expanded to 2 MB.
3. **Throughput Pipeline Saturation:**
   - Android: Removed in-loop `sink.flush()`, 512KB buffer, direct kernel `AutoCloseInputStream`, throttled UI notifications to 100ms.
   - Android: Added `WifiManager.WIFI_MODE_FULL_LOW_LATENCY` (`WifiLock`) and `WakeLock` to prevent 802.11 power-saving radio sleep.
   - Windows: Added `BufWriter::with_capacity(2 * 1024 * 1024)` to decouple network stream from disk I/O.
4. **Translucent Desktop Progress HUD (`progress_hud.rs`):**
   - Threshold lowered to 2.5s (visible within 3s).
   - Dismisses automatically at $\ge 90\%$ before spatial card drop.
   - 10 FPS rate-limiting + `AtomicBool` message coalescing prevents Win32 message queue lockups.
5. **Session History 120 FPS Smooth Scrolling (`HomeScreen.kt`):**
   - In-memory `ThumbnailCache` (`LruCache`) eliminates disk reads during scroll.
   - Viewfinder thumbnails downsampled to 320px with `RGB_565` (~70KB vs 8.3MB full 1080p).
   - Stable item keys added to `LazyColumn(items)`.

---

## 4. Hardware Network Benchmark & Physical Limitations

### Empirical Benchmark Findings (Tenda AC5 5GHz Wi-Fi):
- **Raw Socket Memory-to-Memory Benchmark (No Flick, No TLS, No Disk):**
  `50.0 MB in 6.14s = 8.14 MB/s (65.2 Mbps)`
- **Flick Actual Transfer (TLS 1.3 + Disk Write):**
  `95.0 MB in 10.7s = 8.87 MB/s (71.0 Mbps)`
- **Physical Diagnosis:**
  - Phone is on 5GHz Wi-Fi (Tx link: ~200-292 Mbps, RSSI: -61 dBm).
  - PC is on 5GHz Wi-Fi (Qualcomm QCA9377 1x1, link: 433.3 Mbps).
  - Wi-Fi to Wi-Fi traffic on the same 5GHz band consumes **double airtime** (half-duplex contention: Phone $\rightarrow$ Router, then Router $\rightarrow$ PC).
  - Therefore, the home router physically caps LAN Wi-Fi to Wi-Fi at $\approx 8.5 - 9.0\text{ MB/s}$. Flick is already extracting 100% of available router throughput.

---

## 5. Implemented Milestone: "Flick Turbo Link" (P2P Direct Wi-Fi) & Offline Direct Mode

For payloads $\ge 150\text{ MB}$ (150 MB – 4 GB+), Flick autonomously bypasses the router to achieve **~36.5+ MB/s (4.5x speedup)** over full TLS 1.3 + disk write:
- **Architecture Specification:** [FLICK_TURBO_DIRECT_LINK_RFC.md](file:///C:/Users/Parvez/.gemini/antigravity/brain/9ad8d5ed-42ac-4346-beff-9c146682ce64/FLICK_TURBO_DIRECT_LINK_RFC.md)
- **Threshold Analysis:** Mathematical break-even analysis proved 50MB is slower over Turbo due to ~7.25s radio switch overhead (5.9s on LAN vs 9.4s on Turbo). At 150MB, Turbo takes 11.3s vs LAN's 18.5s, delivering true user-perceived speedup.
- **Implemented Modules:**
  - **Windows Rust (`windows-receiver-rust/src/turbo_wlan.rs`):** `wlanapi.dll` profile injection & connection (`WlanSetProfile` / `WlanConnect` / `WlanDeleteProfile`), `iphlpapi.dll` `GetAdaptersAddresses` L3 IPv4 lease detector, `FLICK_TURBO_READY` UDP beacon on port `45456`, RAII `TurboUploadGuard` + self-healing watchdog, and deterministic restoration to the original Wi-Fi profile (`Reemo`).
  - **Windows Core (`config.rs`, `beacon.rs`):** `get_lan_ip()` includes `GetAdaptersAddresses` fallback for gateway-less networks; `beacon.rs` broadcasts every `1.0s` per active adapter subnet and responds instantaneously on UDP `:45457` (`FLICK_PROBE`) for `< 50 ms` Offline / Power-Cut Hotspot discovery.
  - **Android Offline / Power-Cut Direct Mode (`PcAutoRouter.kt`, `UploadService.kt`, `HomeScreen.kt`):** Option 2 implementation. Detects direct Portable Hotspot subnets (`ap0` / `swlan0` / AP-mode `wlan0`), bypasses `LocalOnlyHotspot` when already on Hotspot so all file sizes stream immediately at 36+ MB/s without router/internet, auto-recovers offline links every 3s, and displays `LINK: HOTSPOT P2P // DIRECT` + `HOTSPOT` badge.
  - **Windows HTTP & HUD (`http_server.rs`, `progress_hud.rs`):** Authenticated `POST /turbo/initiate` & `POST /turbo/teardown` endpoints; `⚡ FLICK // TURBO 5G` HUD header during Turbo streams.
  - **Android Client (`TurboLinkController.kt`, `UploadService.kt`, `FlickHttpClient.kt`, `HomeScreen.kt`, `DeviceSettingsScreen.kt`):** `LocalOnlyHotspot` orchestration (`CHANGE_WIFI_STATE` + `NEARBY_WIFI_DEVICES`), `3.65s` Windows pre-switch settle to align cleanly after Android `hostapd` 5GHz CSA (`2437 → 5745 MHz` Ch 149 80 MHz), automatic LAN recovery ping fallback, `TURBO 5G` CRT badge, and Calibration settings toggle.

### Field Edge-Cases Diagnosed & Resolved:
1. **Windows ARP / NLA Settling Window (Premature Teardown):**
   - *Issue:* Windows associating with Android ad-hoc hotspot takes 200–500ms for L3 ARP/NLA to settle. Single 450ms ping timeout failed prematurely.
   - *Fix:* Added 3-phase mTLS warm-up ping in `TurboLinkController.kt` (950ms connect timeout, 1100ms read timeout, 150ms inter-attempt delay) before dispatching the payload stream.
2. **Asymmetric Router Reconnection Time (Premature LAN Fallback):**
   - *Issue:* When restoring to the home router (`Reemo`), Windows Native Wi-Fi re-association + DHCP lease renewal takes ~7–10 seconds. Android's fallback recovery loop gave up after 5 seconds, attempting LAN upload while the PC was still offline, displaying "restoring home router then tx 0 then failed".
   - *Fix:* Extended LAN recovery ping loop to 12 seconds with adaptive 400ms polling in `UploadService.kt`.

### Empirical Live Benchmarks (TLS 1.3 + Disk Write):
- **Standard Router LAN (`Reemo` 5GHz):** `51.79 s` for 420 MiB = **`8.11 MB/s` (`68.0 Mbps`)**
- **420 MiB Payload via Turbo (`AndroidShare_6446`, 5GHz Ch 149):** `11.50 s` = **`36.52 MB/s` (`306.4 Mbps`)** (`4.50x` speedup)
- **244 MiB Payload via Turbo (`256,128,312` bytes):** `6.965 s` = **`35.03 MiB/s` (`36.73 MB/s`, `293.8 Mbps`)**
- **Zero-Dropout Restoration:** Automatically torn down via `/turbo/teardown` + `reservation.close()` + `WlanConnect("Reemo")` + `WlanDeleteProfile`, returning both devices to `Reemo` 5GHz in `< 400 ms` with zero leftover profiles.

---

## 6. Active Binaries & Deployment Commands

- **Android APK:** `D:\Sandbox\EkSend\Flick-v2.3.0-turbo-debug.apk`
- **Windows Rust Binary:** `D:\Sandbox\EkSend\Flick.exe` (v2.0.1 Release + Turbo Link)

### Deployment Commands:
```powershell
# Deploy Android APK Wirelessly (Check dynamic port if 5555 fails):
adb connect 192.168.0.199:5555
adb connect 192.168.0.199:43947
adb -s 192.168.0.199:43947 install -r "D:\Sandbox\EkSend\Flick-v2.3.0-turbo-debug.apk"
adb -s 192.168.0.199:43947 shell am start -n com.eksend.flick/.MainActivity

# Build & Run Windows Rust Receiver:
cd D:\Sandbox\EkSend\windows-receiver-rust
cargo build --release
Stop-Process -Name Flick -Force -ErrorAction SilentlyContinue
Copy-Item "target\release\flick.exe" "D:\Sandbox\EkSend\Flick.exe" -Force
Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{CommandLine = 'D:\Sandbox\EkSend\Flick.exe'}
```
