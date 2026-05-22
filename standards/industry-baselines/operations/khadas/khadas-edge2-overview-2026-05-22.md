<!-- uuid: 393c6566-a7d9-4c66-abc9-16b6cd37095a -->
<!-- source: https://docs.khadas.com/products/sbc/edge2/start | version: 2025-05-14 | scraped: 2026-05-22 | tool: firecrawl v1.10.0 | re-pull: per Khadas docs revision -->
<!-- gate: [O], [A] -->

# Khadas Edge2 — SoC, CPU, NPU Overview

Tags: Edge2, Rockchip, RK3588S, ARM, aarch64

The **Khadas Edge2** is an ultraslim, credit-card-sized SBC from the Khadas Edge Series, powered by a **Rockchip RK3588S/RK3588S2** SoC.

## Headline specs

- **CPU**: 4× Cortex-A76 @ 2.2–2.4 GHz (big) + 4× Cortex-A55 @ 1.8 GHz (little)
- **NPU**: 6 TOPS (3 cores)
- **Form factor**: credit-card SBC, Pogo Pads + FPC headers + MIPI for expansion
- **No onboard NVMe/M.2 slot on bare module** — requires Captain carrier board for NVMe
- **No onboard ethernet on bare module** — relies on USB ethernet adapter or carrier board
- **Specs PDF**: `https://dl.khadas.com/products/edge2/specs/edge2_specs.pdf`

## Official documentation topics

- Getting Started — `/products/sbc/edge2/getting-started/start`
- Install OS — `/products/sbc/edge2/install-os/start`
- OS Images — `/products/sbc/edge2/os-images/start`
- Configuration Note — `/products/sbc/edge2/configurations/start`
- Application Note — `/products/sbc/edge2/applications/start`
- **NPU Note** — `/products/sbc/edge2/npu/start` (LLM on Edge2, model convert, YOLO demos)
- Development Guide — `/products/sbc/edge2/development/start`
- **Hardware Documentation** — `/products/sbc/edge2/hardware/start`
- Add-ons — `/products/sbc/edge2/add-ons/start`
- Troubleshooting — `/products/sbc/edge2/troubleshooting/start`

## External references

- Khadas product page — `https://khadas.com/edge2`
- Khadas Community forum — `https://forum.khadas.com/`
- Khadas GitHub — `https://github.com/khadas/`
- Khadas Downloads (firmware, manuals, specs PDFs) — `https://dl.khadas.com/`

## Light Architects deployment notes (Khadas as inference node)

- Verified on `khadas@10.129.155.20` running Ubuntu 24.04.3 LTS, kernel 6.1.118
- 16 GB LPDDR5 RAM (14 GB free at idle)
- 58 GB eMMC (`mmcblk0p2`) — sole Linux-native storage on bare module
- RKNPU driver v0.9.8 loaded; userspace tooling (`librknnrt.so`, `rknn_server`) installed via `khadas_llm.sh`
- Mali-G610 r0p0 with OpenCL (`libmali-valhall-g610`) — confirmed via `clinfo`
- WiFi 6 via BCM43752 (PCIe-attached, working); USB ethernet adapter (Realtek RTL8152, 100 Mbit) optional

Last modified upstream: 2025/05/14 22:07 by nick.
