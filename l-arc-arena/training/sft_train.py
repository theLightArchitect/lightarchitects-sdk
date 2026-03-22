"""SFT training from L-ARC Arena output.

Loads arena-generated SFT JSONL (ChatML conversations) and fine-tunes
a model using TRL's SFTTrainer with LoRA.

Usage:
    python sft_train.py --data ./training-data/sft_examples.jsonl \
                        --model "meta-llama/Llama-3.1-8B" \
                        --output ./sft-output \
                        --lora-rank 16
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from datasets import Dataset
from peft import LoraConfig, TaskType
from transformers import AutoModelForCausalLM, AutoTokenizer
from trl import SFTConfig, SFTTrainer


# Maximum line length in bytes for JSONL files (10 MiB).
_MAX_LINE_BYTES = 10 * 1024 * 1024


def load_arena_sft(path: Path) -> Dataset:
    """Load arena SFT JSONL into a HuggingFace Dataset.

    Applies three safety guards:
    - Skips lines exceeding 10 MiB (RT-09: size limit).
    - Skips lines with missing required keys (RT-09: KeyError guard).
    - No reward values to validate here (SFT format has no rewards).
    """
    examples = []
    with open(path) as f:
        for line_number, line in enumerate(f, start=1):
            # RT-09: Reject oversized lines before parsing.
            if len(line.encode()) > _MAX_LINE_BYTES:
                print(f"[load_arena_sft] line {line_number} exceeds {_MAX_LINE_BYTES} bytes — skipped")
                continue

            try:
                data = json.loads(line)
                # Convert ChatML conversations to text.
                text_parts = []
                for msg in data["conversations"]:
                    role = msg["role"]
                    content = msg["content"]
                    text_parts.append(f"<|{role}|>\n{content}")
                text_parts.append("<|end|>")
                examples.append({"text": "\n".join(text_parts)})
            except (KeyError, ValueError, json.JSONDecodeError) as exc:
                # RT-09: Skip malformed lines rather than crashing the load.
                print(f"[load_arena_sft] line {line_number} skipped ({type(exc).__name__}: {exc})")
    return Dataset.from_list(examples)


def main() -> None:
    parser = argparse.ArgumentParser(description="SFT training from arena data")
    parser.add_argument("--data", type=Path, required=True, help="Path to SFT JSONL")
    parser.add_argument("--model", type=str, required=True, help="Base model ID")
    parser.add_argument("--output", type=Path, default=Path("./sft-output"))
    parser.add_argument("--lora-rank", type=int, default=16)
    parser.add_argument("--epochs", type=int, default=3)
    parser.add_argument("--batch-size", type=int, default=4)
    parser.add_argument("--lr", type=float, default=2e-4)
    parser.add_argument("--max-seq-length", type=int, default=2048)
    # RT-10: W&B must be opt-in. Default is "none" to avoid unintentional telemetry.
    parser.add_argument(
        "--report-to",
        dest="report_to",
        default="none",
        choices=["none", "wandb", "tensorboard"],
        help="Training metrics reporting backend (default: none)",
    )
    args = parser.parse_args()

    print(f"Loading arena SFT data from {args.data}")
    dataset = load_arena_sft(args.data)
    print(f"Loaded {len(dataset)} examples")

    print(f"Loading model: {args.model}")
    tokenizer = AutoTokenizer.from_pretrained(args.model)
    if tokenizer.pad_token is None:
        tokenizer.pad_token = tokenizer.eos_token

    model = AutoModelForCausalLM.from_pretrained(
        args.model,
        torch_dtype="auto",
        device_map="auto",
    )

    lora_config = LoraConfig(
        r=args.lora_rank,
        lora_alpha=args.lora_rank * 2,
        target_modules=["q_proj", "k_proj", "v_proj", "o_proj"],
        lora_dropout=0.05,
        bias="none",
        task_type=TaskType.CAUSAL_LM,
    )

    training_config = SFTConfig(
        output_dir=str(args.output),
        num_train_epochs=args.epochs,
        per_device_train_batch_size=args.batch_size,
        learning_rate=args.lr,
        max_seq_length=args.max_seq_length,
        logging_steps=10,
        save_strategy="epoch",
        report_to=args.report_to,  # RT-10: opt-in telemetry, default "none"
        run_name=f"l-arc-arena-sft-r{args.lora_rank}",
    )

    trainer = SFTTrainer(
        model=model,
        args=training_config,
        train_dataset=dataset,
        peft_config=lora_config,
        processing_class=tokenizer,
    )

    print("Starting SFT training...")
    trainer.train()
    trainer.save_model(str(args.output / "final"))
    tokenizer.save_pretrained(str(args.output / "final"))
    print(f"Training complete. Model saved to {args.output / 'final'}")


if __name__ == "__main__":
    main()
