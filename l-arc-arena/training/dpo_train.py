"""DPO training from L-ARC Arena output.

Loads arena-generated DPO JSONL (chosen/rejected pairs) and fine-tunes
a model using TRL's DPOTrainer with LoRA.

Usage:
    python dpo_train.py --data ./training-data/dpo_pairs.jsonl \
                        --model ./sft-output/final \
                        --output ./dpo-output
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from datasets import Dataset
from peft import LoraConfig, TaskType
from transformers import AutoModelForCausalLM, AutoTokenizer
from trl import DPOConfig, DPOTrainer


# Maximum line length in bytes for JSONL files (10 MiB).
_MAX_LINE_BYTES = 10 * 1024 * 1024


def load_arena_dpo(path: Path) -> Dataset:
    """Load arena DPO JSONL into a HuggingFace Dataset.

    Applies two safety guards:
    - Skips lines exceeding 10 MiB (RT-09: size limit).
    - Skips lines with missing required keys (RT-09: KeyError guard).
    """
    examples = []
    with open(path) as f:
        for line_number, line in enumerate(f, start=1):
            # RT-09: Reject oversized lines before parsing.
            if len(line.encode()) > _MAX_LINE_BYTES:
                print(f"[load_arena_dpo] line {line_number} exceeds {_MAX_LINE_BYTES} bytes — skipped")
                continue

            try:
                data = json.loads(line)
                examples.append({
                    "prompt": data["prompt"],
                    "chosen": data["chosen"],
                    "rejected": data["rejected"],
                })
            except (KeyError, ValueError, json.JSONDecodeError) as exc:
                # RT-09: Skip malformed lines rather than crashing the load.
                print(f"[load_arena_dpo] line {line_number} skipped ({type(exc).__name__}: {exc})")
    return Dataset.from_list(examples)


def main() -> None:
    parser = argparse.ArgumentParser(description="DPO training from arena data")
    parser.add_argument("--data", type=Path, required=True, help="Path to DPO JSONL")
    parser.add_argument("--model", type=str, required=True, help="SFT model path or ID")
    parser.add_argument("--output", type=Path, default=Path("./dpo-output"))
    parser.add_argument("--lora-rank", type=int, default=16)
    parser.add_argument("--epochs", type=int, default=1)
    parser.add_argument("--batch-size", type=int, default=2)
    parser.add_argument("--lr", type=float, default=5e-5)
    parser.add_argument("--beta", type=float, default=0.1, help="DPO beta parameter")
    # RT-10: W&B must be opt-in. Default is "none" to avoid unintentional telemetry.
    parser.add_argument(
        "--report-to",
        dest="report_to",
        default="none",
        choices=["none", "wandb", "tensorboard"],
        help="Training metrics reporting backend (default: none)",
    )
    args = parser.parse_args()

    print(f"Loading arena DPO data from {args.data}")
    dataset = load_arena_dpo(args.data)
    print(f"Loaded {len(dataset)} preference pairs")

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

    training_config = DPOConfig(
        output_dir=str(args.output),
        num_train_epochs=args.epochs,
        per_device_train_batch_size=args.batch_size,
        learning_rate=args.lr,
        beta=args.beta,
        logging_steps=10,
        save_strategy="epoch",
        report_to=args.report_to,  # RT-10: opt-in telemetry, default "none"
        run_name=f"l-arc-arena-dpo-b{args.beta}",
    )

    trainer = DPOTrainer(
        model=model,
        args=training_config,
        train_dataset=dataset,
        peft_config=lora_config,
        processing_class=tokenizer,
    )

    print("Starting DPO training...")
    trainer.train()
    trainer.save_model(str(args.output / "final"))
    tokenizer.save_pretrained(str(args.output / "final"))
    print(f"Training complete. Model saved to {args.output / 'final'}")


if __name__ == "__main__":
    main()
