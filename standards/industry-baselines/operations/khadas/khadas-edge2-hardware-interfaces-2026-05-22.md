<!-- uuid: 96b5c421-4099-4a28-a9a6-84b3298b9ffb -->
<!-- source: https://docs.khadas.com/products/sbc/edge2/hardware/interfaces | version: 2025-08-11 | scraped: 2026-05-22 | tool: firecrawl v1.10.0 | re-pull: per Khadas docs revision -->
<!-- gate: [O], [A] -->

# Khadas Edge2 — Hardware Interfaces

## Top side connectors

| Label | Component | Purpose |
|-------|-----------|---------|
| 1 | USB-A | USB 3.1, blue |
| 2 | USB-C | USB 3.1 + DisplayPort |
| 3 | HDMI | Type-A female, up to 8K@60Hz, HDCP 2.3 |
| 4 | USB-C | Power delivery only |
| 5 | USB-A | USB 2.0, black |
| 6 | RGB LED1 | Status indicator |
| 7 | RTC Battery | Real-time clock backup |
| 8 | Reset Button | Force reboot |
| 9 | Function Button | Press 3× in 2s → MaskROM mode |
| A | Power Button | Power on |
| B | Digital Microphone | Right channel |
| C/D/E | Camera connectors | 30-pin, 0.5mm pitch, 4-lane, 16MP 30FPS ISP |
| F | Digital Microphone | Left channel |
| G | G-Sensor | 3-axis accelerometer |
| H | MHF4 Antenna | WiFi/Bluetooth |
| I | SPI Flash | Boot flash memory |
| J | PWM Fan Header | 4-wire |
| K | RGB LED2 | Status indicator |

## Back side connectors

| Label | Component | Purpose |
|-------|-----------|---------|
| 1 | Pogo pins | Debug + USB hub pads |
| 2 | DSI2 | 30-pin FPC, dual-channel display |
| 3 | DSI1 | 40-pin FPC, 4-lane 1080P + touch |
| 4 | IO2 | 30-pin FPC, expansion board connector |
| 5 | IO1 | 30-pin FPC, expansion board connector |
| 6 | XPWR pads | External power key |

## Hardware button matrix

| Reset | Function | Power | Effect |
|-------|----------|-------|--------|
| × | | | Force reboot |
| | × | | Enter Upgrade Mode (TST) |
| | | × | Power on / wake |
| × | | × | Enter Upgrade Mode (KEYS) |

### Special button shortcuts

- `FUNCTION` × 3 in 2 seconds → `MaskROM` mode
- Boot OOWOW recovery: hold `FUNCTION`, short-press `RESET`

## GPIO header reference

40-pin GPIO header documented at `/products/sbc/edge2/applications/gpio/40pin-header`.

## Notes for Light Architects

- The bare Edge2 has **no ethernet port and no M.2 slot**. All expansion runs through IO1/IO2 (carrier board) or USB.
- USB-A 3.1 (port 1, blue) is the highest-bandwidth peripheral interface — ideal for external SSDs.
- USB-A 2.0 (port 5, black) is appropriate for low-bandwidth devices (USB ethernet adapter, keyboard, RTL8152).
- The PWM fan header is wired and active — Edge2 throttles aggressively without active cooling under sustained NPU load.

Last modified upstream: 2025/08/11 05:37 by nick.
