<!-- uuid: d95710d0-7646-4963-b749-d8d99f312d2d -->
<!-- source: https://www.khadas.com/captain | version: product page as of 2026-05-22 | scraped: 2026-05-22 | tool: firecrawl v1.10.0 | re-pull: per Khadas product page revision -->
<!-- gate: [A] -->

# Khadas Captain — Carrier Board for Edge Series

## Purpose

Captain is the official Khadas carrier board for the Edge SBC family (including Edge2). It mates with the SBC via a **314-pin MXM3 connector** and exposes expansion hardware not present on the bare Edge2 module.

## Key expansion features (relevant to Light Architects)

| Feature | Notes |
|---------|-------|
| **PCI-E M.2 Slot** | Full 4-lane PCI-E M.2 slot for **NVMe SSD** storage |
| **Li-Po battery socket** | High-efficiency charging IC, enables mobile/UPS operation |
| **Power Priority** | Captain ↔ Edge bidirectional power; supports 12 V DC barrel input and battery fallback |
| **APDS-9960** | Gesture detection + RGB colour + proximity + light sensing |
| **Gamepad inputs** | Dual gamepads + L/R function buttons |
| **Touchscreen** | Multi-touch via eDP + TP ports |
| **Kap Case** | Compatible enclosure converts Edge + Captain into a mini-PC |

## Connector and dock

> Captain is equipped with a 314-pin MXM3 connector that lets you modularly expand the features & capabilities of your Edge SBC. Use the Captain or design your own carrier board. Edge software can be configured to automatically detect your board.

Custom carrier-board designs are supported — operators are free to design board variants that expose only the I/O they need (e.g. NVMe + ethernet only, no gesture sensor).

## NVMe storage upgrade path

The full 4-lane PCI-E M.2 slot is the canonical Khadas-supported route for adding NVMe SSDs to an Edge2-based Light Architects deployment node. This eliminates the eMMC-only constraint of the bare module (currently 58 GB on the Khadas under audit).

Use cases unlocked by Captain + NVMe SSD:
- Local Neo4j database for SOUL helix at NVMe IOPS rather than eMMC
- Larger RKLLM model collection (7B–14B quantised) without competing for eMMC space
- Build cache (`CARGO_TARGET_DIR`) on fast storage rather than eMMC

## Cross-references

- See `operations/khadas/khadas-edge2-overview-2026-05-22.md` for the bare Edge2 baseline (no NVMe).
- See Khadas community thread `https://forum.khadas.com/t/edge-2-with-direct-nvme-m2-support/17506` for context on why M.2 is carrier-board only.

## Source contact

- Product page: `https://www.khadas.com/captain`
- Khadas contact form: `https://www.khadas.com/contact`
