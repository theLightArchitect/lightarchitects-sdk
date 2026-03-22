"""RL training from L-ARC Arena output.

Supports two algorithms (GEM ICLR 2026 recommendation):
  - GRPO: Group Relative Policy Optimization (sparse rewards)
  - REINFORCE+ReBN: Running-mean Baseline Normalization (dense rewards)

Our 8-dimensional reward system is dense and multi-dimensional, making
REINFORCE+ReBN the recommended default per GEM benchmarks.

Usage:
    python rl_train.py --data ./training-data/rl_trajectories.jsonl \
                       --model ./sft-output/final \
                       --output ./rl-output \
                       --rl-algo reinforce_rebn
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Any

import numpy as np
from datasets import Dataset
from peft import LoraConfig, TaskType
from transformers import AutoModelForCausalLM, AutoTokenizer
from trl import GRPOConfig, GRPOTrainer

from reward_shaping import ProgressiveRewardShaper


# Maximum line length in bytes for JSONL files (10 MiB).
_MAX_LINE_BYTES = 10 * 1024 * 1024


def load_arena_rl(path: Path) -> Dataset:
    """Load arena RL JSONL into a HuggingFace Dataset.

    Applies three safety guards:
    - Skips lines exceeding 10 MiB (RT-09: size limit).
    - Skips lines with missing required keys (RT-09: KeyError guard).
    - Clamps reward values to [0.0, 1.0] (RT-09: range validation).
    """
    examples = []
    with open(path) as f:
        for line_number, line in enumerate(f, start=1):
            # RT-09: Reject oversized lines before parsing.
            if len(line.encode()) > _MAX_LINE_BYTES:
                print(f"[load_arena_rl] line {line_number} exceeds {_MAX_LINE_BYTES} bytes — skipped")
                continue

            try:
                data = json.loads(line)
                reward = data["reward"]
                def _clamp(v: float) -> float:
                    """Clamp reward value to [0.0, 1.0]."""
                    return max(0.0, min(1.0, float(v)))

                examples.append({
                    "prompt": data["prompt"],
                    "completion": data["completion"],
                    "reward_total": _clamp(reward["total"]),
                    "reward_judgment": _clamp(reward["judgment"]),
                    "reward_param_accuracy": _clamp(reward["parameter_accuracy"]),
                    "reward_timing": _clamp(reward["timing"]),
                    "reward_result_usage": _clamp(reward["result_usage"]),
                    "reward_safety": _clamp(reward["safety"]),
                    "reward_efficiency": _clamp(reward["efficiency"]),
                    "reward_escalation": _clamp(reward["escalation"]),
                    "reward_hallucination": _clamp(reward["hallucination"]),
                })
            except (KeyError, ValueError, json.JSONDecodeError) as exc:
                # RT-09: Skip malformed lines rather than crashing the load.
                print(f"[load_arena_rl] line {line_number} skipped ({type(exc).__name__}: {exc})")
    return Dataset.from_list(examples)


class ReinforcePlusReBN:
    """REINFORCE with Running-mean Baseline Normalization.

    From GEM (ICLR 2026): handles dense multi-dimensional rewards better
    than GRPO. Maintains a running baseline per reward dimension and
    normalizes advantages using exponential moving average.

    The key insight from GEM: for dense per-step rewards (like our 8-dim
    system), ReBN provides lower variance policy gradients than GRPO's
    group-relative advantage computation.
    """

    def __init__(
        self,
        n_dimensions: int = 8,
        ema_decay: float = 0.99,
        reward_shaper: ProgressiveRewardShaper | None = None,
    ) -> None:
        self.baselines = np.zeros(n_dimensions)
        self.ema_decay = ema_decay
        self.n_dimensions = n_dimensions
        self.reward_shaper = reward_shaper
        self._step = 0

    def compute_advantage(
        self, rewards: dict[str, float], progress: float = 0.0
    ) -> float:
        """Compute normalized advantage for a single trajectory.

        Args:
            rewards: Per-dimension reward dict (8 dimensions).
            progress: Training progress (0.0-1.0) for progressive shaping.

        Returns:
            Scalar advantage value.
        """
        dim_names = [
            "judgment", "parameter_accuracy", "timing", "result_usage",
            "safety", "efficiency", "escalation", "hallucination",
        ]

        # Get current weights (possibly shaped by progress).
        if self.reward_shaper is not None:
            weights = self.reward_shaper.get_weights(progress)
        else:
            weights = {
                "judgment": 0.22, "parameter_accuracy": 0.18,
                "timing": 0.13, "result_usage": 0.13,
                "safety": 0.10, "efficiency": 0.08,
                "escalation": 0.05, "hallucination": 0.11,
            }

        # Compute per-dimension advantages.
        advantages = np.zeros(self.n_dimensions)
        reward_vec = np.zeros(self.n_dimensions)

        for i, name in enumerate(dim_names):
            r = rewards.get(name, 0.0)
            reward_vec[i] = r
            advantages[i] = r - self.baselines[i]

        # Update baselines with EMA.
        self.baselines = (
            self.ema_decay * self.baselines
            + (1 - self.ema_decay) * reward_vec
        )

        # Weighted sum of normalized advantages.
        weight_vec = np.array([weights[n] for n in dim_names])
        scalar_advantage = float(np.dot(advantages, weight_vec))

        self._step += 1
        return scalar_advantage


def make_reward_fn_from_dataset(
    dataset: Dataset,
    shaper: ProgressiveRewardShaper | None,
    total_steps: int,
    algo: str = "grpo",
    rebn: "ReinforcePlusReBN | None" = None,
):
    """Create a reward function that reads precomputed rewards from the dataset.

    For GRPO: returns the total reward directly (scalar).
    For REINFORCE+ReBN: returns the precomputed ReBN advantage.

    RT-05: Uses index-based tracking instead of completion-text lookup.
    Completion text is not a reliable key — identical completions from
    different examples would collide and return wrong rewards.
    """
    step_counter = {"step": 0}

    # RT-05: Build an index-ordered list of reward data. The GRPO/TRL
    # reward_fn is called with completions in the same order as the
    # dataset rows presented in each training batch. We track by
    # position (global step modulo dataset length) rather than by
    # completion text to avoid collision on identical completions.
    reward_list: list[dict[str, float]] = [
        {
            "total": example["reward_total"],
            "judgment": example["reward_judgment"],
            "parameter_accuracy": example["reward_param_accuracy"],
            "timing": example["reward_timing"],
            "result_usage": example["reward_result_usage"],
            "safety": example["reward_safety"],
            "efficiency": example["reward_efficiency"],
            "escalation": example["reward_escalation"],
            "hallucination": example["reward_hallucination"],
        }
        for example in dataset
    ]
    dataset_size = len(reward_list)

    def reward_fn(completions: list[str], **kwargs: Any) -> list[float]:
        progress = step_counter["step"] / max(total_steps, 1)
        rewards = []
        batch_size = len(completions)

        for batch_idx in range(batch_size):
            # RT-05: Derive dataset index from global step + batch position.
            # This avoids text-based lookup collisions entirely.
            dataset_idx = (step_counter["step"] * batch_size + batch_idx) % dataset_size
            reward_data = reward_list[dataset_idx]

            if algo == "reinforce_rebn" and rebn is not None:
                # Use precomputed ReBN advantage (dense, multi-dimensional).
                adv = rebn.compute_advantage(reward_data, progress)
                rewards.append(adv)
            else:
                # GRPO: use total reward, optionally shaped.
                if shaper is not None:
                    weights = shaper.get_weights(progress)
                    shaped_total = sum(
                        reward_data.get(dim, 0.0) * weights.get(dim, 0.0)
                        for dim in weights
                    )
                    rewards.append(shaped_total)
                else:
                    rewards.append(reward_data["total"])

        step_counter["step"] += 1
        return rewards

    return reward_fn


def main() -> None:
    parser = argparse.ArgumentParser(description="RL training from arena data")
    parser.add_argument("--data", type=Path, required=True, help="Path to RL JSONL")
    parser.add_argument("--model", type=str, required=True, help="SFT model path or ID")
    parser.add_argument("--output", type=Path, default=Path("./rl-output"))
    parser.add_argument(
        "--rl-algo",
        choices=["grpo", "reinforce_rebn"],
        default="reinforce_rebn",
        help="RL algorithm (default: reinforce_rebn, recommended for dense rewards)",
    )
    parser.add_argument("--lora-rank", type=int, default=16)
    parser.add_argument("--epochs", type=int, default=1)
    parser.add_argument("--batch-size", type=int, default=2)
    parser.add_argument("--lr", type=float, default=1e-5)
    parser.add_argument("--progressive-shaping", action="store_true", default=True)
    # RT-10: W&B must be opt-in. Default is "none" to avoid unintentional telemetry.
    parser.add_argument(
        "--report-to",
        dest="report_to",
        default="none",
        choices=["none", "wandb", "tensorboard"],
        help="Training metrics reporting backend (default: none)",
    )
    args = parser.parse_args()

    print(f"Loading arena RL data from {args.data}")
    dataset = load_arena_rl(args.data)
    print(f"Loaded {len(dataset)} trajectories")
    print(f"RL algorithm: {args.rl_algo}")

    shaper = ProgressiveRewardShaper() if args.progressive_shaping else None
    rebn = None  # Set below if using REINFORCE+ReBN.

    if args.rl_algo == "reinforce_rebn":
        print("Using REINFORCE+ReBN (GEM ICLR 2026 — dense reward specialist)")
        rebn = ReinforcePlusReBN(reward_shaper=shaper)

        # Compute advantages for each trajectory.
        advantages = []
        for i, example in enumerate(dataset):
            progress = i / len(dataset)
            reward_dict = {
                "judgment": example["reward_judgment"],
                "parameter_accuracy": example["reward_param_accuracy"],
                "timing": example["reward_timing"],
                "result_usage": example["reward_result_usage"],
                "safety": example["reward_safety"],
                "efficiency": example["reward_efficiency"],
                "escalation": example["reward_escalation"],
                "hallucination": example["reward_hallucination"],
            }
            adv = rebn.compute_advantage(reward_dict, progress)
            advantages.append(adv)

        dataset = dataset.add_column("advantage", advantages)
        print(f"Advantages computed: mean={np.mean(advantages):.4f}, "
              f"std={np.std(advantages):.4f}")

        # For REINFORCE+ReBN, we use the precomputed advantages
        # with a standard policy gradient update via GRPO infrastructure.
        print("Note: Using GRPO infrastructure with precomputed ReBN advantages")

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

    total_steps = len(dataset) * args.epochs // args.batch_size
    rebn_instance = rebn if args.rl_algo == "reinforce_rebn" else None
    reward_fn = make_reward_fn_from_dataset(
        dataset, shaper, total_steps, algo=args.rl_algo, rebn=rebn_instance,
    )

    training_config = GRPOConfig(
        output_dir=str(args.output),
        num_train_epochs=args.epochs,
        per_device_train_batch_size=args.batch_size,
        learning_rate=args.lr,
        logging_steps=10,
        save_strategy="epoch",
        report_to=args.report_to,  # RT-10: opt-in telemetry, default "none"
        run_name=f"l-arc-arena-rl-{args.rl_algo}",
    )

    trainer = GRPOTrainer(
        model=model,
        args=training_config,
        train_dataset=dataset,
        peft_config=lora_config,
        processing_class=tokenizer,
        reward_funcs=reward_fn,
    )

    print(f"Starting RL training ({args.rl_algo})...")
    trainer.train()
    trainer.save_model(str(args.output / "final"))
    tokenizer.save_pretrained(str(args.output / "final"))
    print(f"Training complete. Model saved to {args.output / 'final'}")


if __name__ == "__main__":
    main()
