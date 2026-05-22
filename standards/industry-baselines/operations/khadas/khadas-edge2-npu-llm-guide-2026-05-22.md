<!-- uuid: 2ddf728b-650f-46c8-b96a-58e7abb6754b -->
<!-- source: https://docs.khadas.com/products/sbc/edge2/npu/llm-on-edge2 | version: 2025-04-23 | scraped: 2026-05-22 | tool: firecrawl v1.10.0 | re-pull: per Khadas docs revision -->
<!-- gate: [O], [P] -->

# LLM on Edge2 (Khadas Official Guide)

To quickly run large language models (LLMs) on Khadas Edge2 (RK3588S), run:

```bash
khadas_llm.sh
```

This script will:

- Install necessary dependencies such as `cmake`
- Prompt the operator to choose from the following supported models:
  1. DeepSeek 1.5B
  2. DeepSeek 7B
  3. Qwen2 2B-VL
  4. ChatGLM3 6B
- Automatically clone the required repositories
- Download the selected RKLLM model
- Compile the demo
- Configure tokens and context length
- Launch the inference demo optimized for Edge2 NPU

No manual setup required — just run the script and follow the prompts.

## Related Khadas Edge2 NPU resources

- Edge2 NPU Notes — `https://docs.khadas.com/products/sbc/edge2/npu/start`
- Edge2 NPU Model Convert — `https://docs.khadas.com/products/sbc/edge2/npu/npu-convert`
- Convert Your Model to ONNX — `https://docs.khadas.com/products/sbc/edge2/npu/convert-onnx`
- DeepSeek-R1-Distill-Qwen-1.5B/7B on Edge2 — `https://docs.khadas.com/products/sbc/edge2/npu/deepseek-r1-distill-qwen-1.5b-7b`
- Object Detection with RTSP Streaming — `https://docs.khadas.com/products/sbc/edge2/npu/object-detection-demo-with-rtsp`

## NPU Demos catalogued by Khadas

- YOLOv7-tiny
- YOLOv8n + OpenCV
- DenseNet CTC (ONNX/Keras)
- VGG16 (TensorFlow/Keras)
- RetinaFace (PyTorch)
- FaceNet (PyTorch)
- Face Recognition pipeline
- YOLOv8n-Pose
- TFLite demo

Last modified upstream: 2025/04/23 22:22 by jacobe.
