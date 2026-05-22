<!-- uuid: 7eccdaa4-9ecb-4d38-a20e-e697eec82fa0 -->
<!-- source: https://www.electronics-lab.com/deepseek-r1-distill-qwen-1-5b-ai-model-deployed-on-rockchip-rk3588-soc-using-rkllm-toolkit/ | version: 2025-04-27 | scraped: 2026-05-22 | tool: firecrawl v1.10.0 | re-pull: when newer benchmark articles emerge -->
<!-- gate: [P], [O] -->

# DeepSeek-R1-Distill-Qwen-1.5B on Rockchip RK3588 NPU (RKLLM Toolkit)

Author: Debashis Das · Published: 2025-04-27 · Source: Electronics-Lab.com

## Hardware-accelerated LLM inference on RK3588

To enable hardware-accelerated inference, Radxa published instructions for running DeepSeek R1 (Qwen2 1.5B) on the Rockchip RK3588's 6 TOPS NPU using the **RKLLM toolkit**.

The toolkit converts trained models (HuggingFace / ONNX) on an x86 workstation into the `.rkllm` format. The compiled binary then runs on the development board via the RKLLM C API, targeting the NPU directly.

## Benchmark — Radxa-reported tokens/sec on RK3588(S)

| Model | Tokens/sec on NPU |
|-------|-------------------|
| **TinyLlama 1.1B** | **15.03 tok/s** |
| **Qwen 1.8B** | **14.18 tok/s** |
| **Phi3 3.8B** | **6.46 tok/s** |
| **ChatGLM3 (~6B)** | **3.67 tok/s** |

> Independent observation: DeepSeek-R1-Distill-Qwen-1.5B on NPU = **~15 tok/s**.
> DeepSeek-R1 (Qwen 14B) on RK3588 GPU = **1.4 tok/s** (required AMD W7700 dGPU for usable performance).

## RKLLM model layout (pre-compiled Radxa example)

Files in the published Radxa DeepSeek-R1-Distill-Qwen-1.5B_RKLLM repo:

- `configuration.json` — runtime config
- `librkllmrt.so` — RKLLM runtime library
- `llm_demo` — demo executable
- `DeepSeek-R1-Distill-Qwen-1.5B.rkllm` — 1.9 GB compiled model
- `README.md`

## Conversion workflow (x86 workstation, one-time per model)

```bash
git clone https://www.modelscope.cn/radxa/DeepSeek-R1-Distill-Qwen-1.5B_RKLLM.git

# Model conversion (Python on x86, produces .rkllm):
cd rknn-llm/rkllm-toolkit/examples/
python3 test.py
```

After conversion, the `*.rkllm` artefact is copied to the RK3588 board for inference via the RKLLM C API.

## Hardware substrate

- **NPU**: 6 TOPS, 3-core (RK3588), accessed via librkllmrt.so + RKLLM C API
- **Toolkits**: RKNN-LLM toolkit (model conversion) + RKNN runtime (on-device inference)
- **Supported chips**: RK3588, RK3588S, RK3576
- **Reference boards tested**: Radxa ROCK5 Model B (RK3588), Banana Pi BPI-M7 (RK3588)
- **Reference board for Light Architects**: Khadas Edge2 (RK3588S) — also supported by Khadas's official `khadas_llm.sh` bootstrap

## Implications for Light Architects (SOUL helix retrieval, CORSO code analysis)

- 1.5B–2B parameter models comfortably reach **~15 tok/s** on bare RK3588 NPU — usable for SOUL embedding generation, short-form CORSO code review, reasoning agents
- 3B–4B models drop to ~6 tok/s — viable for batch embedding work, slow for interactive
- 6B+ models fall under 4 tok/s on NPU alone — better suited to cloud routing or a dGPU
- The published 7B RKLLM (ahz-r3v/DeepSeek-R1-Distill-Qwen-7B-rk3588-rkllm-1.1.4 on HuggingFace) confirms quantised 7B fits and runs on 16 GB RAM RK3588 boards

## Cross-references

- See `operations/khadas/khadas-edge2-npu-llm-guide-2026-05-22.md` for the on-device bootstrap script (`khadas_llm.sh`).
