# 🎨 Flick — Design System Specification (`DESIGN.md`)

**Document Version:** 2.1.2  
**Design Philosophy:** Teenage Engineering (TE) Industrial Hardware Aesthetic — inspired by the OP-1, TP-7, EP-133, and OB-4 field devices.  
**Ecosystem Platforms:** Android Companion App (Jetpack Compose) + Windows Receiver Desktop (`Flick.exe` via Tkinter/Win32)  
**Brand Identity:** FLICK // TX-1 (Transceiver Field Companion) & RX-1 (Desktop Field Receiver)

---

## 1. Visual North Star & Anti-Slop Principles

Flick rejects generic SaaS cards, gradient purple soup, and lifeless utility lists. It treats software as a **tactile, machined industrial physical instrument**:

1. **Machined Obsidian Chassis (`#0B0C0E`):** A deep matte obsidian slate canvas framed with beveled seams (`#1E2024` / `#282B31`) and industrial slotted screw heads.
2. **Sacred International Safety Orange (`#FF4400`):** Used intentionally on master triggers (`F1`), active indicators, live radio links, and primary function keys.
3. **Machined CRT / Telemetry Panels (`#0D0E10`):** Hardware displays showing machine hostname, direct LAN network telemetry, throughput speed LED meters, and an animated TP-7 motorized tape reel.
4. **Physical 3-Way Rocker Switches:** Dedicated hardware selectors for clipboard target injection (`01 MULTI`, `02 PATH`, `03 BITMAP`) with tactile 3D keycaps and instant live switching.
5. **3D Push-Buttons with Keycap Travel:** Master action buttons feature physical 3px/4px depression, top edge bevels, and tactile bottom shadow lips (`#882200`).
6. **Engineered Typography:** Swiss Grotesk (`Work Sans`) for confident headlines, paired with technical monospace (`Space Mono`) for telemetry, channels, and ports, and `Silkscreen` for digital segments.

---

## 2. Hardware Color Tokens

```python
# Teenage Engineering Hardware Palette (Shared across Android & Windows)
COLOR_BG               = "#0B0C0E"   # Deep obsidian matte chassis
COLOR_CHASSIS_BORDER   = "#1E2024"   # Outer bevel outline
COLOR_PANEL_BG         = "#131518"   # Machined industrial surface module plate
COLOR_PANEL_BORDER     = "#25282E"   # Module chamfer border
COLOR_CRT_BG           = "#0D0E10"   # Recessed CRT telemetry well
COLOR_CRT_BORDER       = "#22252B"   # CRT inner outline

# International Safety Orange (Signature TE Accent)
COLOR_ORANGE           = "#FF4400"   # International Safety Orange (Brand Core)
COLOR_ORANGE_BRIGHT    = "#FF5500"   # Keycap upper gradient
COLOR_ORANGE_DARK      = "#E63D00"   # Keycap lower gradient
COLOR_ORANGE_LIP       = "#882200"   # 3D mechanical button bottom shadow lip
COLOR_ORANGE_LIGHT     = "#FFAA77"   # Glowing text & legible subtitles
COLOR_ORANGE_BADGE_BG  = "#331206"   # Amber/Orange container

# Throughput LED Speed Meter
COLOR_AMBER            = "#FF8800"   # Mid-range transfer speed
COLOR_YELLOW           = "#FFE600"   # Peak throughput speed
COLOR_LED_OFF          = "#1C1F25"   # Inactive LED slot

# Tactile Dark Slate Keycaps
COLOR_KEYCAP_BG        = "#22252B"   # Dark slate tactile keycap
COLOR_KEYCAP_LIP       = "#111316"   # Keycap bottom shadow lip
COLOR_KEYCAP_BORDER    = "#353942"   # Keycap edge highlight
COLOR_KEYCAP_HOVER     = "#2A2E36"

# Typography
COLOR_TEXT_WHITE       = "#FFFFFF"   # Crisp white for titles & keycaps
COLOR_TEXT_MAIN        = "#EDEDED"   # Off-white reading text
COLOR_TEXT_MUTED       = "#8E95A2"   # Monospace technical telemetry
COLOR_TEXT_DIM         = "#565C68"   # Metadata / timestamps / channel tags
```

---

## 3. Typography Hierarchy

| Role | Font Family | Weight & Size | Usage |
| :--- | :--- | :--- | :--- |
| **Chassis Main Title** | `Work Sans` | Bold/Black, `20-24pt` | `"FLICK // RX-1 FIELD RECEIVER"` |
| **Machine Hostname** | `Work Sans` | Bold/Black, `18-20pt` | `"PARVEZ-PC"`, `"CMF-PHONE-1"` |
| **Subsystem Headers** | `Work Sans` | Bold, `11-13pt` | `"OPTICAL MATRIX"`, `"TRACK RECORDER"` |
| **Technical Telemetry** | `Space Mono` | Bold, `8-9pt` | `"ARMED // 45455-TCP"`, `"RTT: 006 MS"` |
| **Channel / Slot Tags** | `Space Mono` | Bold, `9pt` | `"CH-01"`, `"SLOT-01"`, `"REF: TE-RX1-2026"` |
| **Network Address** | `Space Mono` | Regular, `8-9pt` | `"192.168.0.200 : 45455-TCP • DIRECT LAN"` |
| **Digital CRT Speed** | `Silkscreen` | Regular, `8-9pt` | `"48.2 MB/S"`, `"0.0 MB/S // IDLE"` |

---

## 4. Hardware Interaction Blueprints

### 4.1 TP-7 Motorized Spool Reel
- Recreates the iconic Teenage Engineering TP-7 motorized field tape recorder.
- 48x48 / 56x56 rotating reel with a 3-spoke machined wheel and bright Safety Orange (`#FF4400`) center drive spindle.
- Rotates smoothly during peer connection and data transmission; displays live speed readout underneath.

### 4.2 Physical 3-Way Rocker Switch (Clipboard Mode)
- Direct tactile rocker tray with 3 states: `01 MULTI`, `02 PATH`, `03 BITMAP`.
- Active state has full 3D keycap bevel (`#FF5500` / `#E63D00` with `#882200` shadow lip).
- Instant live switching without having to navigate to secondary settings pages.

### 4.3 3D Push-Button Master Keys (`BentoActionTile.kt`)
- `F1` (Safety Orange Keycap): `TRANSMIT FILES` with subtitle `PHOTOS // 4K // DOC`. Dual-layer mechanical socket with 5dp physical press travel into chassis and bottom shadow lip compression.
- `F2` (Machined Graphite Keycap): `CLIPBOARD` with subtitle `INSTANT PC PASTE` and Safety Orange bolt icon. Features identical 5dp tactile push depression and top chamfer edge highlight.

### 4.4 Settings Calibration Matrix (`TactileModeKeycap`)
- Dedicated 2x2 grid with hardware index numbers (`01 MULTI`, `02 PATH`, `03 BITMAP`, `04 FILE`).
- 3D tactile state switches replacing all generic checkbox/checkmark patterns.
- Typography unified with Swiss Bold Sans to create a seamless aesthetic across all screens.

### 4.5 Session History
- Clean hardware log list of received and sent payloads with millisecond transfer latencies. No awkward `REC ●` or `// ` prefixes.
- Edge-to-edge `BackHandler` on all sub-screens returning directly to Home without app exit.
