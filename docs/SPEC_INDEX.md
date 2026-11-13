# ⚡ Flick (formerly EkSend) — Specification Index

> **Original Release Notes & 20/80 Architecture Blueprint**  
> *Archived here so the repository root `README.md` showcases the GitHub landing page.*

---

## 🚀 Release v2.2.0 Highlights (Zero-Trust Hardware mTLS 1.3)

- 🔒 **Zero-Trust Mutual TLS 1.3 & SPKI Pinning:** Both Android and Windows generate hardware-backed ECDSA P-256 keypairs (`StrongBox` / `TEE`). Ephemeral QR code handshake with HMAC proof of possession completely eliminates rogue MITM devices.
- 📱 **Concept 4.1 Signal Orange Ecosystem:** Designed to match Nothing OS and CMF aesthetics with Airborne Signal Orange (`#FF4400`), Matte Obsidian chassis (`#0B0A0E`), and high-legibility glowing badges.
- 📦 **One-Click Native Windows Installer (`Install_Flick.bat`):** 
  - Standard user installation into `%LOCALAPPDATA%\Programs\Flick\app` without UAC prompts.
  - Desktop & Start Menu shortcuts automatically configured.
  - Official Microsoft-signed `pythonw.exe` runner eliminating Windows Defender ML false positives.
  - Windows Auto-Startup in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
- 🔔 **Polished Windows Notification & Audio:** 
  - Native Windows chime suppressed via `NIIF_NOSOUND (0x10)` to eliminate double sounds.
  - Synthesized in-memory dual-tone arpeggio (C5 523Hz $\rightarrow$ G5 784Hz) arrival chime.
  - Application User Model ID (`Flick.WindowsReceiver`) configured so notifications display **`Flick`** instead of `Python`.
- ⚡ **Zero-Stutter Dashboard (`desktop_dashboard.py`):** Fingerprint caching prevents 3-second redraw thrashing and preserves user scroll position.
- 🛡️ **CameraX & Keystore Hardening:** Fixed Conscrypt `INCOMPATIBLE_DIGEST` by adding `DIGEST_NONE`, and resolved 90° portrait sensor QR recognition with software YUV rotation.

---

## 📚 Complete Specification Suite (20/80 Blueprint)

| Document | Scope & Contents |
| :--- | :--- |
| 🧠 [**`MEMORY.md`**](../MEMORY.md) | **Engineering Memory & System Ledger:** System topology, exact Teenage Engineering hardware design tokens, 3D keycap physics, active target IPs, wireless ADB workflow, and critical anti-slop rules. |
| 📄 [**`PRD.md`**](../PRD.md) | **Product Requirements Document:** Problem statement, personas, Phase 1 (MVP) vs. Phase 2 (Roadmap) scope, latency budgets (`<50ms` close, `<150ms` fast-path ping), and security boundaries. |
| 🏗️ [**`ARCHITECTURE.md`**](../ARCHITECTURE.md) | **System Architecture & Contracts:** System Mermaid diagram, UDP discovery beacon (`45454`), HTTP streaming contract (`45455`), Win32 `CF_HDROP`/`CF_UNICODETEXT`/`CF_DIB` binary struct layouts, v1.0.1 hardening, and v2.0 mTLS specification. |
| 🎨 [**`DESIGN.md`**](../DESIGN.md) | **Design System & Material 3 Expressive Tokens:** Inspired by tintotint's *"KDE Connect of the Future"* redesign—Dynamic Color bindings, Lavender-Indigo & Electric Teal fallback palette, `ScallopedDeviceBadge`, `DiscoveredPairCard`, Bento tiles, and haptic rules. |
| 🚀 [**`FEATURES.md`**](../FEATURES.md) | **Feature Matrix & Roadmap:** Complete inventory of v1.0.1 capabilities (`F-101` to `F-116`), v2.0 roadmap (`F-201` to `F-208`), and the Windows application paste behavior matrix. |
| 🛠️ [**`IMPLEMENTATION_PLAN.md`**](../IMPLEMENTATION_PLAN.md) | **Autonomous Execution Guide:** Exact file tree, step-by-step build phases, v1.0.1 packaging details, and Phase 7 v2.0 mTLS execution steps. |
| 📖 [**`HOW_TO_USE.md`**](../HOW_TO_USE.md) | **Complete User Guide (English & বাংলা):** Everyday zero-click sharing, in-app controls, QR code pairing, standalone `Flick.exe` setup, Windows autostart, and tray navigation. |
| 🛡️ [**`flick_security_bundle.md`**](../flick_security_bundle.md) | **Adversarial Security Bundle:** Full architectural threat model, attack vectors, remediation proof, and 13 complete source code files for external audits. |
| 📝 [**`ARGON_PLAN.md`**](../ARGON_PLAN.md) | **Initial Concept Notes:** Early architecture and product ideation notes. |
| 🦀 [**`RUST_REBUILD_SPEC.md`**](../RUST_REBUILD_SPEC.md) | **Rust Receiver Rebuild Spec:** Specification for the native Win32 Rust daemon (`windows-receiver-rust`). |
