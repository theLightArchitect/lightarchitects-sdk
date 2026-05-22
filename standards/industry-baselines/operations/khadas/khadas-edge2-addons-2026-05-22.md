<!-- uuid: bfd5e8a3-74bf-4b04-a149-f1cd8a36edd1 -->
<!-- source: https://docs.khadas.com/products/sbc/edge2/add-ons/start | version: 2022-07-19 | scraped: 2026-05-22 | tool: firecrawl v1.10.0 | re-pull: per Khadas docs revision -->
<!-- gate: [O], [A] -->

# Khadas Edge2 — Official Add-ons Catalogue

The Khadas Add-ons page is the canonical list of vendor-supported peripherals for the Edge2. Notably **absent**: an M.2 NVMe breakout board (NVMe support is provided via the separate Khadas Captain carrier board — see `architecture/khadas/khadas-captain-carrier-board-2026-05-22.md`).

## Catalogued add-ons

| Add-on | Purpose | Khadas docs path |
|--------|---------|------------------|
| **Edge2 Cooling Fan** | Active cooling for sustained NPU/CPU workloads | `/products/sbc/edge2/add-ons/cooling-fan` |
| **Edge2 TS050 Touchscreen** | 5" capacitive touchscreen via DSI | `/products/sbc/edge2/add-ons/ts050-touchscreen` |
| **Edge2 IMX585 MIPI Camera** | Sony IMX585 4K ISP camera (low-light) | `/products/sbc/edge2/add-ons/imx585-mipi-camera` |
| **Edge2 IMX415 MIPI Camera** | Sony IMX415 4K ISP camera | `/products/sbc/edge2/add-ons/imx415-mipi-camera` |
| **Edge2 OS08A10 MIPI Camera** | OmniVision OS08A10 ISP camera | `/products/sbc/edge2/add-ons/os08a10-mipi-camera` |
| **Edge2 IMX678 MIPI Camera** | Sony IMX678 ISP camera | `/products/sbc/edge2/add-ons/imx678-mipi-camera` |

General MIPI camera usage: `/products/sbc/edge2/add-ons/edge2-mipi-camera`

## Implications for Light Architects

- **Cooling fan is recommended** for any sustained NPU LLM workload — RK3588 throttles aggressively above ~75 °C, capping inference tok/s
- The MIPI camera options are not currently load-bearing in the platform but become relevant if SOUL ever gains a vision modality (e.g. SERAPH visual recon)
- For wired ethernet, no Edge2 add-on exists — see Captain carrier board or USB ethernet adapter

Last modified upstream: 2022/07/19 03:42 by frank.
