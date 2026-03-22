"""Progressive reward shaping for L-ARC Arena RL training.

Implements the 3-phase progressive reward emphasis from ToolRL:
  - Early (0-30%): format compliance + tool selection
  - Mid (30-70%): parameter accuracy + timing
  - Late (70-100%): full 8-dimensional balanced weights

This teaches the model to walk before it runs — master tool selection
first, then parameter precision, then the full reward surface.
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class PhaseWeights:
    """Weights for a single training phase."""

    judgment: float = 0.22
    parameter_accuracy: float = 0.18
    timing: float = 0.13
    result_usage: float = 0.13
    safety: float = 0.10
    efficiency: float = 0.08
    escalation: float = 0.05
    hallucination: float = 0.11

    def as_dict(self) -> dict[str, float]:
        """Convert to dictionary."""
        return {
            "judgment": self.judgment,
            "parameter_accuracy": self.parameter_accuracy,
            "timing": self.timing,
            "result_usage": self.result_usage,
            "safety": self.safety,
            "efficiency": self.efficiency,
            "escalation": self.escalation,
            "hallucination": self.hallucination,
        }


class ProgressiveRewardShaper:
    """3-phase progressive reward emphasis.

    Smoothly interpolates between phase-specific weight distributions
    as training progresses. Based on ToolRL's finding that progressive
    shaping improves convergence by 15-20% vs flat weights.
    """

    def __init__(self) -> None:
        # Early phase (0-30%): emphasize tool selection + hallucination avoidance.
        self.early = PhaseWeights(
            judgment=0.35, parameter_accuracy=0.05,
            timing=0.05, result_usage=0.05,
            safety=0.10, efficiency=0.05,
            escalation=0.05, hallucination=0.30,
        )

        # Mid phase (30-70%): shift to parameter accuracy + timing.
        self.mid = PhaseWeights(
            judgment=0.20, parameter_accuracy=0.25,
            timing=0.15, result_usage=0.10,
            safety=0.10, efficiency=0.05,
            escalation=0.05, hallucination=0.10,
        )

        # Late phase (70-100%): full balanced weights.
        self.late = PhaseWeights()  # defaults = production weights

    def get_weights(self, progress: float) -> dict[str, float]:
        """Get interpolated weights for the current training progress.

        Args:
            progress: Training progress from 0.0 (start) to 1.0 (end).

        Returns:
            Dictionary of dimension weights summing to 1.0.
        """
        progress = max(0.0, min(1.0, progress))

        if progress < 0.3:
            # Interpolate early → mid.
            t = progress / 0.3
            return self._interpolate(self.early, self.mid, t)
        if progress < 0.7:
            # Interpolate mid → late.
            t = (progress - 0.3) / 0.4
            return self._interpolate(self.mid, self.late, t)

        # Pure late phase.
        return self.late.as_dict()

    @staticmethod
    def _interpolate(
        a: PhaseWeights, b: PhaseWeights, t: float
    ) -> dict[str, float]:
        """Linear interpolation between two weight sets."""
        a_dict = a.as_dict()
        b_dict = b.as_dict()
        result = {}
        for key in a_dict:
            result[key] = a_dict[key] * (1 - t) + b_dict[key] * t
        return result


if __name__ == "__main__":
    # Demo: show weight evolution across training.
    shaper = ProgressiveRewardShaper()
    for pct in [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100]:
        progress = pct / 100
        weights = shaper.get_weights(progress)
        top_dim = max(weights, key=weights.get)
        print(f"Progress {pct:3d}%: top={top_dim:20s} "
              f"judgment={weights['judgment']:.2f} "
              f"params={weights['parameter_accuracy']:.2f} "
              f"halluc={weights['hallucination']:.2f}")
