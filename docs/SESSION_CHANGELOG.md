# Flick Mobile (TX-1) & Desktop (RX-1) Update Log

**Date:** 2026-11-20  
**Target:** Android App (`android-app`) & Desktop Receiver (`windows-receiver-rust`)

---

## 1. Multi-PC Horizontal Swipe Switcher (TX-1 CRT Telemetry Deck)
- **Fluid Sideways Paging:** When multiple PCs (e.g. `PARVEZRAHMAN`, `SAIF`, etc.) are paired, users can swipe horizontally across the CRT Telemetry Deck.
- **Unobtrusive Teenage Engineering Aesthetic:** No chunky buttons or forced controls. Added minimal tactile indicator dots (8dp active `#FF4400`, 5dp inactive `#333742`) right below the telemetry card.
- **Instant Haptic Switching:** Switching cards fires a crisp haptic solenoid tick (`HapticHelper.triggerQuickTick`) and updates the active target PC immediately in `FlickPreferences`.
- **Single-Card Graceful Fallback:** If only 1 PC is paired, the horizontal pager seamlessly collapses into a static card without dots.

---

## 2. RF Link Detection & Zero-Trust Pairing Fixes
- **Suppressed Duplicate Banners:** Already-paired PCs no longer trigger annoying `RF LINK DETECTED // 45454-UDP` banners upon opening the app.
- **Silent Background DHCP/IP Sync:** UDP beacons from paired devices now silently synchronize and update `lastKnownIp` in the background without UI interruption.
- **Fixed `unexpected end of stream` Exception:** 
  - *Cause:* The `[✓]` button previously triggered plain HTTP `requestLanPair()` against port 45455, which only accepts TLS 1.3 / HTTPS.
  - *Fix:* Truly new PCs now route directly to `onNavigateToScanner()` for zero-trust QR SPKI certificate exchange.

---

## 3. Settings Paired Node Management
- **Hardware Nodes List:** Added Section 03 in Settings showing all paired PCs with real-time `ACTIVE` or `TAP TO CONNECT` badges.
- **In-Place Node Unpairing:** Users can unpair and disconnect PCs directly from the list with an instant reactive refresh without being booted back to the home screen.
- **Targeted IP / Port / Pin Config:** Manual edit dialogs now target whichever node is selected.

---

## 4. Build & Environment Updates
- Integrated portable OpenJDK 17 (`C:\Users\User\.jdks\jdk-17.0.13+11`) for Android Gradle Plugin 8.7.2 compatibility.
- Compiled and verified production debug APK: `Flick-TX1-debug.apk` (18.7 MB).
