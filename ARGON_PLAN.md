Yes — **100% doable, zero cloud cost, and we can build the complete APK + Windows listener right here.**

You can even beat existing tools like **KDE Connect** or **LocalSend** (which still often ask you to pick a device unless you use a Direct Share shortcut) by building a **headless, zero-UI Android Share Target** paired with a **lightweight Windows background daemon**.

---

### 1. How It Can Be Solved (Two Paths)

#### Option A: Existing Zero-Code Workaround (KDE Connect Direct Share)
If you want to try an existing app first: **KDE Connect** (free, open-source) registers **Direct Share targets** in Android's share sheet (the top row of icons with your PC's actual name, e.g., *"Send to Parvez-PC"*). Tapping that specific icon bypasses the device picker and sends the file immediately. You can pair that with a tiny 20-line PowerShell/Python folder watcher on Windows to auto-copy the file/image to your clipboard.
*Downside:* Android sometimes delays loading Direct Share targets by 1–2 seconds, or pushes them off the top row.

#### Option B: Custom "One-Tap Teleport" APK + PC Daemon (Fastest & Zero Friction)
We build a custom, ultra-lightweight Android app (**~2 MB**) whose **only job** is to sit in the main Share Sheet app list. Because your PC is hardcoded/pre-paired as the sole target, tapping the app icon doesn't open a screen—it immediately fires the file over LAN in the background, vibrates once (`50ms`), shows a `"Sent to PC ⚡"` toast, and closes in `<100ms`.

---

### 2. Concrete Architecture ("SnapDrop-Zero" / "Teleport")

```
[Android Share Sheet]
        │ (1. Tap "Send to PC" icon — Theme.NoDisplay Activity)
        ▼
[Android Foreground Service / Coroutine]
        │ (2. Streams raw URI bytes over LAN Wi-Fi / Tailscale TCP socket)
        ▼
[Windows Tray Daemon (Python / Go / Rust)]
        ├─► Saves file to C:\Users\<You>\Downloads\Teleport\
        └─► Injects into Windows Clipboard (Win32 API):
              • CF_HDROP (File Drop List) ──► Paste into IDE / Explorer / Slack / Browser upload!
              • CF_DIB (Bitmap Image)     ──► Paste raw image into Docs / Figma / Chat!
              • CF_UNICODETEXT (File Path)──► Paste path string into Terminal / Code editor!
```

#### Why the Windows Clipboard Magic Works So Well:
Windows Clipboard supports **multiple formats simultaneously** on a single clipboard item (`OpenClipboard` -> `SetClipboardData` for multiple formats before `CloseClipboard`):
1. **`CF_HDROP` (File Object)**: If you press `Ctrl+V` inside **VS Code, Cursor, Antigravity, Chrome, Discord, or File Explorer**, Windows pastes the actual file or uploads the image!
2. **`CF_UNICODETEXT` (File Path)**: If you press `Ctrl+V` inside a **terminal, text prompt, or code editor**, it pastes `"C:\Users\...\photo.jpg"`.
3. **`CF_DIB` (Raw Bitmap - Optional mode)**: For apps that only accept raw pixel data (like Paint or certain web canvases), it pastes the uncompressed bitmap.

---

### 3. Brief Implementation Plan

#### Part 1: Windows Receiver Daemon (`pc_receiver.py` or `.exe`)
- **Discovery**: Broadcasts a tiny UDP beacon (`_teleport._udp.local` via mDNS/Zeroconf or UDP broadcast on port `45454`) so even if your router changes your PC's local IP, the phone always knows where the PC is instantly.
- **Fast HTTP/TCP Receiver**: Listens on port `45455`. Streams incoming multipart/raw binary payloads directly to disk (`~/Downloads/PhoneDrops/`).
- **Multi-Format Win32 Clipboard Injector**: Uses `pywin32` (`win32clipboard`) to place **both** the file drop handle (`CF_HDROP`), bitmap data (if image), and text path (`CF_UNICODETEXT`) onto your clipboard simultaneously, plus triggers a subtle Windows notification sound.

#### Part 2: Android Zero-UI APK (Kotlin)
- **`AndroidManifest.xml`**:
  - Registers `ShareReceiverActivity` with `android:theme="@android:style/Theme.NoDisplay"` (so no window ever pops up over your gallery!).
  - Intent filters for `android.intent.action.SEND` and `android.intent.action.SEND_MULTIPLE` with `mimeType="*/*"`.
- **`ShareReceiverActivity.kt`**:
  - Extracts `Intent.EXTRA_STREAM` URI(s).
  - Hands the URI directly to a fast background `WorkManager` / Foreground Service (or coroutine with `goAsync`/short service) and calls `finish()` immediately.
- **Network Engine (`OkHttp` / Raw Socket)**:
  - Caches your PC's last known IP (attempts cached IP first with a `150ms` connect timeout; if IP changed, listens for the UDP beacon).
  - Streams `contentResolver.openInputStream(uri)` straight to the PC at full Wi-Fi speed (50–100+ MB/s on 5GHz Wi-Fi).
  - Triggers a tiny haptic tick (`VibrationEffect.createOneShot`) when the PC replies `200 OK - Copied to Clipboard`.

---

### 4. Ready-to-Use Master Prompt
Here is the exact, structured prompt you can use (either right here with me, or in any session) to build the entire system end-to-end:

```markdown
Build a complete, zero-UI, 1-click Android-to-Windows file & clipboard teleporter ("PC Teleport") consisting of two parts:

1. **Windows Background Receiver & Multi-Format Clipboard Injector (Python)**:
   - Listen on TCP port 45455 (HTTP POST `/upload`) and broadcast a UDP discovery beacon on port 45454 every 2 seconds so the Android app auto-discovers the PC's LAN IP without manual setup.
   - Save incoming files to `~/Downloads/Teleport/` with collision-safe filenames.
   - Immediately populate the Windows Clipboard using `pywin32` (`win32clipboard`) in a SINGLE clipboard session with MULTIPLE formats simultaneously:
     a) `CF_HDROP` so pressing Ctrl+V in browsers, IDEs, Discord, or File Explorer pastes/attaches the actual file or photo.
     b) `CF_UNICODETEXT` containing the absolute Windows file path so pressing Ctrl+V in terminals or text inputs pastes the file path.
     c) Optional toggle/flag: if the file is an image (`.png`, `.jpg`, `.webp`), also populate `CF_DIB` so image-only editors receive raw pixels.
   - Provide a `requirements.txt` and a `.bat` / `.vbs` startup script to run it silently in the background on Windows boot.

2. **Android "Zero-UI" Share Target App (Kotlin + Gradle)**:
   - A lightweight Android project that compiles into a release/debug APK via `./gradlew assembleDebug`.
   - Main Activity (`MainActivity`) only used once to show current paired PC IP, test connection, and choose clipboard mode (File+Path vs Raw Bitmap).
   - Share Activity (`TeleportShareActivity`) configured with `android:theme="@android:style/Theme.NoDisplay"` and `excludeFromRecents="true"`.
   - Intent filters for `ACTION_SEND` and `ACTION_SEND_MULTIPLE` (`*/*` — photos, videos, any file).
   - When tapped in the Android Share Sheet:
     - Do NOT open any UI. Immediately start a short-lived Foreground Service (`DataSync`) to stream the `content://` URI InputStream over HTTP to the cached PC IP (auto-falling back to UDP discovery if the IP changed) and call `finish()`.
     - Trigger a crisp haptic vibration (`EFFECT_CLICK`) and a short Toast ("Copied to PC Clipboard ⚡") as soon as the upload finishes.
```

---

### Want me to build it right now?
Since we have Android SDK / Gradle tools available in your environment, I can:
1. Write and start the **Windows Receiver script** on your PC right now, and
2. Scaffold and compile the **actual `.apk` file** (and even install it directly to your phone via `adb install` if you plug it in or use Wireless Debugging).

Let me know if you want me to go ahead and build both!