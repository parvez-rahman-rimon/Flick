# 🛠️ Flick — Autonomous Execution & Implementation Plan (`IMPLEMENTATION_PLAN.md`)

**Document Version:** 1.0.1  
**Target Executor:** Autonomous Engineering Agent (Gemini 3.8 Flash / Antigravity)  
**Target Workspace:** `D:\Sandbox\EkSend`

---

## 1. Target Repository Structure

The autonomous coding agent MUST generate the exact directory layout below inside `D:\Sandbox\EkSend`:

```text
D:\Sandbox\EkSend\
├── README.md                           # Master specification index & quick start
├── PRD.md                              # Product Requirements Document
├── ARCHITECTURE.md                     # Protocols, Win32 structs, mTLS v2.0 specification
├── DESIGN.md                           # M3 Expressive tokens & Compose blueprints
├── FEATURES.md                         # MVP vs v2.0 feature matrix & paste behavior
├── IMPLEMENTATION_PLAN.md              # Phased execution guide for autonomous agents
├── HOW_TO_USE.md                       # Comprehensive bilingual user manual
├── flick_security_bundle.md            # Self-contained audit bundle with 13 full source files
├── Flick.exe                           # Standalone 21.8 MB zero-dependency Windows Receiver
│
├── windows-receiver\
│   ├── Flick.exe                       # Compiled standalone binary (PyInstaller)
│   ├── flick.ico                       # Multi-resolution branding icon (16x16 to 256x256)
│   ├── build_exe.py                    # PyInstaller automation script
│   ├── build_exe.bat                   # 1-click batch build wrapper
│   ├── add_to_startup.bat              # Script to pin Flick.exe to shell:startup
│   ├── remove_from_startup.bat         # Script to unpin Flick.exe from startup
│   ├── requirements.txt                # pywin32, Pillow, pystray, qrcode, pyinstaller
│   ├── config_manager.py               # Loads/saves ~/.flick/config.json, supports sys._MEIPASS
│   ├── clipboard_engine.py             # Win32 CF_HDROP, CF_UNICODETEXT, CF_DIB + Win+V Privacy opt-out
│   ├── discovery_beacon.py             # UDP port 45454 JSON broadcaster (every 2.0s)
│   ├── http_receiver.py                # Threaded HTTP server (/ping, /pair, /upload, /clipboard) + safe_filename + MOTW
│   ├── qr_modal.py                     # Tkinter top-most QR code pairing window with flick.ico
│   ├── flick_receiver.py               # Main entrypoint: pystray message pump, Named Mutex, global hotkeys
│   ├── test_receiver_e2e.py            # Automated 7-point E2E self-test
│   ├── start_flick.bat                 # Console launcher
│   └── start_flick_silent.vbs          # Windowless background startup script
│
└── android-app\
    ├── settings.gradle.kts
    ├── build.gradle.kts
    ├── gradle.properties
    ├── gradlew / gradlew.bat
    └── app\
        ├── build.gradle.kts
        └── src\main\
            ├── AndroidManifest.xml             # android:allowBackup="false"
            ├── java\com\eksend\flick\
            │   ├── MainActivity.kt             # M3 Expressive Companion UI Host
            │   ├── TeleportShareActivity.kt    # Headless Theme.NoDisplay Share Target (content:// boundary)
            │   ├── data\
            │   │   ├── Models.kt               # PairedPc, DiscoveredPc, TransferRecord, ClipboardMode
            │   │   └── FlickPreferences.kt     # SharedPreferences JSON storage & state flows
            │   ├── network\
            │   │   ├── UdpDiscoveryListener.kt # UDP 45454 listener with packet-bound effectiveIp
            │   │   ├── PcAutoRouter.kt         # 150ms cached IP ping -> UDP discovery fallback
            │   │   └── FlickHttpClient.kt      # OkHttp chunked binary stream uploader & /pair client
            │   ├── service\
            │   │   ├── UploadService.kt        # ForegroundService (dataSync) streaming content:// URIs
            │   │   └── HapticHelper.kt         # Tactile dual-pulse haptics & Toast feedback engine
            │   └── ui\
            │       ├── theme\
            │       │   ├── Theme.kt            # Dynamic Color (API 31+) + M3 Expressive Fallback
            │       │   └── Type.kt             # M3 Typography scale
            │       ├── components\
            │       │   ├── ScallopedDeviceBadge.kt # 12-lobed M3 Expressive wavy polygon badge
            │       │   ├── DiscoveredPairCard.kt   # Split-action LAN pair request banner
            │       │   └── BentoActionTile.kt      # Rounded 18.dp tonal action cards
            │       └── screens\
            │           ├── HomeScreen.kt           # KDE Connect Future-inspired dashboard
            │           ├── DeviceSettingsScreen.kt # 4-mode clipboard selector & manual IP config
            │           └── PairingScannerScreen.kt # CameraX + MLKit/ZXing QR code scanner
            └── res\
                ├── values\
                │   ├── strings.xml
                │   └── themes.xml              # Theme.Flick & Theme.Flick.TranslucentNoDisplay
                └── drawable\
                    └── ic_flick_bolt.xml       # Vector adaptive & notification icon
```

---

## 2. Phase-by-Phase Execution Roadmap

### Phase 1: Windows Receiver & Win32 Multi-Format Clipboard Engine (`/windows-receiver`)

#### Step 1.1: Dependencies (`windows-receiver/requirements.txt`)
```text
pywin32>=306
Pillow>=10.0.0
pystray>=0.19.5
qrcode>=7.4.2
```
Install and verify in the local Python environment:
```powershell
pip install -r D:\Sandbox\EkSend\windows-receiver\requirements.txt
```

#### Step 1.2: Configuration Manager (`windows-receiver/config_manager.py`)
- Create `~/.flick/config.json` and default download directory `~/Downloads/Flick/` automatically on first run.
- Generate a persistent `pc_id` (`str(uuid.uuid4())`), `pc_name` (`socket.gethostname()`), and a cryptographically random 32-byte hex `auth_token` (`secrets.token_hex(32)`).
- Implement thread-safe `get_config()`, `set_clipboard_mode(mode)`, and `add_paired_device(client_id, client_name)`.
- Implement `get_lan_ip() -> str`:
  ```python
  def get_lan_ip() -> str:
      try:
          with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
              s.settimeout(0.5)
              s.connect(("8.8.8.8", 80))
              return s.getsockname()[0]
      except Exception:
          return socket.gethostbyname(socket.gethostname())
  ```

#### Step 1.3: Win32 Multi-Format Clipboard Injector (`windows-receiver/clipboard_engine.py`)
- Implement `inject_files_to_clipboard(file_paths: list[str], mode: str = "SMART_MULTI") -> list[str]`:
  - Acquire a module-level `threading.Lock()`.
  - Open the Windows clipboard with a 10-attempt retry loop (`time.sleep(0.025)` on `pywintypes.error`) and call `win32clipboard.EmptyClipboard()`.
  - **Format 1 (`CF_HDROP`):** Inject when `mode in ("SMART_MULTI", "FORCE_IMAGE", "FORCE_FILE")`. Pack the 20-byte `DROPFILES` header `struct.pack("<IiiII", 20, 0, 0, 0, 1)` + UTF-16LE null-terminated paths + final `b"\x00\x00"`.
  - **Format 2 (`CF_UNICODETEXT`):** Inject when `mode in ("SMART_MULTI", "FORCE_PATH")`. Join absolute paths with `"\r\n"` and pass to `win32clipboard.SetClipboardData(win32con.CF_UNICODETEXT, path_str)`.
  - **Format 3 (`CF_DIB`):** Inject when `mode in ("SMART_MULTI", "FORCE_IMAGE")`, `len(file_paths) == 1`, file size $\le 32\text{ MB}$, and extension is in `{".png", ".jpg", ".jpeg", ".webp", ".bmp", ".gif"}`. Use `PIL.ImageOps.exif_transpose()`, convert to `RGB`, save as `BMP` to `BytesIO`, strip the first `14` bytes (`bmp_bytes[14:]`), and call `win32clipboard.SetClipboardData(win32con.CF_DIB, dib_bytes)`.
  - Call `win32clipboard.CloseClipboard()` in a `finally` block.
- Implement `inject_text_to_clipboard(text: str) -> list[str]` for `POST /clipboard`.

#### Step 1.4: UDP Discovery Beacon (`windows-receiver/discovery_beacon.py`)
- Run a daemon thread broadcasting the `FLICK_DISCOVERY_V1` JSON packet every `2.0` seconds on UDP port `45454` with `SO_BROADCAST = 1`.
- Broadcast to both `255.255.255.255` and the current subnet broadcast (e.g., `192.168.1.255` derived by replacing the last octet of `get_lan_ip()` with `255`).

#### Step 1.5: Threaded HTTP Streaming Server (`windows-receiver/http_receiver.py`)
- Bind `ThreadingHTTPServer(("0.0.0.0", 45455), FlickRequestHandler)`.
- Implement constant-time token verification via `hmac.compare_digest(provided_token, expected_token)`.
- **`GET /ping`:** Return `200 OK` JSON with `pc_id`, `pc_name`, `authenticated` boolean, and current `clipboard_mode`.
- **`POST /pair`:** Verify QR token (or show a top-most Windows `ctypes.windll.user32.MessageBoxW` confirmation prompt with `MB_YESNO | MB_ICONQUESTION | MB_TOPMOST` when pairing via LAN discovery without QR). Return `auth_token` on approval.
- **`POST /upload`:**
  - Validate `X-Flick-Token`.
  - URL-decode `X-File-Name` (`urllib.parse.unquote`), sanitize via `os.path.basename()`, and resolve filename collisions in `~/Downloads/Flick/` (if `photo.jpg` exists, write to `photo_1730970000123.jpg`).
  - Stream `Content-Length` bytes from `self.rfile` in `65536`-byte chunks directly to a `.part` temporary file, then `os.replace()` to the final path.
  - Determine effective clipboard mode (`X-Clipboard-Mode` header if not `"DEFAULT"`, else `config["clipboard_mode"]`).
  - Support multi-file batches (`X-Batch-Index` / `X-Batch-Total`) by accumulating batch file paths and injecting all of them into `clipboard_engine.inject_files_to_clipboard()`.
  - Optionally play a subtle non-blocking Windows chime (`winsound.MessageBeep(winsound.MB_OK)` if `play_sound` is enabled).
  - Return `200 OK` JSON with `saved_path`, `bytes_written`, `clipboard_formats`, and `elapsed_ms`.
- **`POST /clipboard`:** Validate `X-Flick-Token`, read UTF-8 body, call `inject_text_to_clipboard()`, return `200 OK`.

#### Step 1.6: System Tray & QR Pairing Modal (`windows-receiver/qr_modal.py` & `flick_receiver.py`)
- Generate a programmatic lightning-bolt icon using `PIL.ImageDraw` (`64x64` rounded deep-indigo square `#5B4B9E` with an electric-teal/white bolt).
- Build `pystray.Icon` menu:
  - `Flick Receiver • {ip}:45455` (disabled status label)
  - `Pair New Phone (Show QR)...` (opens Tkinter window with QR code encoding JSON `{"ip": ip, "port": 45455, "pc_id": pc_id, "pc_name": pc_name, "token": auth_token}`)
  - `Clipboard Mode` submenu with radio items:
    - `Smart Multi (File + Path + Image)`
    - `Force Path Only (Terminal / Text)`
    - `Force Image Only (Bitmap + File)`
    - `Force File Drop Only (CF_HDROP)`
  - `Open Flick Downloads Folder` (`os.startfile(save_dir)`)
  - `Quit Flick`

---

### Phase 2: Android Project Scaffolding (`/android-app`)

#### Step 2.1: Gradle & SDK Configuration
- Configure `compileSdk = 35`, `minSdk = 26`, `targetSdk = 35`, `jvmTarget = "17"`.
- Use Kotlin `2.0.21` (or `1.9.24` matching local Android Gradle Plugin `8.5.2` / `8.7.2`) and Compose BOM `2024.09.03`.
- Include dependencies in `app/build.gradle.kts`:
  - `androidx.core:core-ktx`
  - `androidx.activity:activity-compose`
  - `androidx.compose.material3:material3`
  - `androidx.compose.material:material-icons-extended`
  - `com.squareup.okhttp3:okhttp:4.12.0`
  - `org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1`
  - CameraX (`camera-core`, `camera-camera2`, `camera-lifecycle`, `camera-view` `1.3.4`) + `com.google.mlkit:barcode-scanning:17.3.0` (or embedded lightweight ZXing `com.journeyapps:zxing-android-embedded:4.3.0` so APK works on 100% of devices even without Google Play Services!). **Architectural Recommendation:** Use `com.google.zxing:core:3.5.3` with a 40-line `ImageAnalysis.Analyzer` on CameraX `ImageProxy` (YUV_420_888 plane[0] luminance bytes)! This gives a pure Jetpack Compose CameraX preview with **zero** Google Play Services dependency and keeps the APK tiny (`< 4 MB`)!

#### Step 2.2: `AndroidManifest.xml` Contract
```xml
<manifest xmlns:android="http://schemas.android.com/apk/res/android">

    <uses-permission android:name="android.permission.INTERNET" />
    <uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" />
    <uses-permission android:name="android.permission.ACCESS_WIFI_STATE" />
    <uses-permission android:name="android.permission.VIBRATE" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE_DATA_SYNC" />
    <uses-permission android:name="android.permission.POST_NOTIFICATIONS" />
    <uses-permission android:name="android.permission.CAMERA" />

    <uses-feature android:name="android.hardware.camera" android:required="false" />

    <application
        android:allowBackup="true"
        android:icon="@drawable/ic_flick_bolt"
        android:label="Flick"
        android:supportsRtl="true"
        android:usesCleartextTraffic="true"
        android:theme="@style/Theme.Flick">

        <!-- 1. M3 Expressive Companion Dashboard -->
        <activity
            android:name=".MainActivity"
            android:exported="true"
            android:theme="@style/Theme.Flick">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>

        <!-- 2. Zero-UI 1-Tap Share Target (<50ms close) -->
        <activity
            android:name=".TeleportShareActivity"
            android:exported="true"
            android:label="Flick to PC ⚡"
            android:theme="@android:style/Theme.NoDisplay"
            android:excludeFromRecents="true"
            android:noHistory="true"
            android:taskAffinity="">
            <intent-filter>
                <action android:name="android.intent.action.SEND" />
                <category android:name="android.intent.category.DEFAULT" />
                <data android:mimeType="*/*" />
            </intent-filter>
            <intent-filter>
                <action android:name="android.intent.action.SEND_MULTIPLE" />
                <category android:name="android.intent.category.DEFAULT" />
                <data android:mimeType="*/*" />
            </intent-filter>
        </activity>

        <!-- 3. Chunked Streaming Foreground Service -->
        <service
            android:name=".service.UploadService"
            android:enabled="true"
            android:exported="false"
            android:foregroundServiceType="dataSync" />
    </application>
</manifest>
```

---

### Phase 3: Android M3 Expressive UI Implementation (`/android-app`)

1. **`Theme.kt`:** Implement `FlickTheme` with `Build.VERSION.SDK_INT >= Build.VERSION_CODES.S` dynamic color check (`dynamicDarkColorScheme` / `dynamicLightColorScheme`) and the fallback `FlickLightColors` / `FlickDarkColors` defined in `DESIGN.md`.
2. **`ScallopedDeviceBadge.kt`:** Implement the 12-lobed wavy M3 Expressive polygon badge using polar coordinate vertex generation (`r = baseR + amp * cos(12 * theta)`) with smooth cubic/quadratic path interpolation.
3. **`DiscoveredPairCard.kt`:** Implement the split-action LAN pairing banner matching `home.png` (`Pair request`, bold PC hostname, key icon + short ID, right-side compact `X` button + tall `✓` accept button).
4. **`HomeScreen.kt`:**
   - Passive UDP listener (`UdpDiscoveryListener`) updates discovered unpaired PCs and refreshes live online/ping status of paired PCs.
   - Displays `DiscoveredPairCard` when an unpaired PC is broadcasting on the LAN.
   - Displays `DeviceStatusHeroSection` for the active paired PC with:
     - `Send` section: **Files** (`ActivityResultContracts.GetMultipleContents("*/*")`) and **Clipboard** (reads primary clipboard or opens a quick input dialog and sends to `POST /clipboard`).
     - `Control & Mode` section: Quick Clipboard Mode indicator tile, Ping/Test connection button, QR Scanner square button, and primary-filled Device Settings square button.
     - Recent Transfers list showing the last transfers with millisecond timing badges (`84 ms`).
   - Bottom floating status pill (`"{N} device(s) paired"`).
5. **`DeviceSettingsScreen.kt`:**
   - Implement the 2x2 selectable Clipboard Mode cards (`Smart Multi`, `Force Path`, `Force Image`, `Force File`) matching `settings.png`.
   - Implement synchronization switches (`Haptic confirmation`, `Auto-update IP via UDP`) and a manual IP/Port/Token dialog for Tailscale or manual setup.
6. **`PairingScannerScreen.kt`:**
   - Request `Manifest.permission.CAMERA` progressively when navigated to.
   - Bind CameraX `Preview` + `ImageAnalysis` with `com.google.zxing.MultiFormatReader` to decode the Windows QR code JSON in `< 100ms`, call `POST /pair` to confirm, save the `PairedPc` in `FlickPreferences`, fire `EFFECT_HEAVY_CLICK`, and navigate back to `HomeScreen`.

---

### Phase 4: Headless Share Target & Streaming Engine (`TeleportShareActivity` + `UploadService`)

1. **`TeleportShareActivity.kt`:**
   - Inside `onCreate(savedInstanceState)`:
     - Immediately extract `intent.action`:
       - If `ACTION_SEND`: check `EXTRA_STREAM` (`Uri?`). If null, check `EXTRA_TEXT` (`String?`).
       - If `ACTION_SEND_MULTIPLE`: extract `ArrayList<Uri>` from `EXTRA_STREAM`.
     - **Critical Android URI Permission Rule:** Because `TeleportShareActivity` calls `finish()` immediately in `< 50ms`, its temporary URI grant would be revoked before `UploadService` finishes reading large files unless propagated! Therefore, pass the URI(s) via `Intent.setClipData(ClipData)` with `Intent.FLAG_GRANT_READ_URI_PERMISSION` to `UploadService` (or call `grantUriPermission(packageName, uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)`).
     - Call `ContextCompat.startForegroundService(this, serviceIntent)`.
     - Call `finish()` and `overridePendingTransition(0, 0)`.
2. **`UploadService.kt`:**
   - Call `startForeground(NOTIFICATION_ID, buildMinimalNotification())` within 5 seconds (Android 14 `ServiceCompat.startForeground` with `FOREGROUND_SERVICE_TYPE_DATA_SYNC`).
   - Resolve target PC IP via `PcAutoRouter.resolveTargetIp(pairedPc)` (`150ms` cached IP `/ping` $\rightarrow$ `2200ms` UDP `45454` fallback).
   - Stream each URI using a custom `okhttp3.RequestBody`:
     ```kotlin
     override fun writeTo(sink: BufferedSink) {
         contentResolver.openInputStream(uri)?.use { input ->
             sink.writeAll(input.source())
         } ?: throw IOException("Cannot open URI stream")
     }
     ```
   - On `200 OK`, record the transfer in `FlickPreferences`, switch to `Dispatchers.Main` to fire `HapticHelper.triggerSuccess(applicationContext)` (`50ms` `EFFECT_CLICK` + `"⚡ Copied to PC Clipboard"` Toast), and call `stopSelf()` when all queued items finish.

---

### Phase 5: End-to-End Verification & Build Protocol

The autonomous agent MUST execute and verify the following steps before concluding:

1. **Automated Windows Receiver & Clipboard Verification (`test_receiver_e2e.py`):**
   - Start `http_receiver` and `discovery_beacon` in background threads.
   - Generate a test PNG image (`test_flick_image.png`, `400x300` RGB) in memory and `POST /upload` with `X-Flick-Token` and `X-Clipboard-Mode: SMART_MULTI`.
   - Assert HTTP `200 OK` response and verify file exists in `~/Downloads/Flick/`.
   - Open `win32clipboard` and assert that **all three formats** (`win32con.CF_HDROP`, `win32con.CF_UNICODETEXT`, and `win32con.CF_DIB`) are simultaneously present on the live Windows Clipboard!
   - Test `FORCE_PATH` mode and assert `CF_UNICODETEXT` is present while `CF_HDROP` and `CF_DIB` are absent.
2. **Android APK Compilation (`assembleDebug`):**
   - Run `./gradlew.bat assembleDebug` inside `D:\Sandbox\EkSend\android-app`.
   - Verify 0 compilation errors and confirm the output APK at `D:\Sandbox\EkSend\android-app\app\build\outputs\apk\debug\app-debug.apk`.
3. **Live Daemon Startup & Device Installation (if ADB device connected):**
   - Check `adb devices`. If a physical phone or emulator is attached, install `app-debug.apk` via `adb install -r`.
   - Start `flick_receiver.py` in the background so the Windows System Tray receiver is live and ready for the user.

---

### Phase 6: v1.0.1 Standalone Binary Packaging & Security Hardening (Completed)

1. **PyInstaller Compilation (`build_exe.py` / `build_exe.bat`):**
   - Package all dependencies (`pywin32`, `Pillow`, `pystray`, `qrcode`, `cryptography`) into a single portable binary `Flick.exe` (21.8 MB).
   - Bundle high-resolution multi-layer icon `flick.ico`.
   - Add `get_resource_path()` to `config_manager.py` to transparently resolve PyInstaller's `sys._MEIPASS` temporary extraction directory.
2. **Adversarial Security Hardening:**
   - **`safe_filename()`**: Strip trailing dots and spaces to prevent Win32 `CreateFileW` denylist evasion; sanitize terminal injection metacharacters `&;^%$()[]`; handle DOS reserved devices (`CON`, `PRN`, `NUL`).
   - **Quarantine**: 37 dangerous script/binary extensions quarantined with `.blocked` and barred from `CF_HDROP`.
   - **NTFS Mark-of-the-Web**: Attach `:Zone.Identifier` stream (`ZoneId=3`) to all written files.
   - **Win+V & Cloud Privacy Opt-out**: Injected clipboard flags `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory=0`, `CanUploadToCloudClipboard=0`.
   - **Single-Instance Mutex & Global Hotkeys**: Windows Named Mutex `Local\Flick_Windows_Receiver_SingleInstance_Mutex`; registered hotkeys `Win+Shift+F` and `Ctrl+Alt+F`.
   - **Android Hardening**: `android:allowBackup="false"`, restricted `TeleportShareActivity` to `content://` and rejected self-authority; pinned UDP `effectiveIp` to packet source address.
3. **End-to-End Test Suite Execution (`test_receiver_e2e.py`):**
   - Verified 7 test vectors passing 100%: GET /ping, Auth ping, UDP beacon reception, SMART_MULTI 3-format injection, FORCE_PATH mode, POST /clipboard text teleport, and 401 unauthorized rejection.

---

### Phase 7: v2.0 Zero-Trust Mutual TLS 1.3 Execution Roadmap (Completed & Verified)

This phase eliminates the static bearer token and secures the communication against sniffers and active LAN attackers on public networks.

1. **Task 7.1 — Windows Receiver ECDSA P-256 Keypair & DPAPI Storage (`crypto_manager.py`):**
   - Implemented ECDSA P-256 keypair and 10-year self-signed X.509 certificate generation with `cryptography`.
   - Private key protected on disk using Windows DPAPI (`win32crypt.CryptProtectData`) at `~/.flick/pc_key.enc`.
   - SPKI SHA-256 fingerprint (`pin`, 64 hex chars) calculated and displayed in tray, startup toast, and QR modal.
2. **Task 7.2 — Android Keystore P-256 Keypair Generation (`KeystoreManager.kt`):**
   - Implemented hardware-backed ECDSA P-256 keypair generation inside `AndroidKeyStore` with `setIsStrongBoxBacked(true)` and automatic fallback to standard hardware TEE.
   - Built custom `FlickX509KeyManager` and `PinningTrustManager` validating server SPKI SHA-256 against the scanned QR pin.
3. **Task 7.3 — Ephemeral QR Code Handshake (`flick://pair` schema):**
   - Upgraded QR code URI: `flick://pair?v=2&ip=<IP>&p=45455&pin=<SPKI_SHA256>&s=<128_BIT_SECRET>&id=<PC_ID>`.
   - Implemented `POST /v2/pair` verifying client proof of possession: `proof = HMAC-SHA256(pair_secret, client_der_cert)`.
   - Static bearer token completely eliminated from network transit.
4. **Task 7.4 — Authenticated Rotating UDP Beacon (`FLICK_DISCOVERY_V2`):**
   - Windows receiver computes rolling authenticator tag: `AuthTag = HMAC-SHA256(pair_secret, floor(epoch / 30) + pc_id)`.
   - Android client verifies tag with $\pm 1$ time-step window (90-second tolerance) before making network calls, rejecting spoofed beacons.
5. **Task 7.5 — End-to-End Test Suite Execution (`test_receiver_e2e.py`):**
   - **10/10 Test Vectors Passing 100%**:
     1. Out-of-band Ephemeral QR Handshake (`POST /v2/pair` with HMAC proof).
     2. Authenticated `GET /ping` over mTLS 1.3 (`authenticated=True`, `version=2.0.0`).
     3. Rolling UDP Beacon V2 reception and `AuthTag` verification.
     4. `POST /upload` over mTLS 1.3 (`SMART_MULTI`: `CF_HDROP` + `CF_UNICODETEXT` + `CF_DIB`).
     5. `POST /upload` over mTLS 1.3 (`FORCE_PATH`: plain path text only).
     6. `POST /clipboard` plain text teleport over mTLS 1.3.
     7. Adversarial test: Client without certificate rejected by TLS handshake (`PEER_DID_NOT_RETURN_A_CERTIFICATE`).
     8. Adversarial test: Unpaired client certificate rejected with HTTP 401 Unauthorized.
     9. Adversarial test: Invalid HMAC proof rejected on `/v2/pair` with HTTP 403 Forbidden.
     10. Double-broadcast directed subnet & 255.255.255.255 compatibility.
6. **Task 7.6 — Binary Recompilation:**
   - Standalone Windows binary `Flick.exe` (25.76 MB) compiled with PyInstaller, bundling DPAPI and cryptography modules.
   - Android APK `app-debug.apk` (18.6 MB) compiled with `./gradlew.bat assembleDebug` (0 compilation errors).


