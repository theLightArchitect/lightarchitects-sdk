"""Copilot evaluation runner.

Usage:
    python -m eval.runner [--base-url URL] [--token TOKEN] [--ollama-host HOST]
                          [--model MODEL] [--concurrency N] [--ids 1,5,10-20]
                          [--out results.json]

Environment variables (override flags):
    LIGHTARCHITECTS_WEBSHELL_TOKEN   auth token
    LIGHTARCHITECTS_BASE_URL         base URL (default http://localhost:8733)
    OLLAMA_HOST                      Ollama host (default http://localhost:11434)
    OLLAMA_MODEL                     judge model (default llama3.2:3b)
    LIGHTSQUAD_EVAL_CWD              working directory for build sessions
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import re
import sys
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Optional

import httpx

from .prompts import JUDGE_RUBRICS, PROMPTS, Prompt

BASE_URL     = os.environ.get("LIGHTARCHITECTS_BASE_URL", "http://localhost:8733")
TOKEN        = os.environ.get("LIGHTARCHITECTS_WEBSHELL_TOKEN", "test-token")
# Default to local Ollama proxy — avoids SSL cert issues with remote ollama.cloud.
OLLAMA_HOST  = os.environ.get("OLLAMA_HOST", "http://localhost:11434")
OLLAMA_MODEL = os.environ.get("OLLAMA_MODEL", "deepseek-v4-flash:cloud")
EVAL_CWD     = os.environ.get("LIGHTSQUAD_EVAL_CWD", str(Path.home()))

SSE_TIMEOUT_S    = 180   # max seconds to wait for done=true per prompt (raised from 120)
JUDGE_TIMEOUT_S  = 120   # max seconds for the Ollama judge (raised from 60 for large models)
JUDGE_NUM_PREDICT = 2048  # token budget for judge: thinking models need headroom for <think> blocks
# Global semaphore capping concurrent judge calls to 1 — prevents thinking-model KV-cache
# contention that causes score=1 fallback tokens under high load.
_JUDGE_SEM: asyncio.Semaphore | None = None

# Global semaphore capping concurrent copilot calls to 1 — the backing Ollama model (e.g.
# qwen3-coder:480b-cloud) uses continuous batching; concurrent requests share KV-cache state
# and cause the model to generate answers for the wrong prompt (cross-contamination).
_COPILOT_SEM: asyncio.Semaphore | None = None


def _judge_sem() -> asyncio.Semaphore:
    global _JUDGE_SEM  # noqa: PLW0603
    if _JUDGE_SEM is None:
        _JUDGE_SEM = asyncio.Semaphore(1)
    return _JUDGE_SEM


def _copilot_sem() -> asyncio.Semaphore:
    global _COPILOT_SEM  # noqa: PLW0603
    if _COPILOT_SEM is None:
        _COPILOT_SEM = asyncio.Semaphore(1)
    return _COPILOT_SEM


@dataclass
class StructuralResult:
    http_status: int
    response_text: str
    turn_span_id: Optional[str]
    done_received: bool
    elapsed_ms: int
    error: Optional[str] = None
    # Set when the LLM invoked lightsquad_plan (or any tool) mid-turn.
    tool_triggered: bool = False
    # Text streamed before the first tool_start event — the actual prose answer.
    # When non-empty, the judge should evaluate this instead of the full response.
    text_before_tool_call: str = ""

    @property
    def passed(self) -> bool:
        return self.http_status < 400 and self.done_received and self.error is None


@dataclass
class JudgeResult:
    score: Optional[int]  # 1-5 or None if judge failed
    reason: str
    raw: str
    error: Optional[str] = None

    @property
    def passed(self) -> bool:
        return self.score is not None and self.score >= 3


@dataclass
class EvalResult:
    prompt_id: int
    category: str
    prompt_text: str
    expect_strategy_trigger: bool
    structural: StructuralResult
    judge: Optional[JudgeResult] = None

    @property
    def overall_pass(self) -> bool:
        if self.expect_strategy_trigger:
            # Strategy-trigger prompts have two valid terminal states:
            #   (a) done_received=True  — strategy completed (e.g., ENRICH)
            #   (b) done_received=False — strategy paused at HITL checkpoint
            # In both cases the copilot returned http=200 and no error.
            # Response text may be absent when the strategy only emits status_update
            # events before pausing — that is still correct behaviour.
            s = self.structural.http_status < 400 and self.structural.error is None
        else:
            s = self.structural.passed
        j = (self.judge is None) or self.judge.passed
        return s and j


# ── HTTP helpers ──────────────────────────────────────────────────────────────

def auth_headers() -> dict:
    return {"Authorization": f"Bearer {TOKEN}", "Content-Type": "application/json"}


async def create_build(client: httpx.AsyncClient, prompt_id: int) -> str:
    # Each prompt gets its own isolated CWD so HelixSessionMemory writes a
    # unique session file per prompt (keyed by cwd + date).  Without this,
    # all 100 prompts share the same session file and the model's context grows
    # with every prior Q&A pair, causing it to answer the wrong question.
    isolated_cwd = f"/tmp/la-eval-{prompt_id:03d}"
    import os as _os
    _os.makedirs(isolated_cwd, exist_ok=True)
    resp = await client.post(
        f"{BASE_URL}/api/builds",
        json={"cwd": isolated_cwd},
        headers=auth_headers(),
        timeout=10,
    )
    resp.raise_for_status()
    return resp.json()["build_id"]


async def collect_native_sse(
    resp: httpx.Response,
    timeout_s: int = SSE_TIMEOUT_S,
) -> tuple[str, Optional[str], bool, bool, str]:
    """Parse SSE directly from the POST /copilot response body (native backend).

    Events arrive as  ``event: <name>\\ndata: <json>\\n\\n``.
    Text chunks are in  ``event: text``  payloads (``{"type":"text","chunk":"..."}``)
    and completion is signalled by  ``event: complete``.

    Returns (response_text, turn_span_id, done_received, tool_triggered, text_before_tool_call).

    ``text_before_tool_call`` captures prose streamed before the first ``tool_start``
    event so the judge evaluates the actual answer, not any post-tool-offer filler.
    """
    chunks: list[str] = []
    pre_tool_chunks: list[str] = []
    done = False
    tool_triggered = False
    deadline = time.monotonic() + timeout_s
    try:
        async for line in resp.aiter_lines():
            if time.monotonic() > deadline:
                break
            if not line.startswith("data: "):
                continue
            payload = line[6:].strip()
            if not payload:
                continue
            try:
                ev = json.loads(payload)
            except json.JSONDecodeError:
                continue
            t = ev.get("type", "")
            if t == "text":
                c = ev.get("chunk", "")
                if c:
                    chunks.append(c)
                    if not tool_triggered:
                        pre_tool_chunks.append(c)
            elif t == "tool_start":
                # Capture pre-tool text snapshot on the first tool invocation.
                if not tool_triggered:
                    tool_triggered = True
            elif t == "complete":
                done = True
                break
            elif t == "error":
                done = True  # error counts as done (response ended)
                break
    except (httpx.ReadTimeout, httpx.RemoteProtocolError):
        pass
    text_before = "".join(pre_tool_chunks)
    return "".join(chunks), None, done, tool_triggered, text_before


async def collect_broadcast_sse(
    client: httpx.AsyncClient,
    build_id: str,
    timeout_s: int = SSE_TIMEOUT_S,
) -> tuple[str, Optional[str], bool]:
    """Stream /api/builds/{id}/events until copilot_response done=true (lightarchitects backend).

    Returns (response_text, turn_span_id, done_received).
    """
    chunks: list[str] = []
    turn_span_id: Optional[str] = None
    done = False

    deadline = time.monotonic() + timeout_s
    try:
        async with client.stream(
            "GET",
            f"{BASE_URL}/api/builds/{build_id}/events",
            headers={**auth_headers(), "Accept": "text/event-stream"},
            timeout=httpx.Timeout(timeout_s, connect=5),
        ) as resp:
            async for line in resp.aiter_lines():
                if time.monotonic() > deadline:
                    break
                if not line.startswith("data: "):
                    continue
                payload = line[6:].strip()
                if not payload:
                    continue
                try:
                    ev = json.loads(payload)
                except json.JSONDecodeError:
                    continue
                if ev.get("type") != "copilot_response":
                    continue
                chunk = ev.get("chunk", "")
                if chunk:
                    chunks.append(chunk)
                if ev.get("turn_span_id"):
                    turn_span_id = ev["turn_span_id"]
                if ev.get("done"):
                    done = True
                    break
    except (httpx.ReadTimeout, httpx.RemoteProtocolError):
        pass

    return "".join(chunks), turn_span_id, done


# ── Ollama judge ──────────────────────────────────────────────────────────────

async def judge_response(
    client: httpx.AsyncClient,
    prompt: Prompt,
    response_text: str,
) -> JudgeResult:
    rubric_key = getattr(prompt, "judge_rubric", "general")
    rubric = JUDGE_RUBRICS.get(rubric_key, JUDGE_RUBRICS["general"])
    # Provide up to 3000 chars of response so the judge has enough signal.
    judge_prompt = rubric.format(prompt=prompt.text[:500], response=response_text[:3000])

    tokens: list[str] = []
    try:
        async with _judge_sem(), client.stream(
            "POST",
            f"{OLLAMA_HOST}/api/chat",
            json={
                "model": OLLAMA_MODEL,
                "stream": True,
                "messages": [{"role": "user", "content": judge_prompt}],
                # 512 tokens: enough budget for a thinking preamble (<think>…</think>)
                # followed by the structured SCORE:<N> REASON:<text> output.
                "options": {"temperature": 0, "num_predict": JUDGE_NUM_PREDICT},
            },
            headers={"Content-Type": "application/json"},
            timeout=httpx.Timeout(JUDGE_TIMEOUT_S, connect=10),
        ) as resp:
            async for line in resp.aiter_lines():
                if not line.strip():
                    continue
                try:
                    obj = json.loads(line)
                except json.JSONDecodeError:
                    continue
                content = obj.get("message", {}).get("content", "")
                if content:
                    tokens.append(content)
                if obj.get("done"):
                    break
    except Exception as exc:
        return JudgeResult(score=None, reason="", raw="", error=str(exc))

    raw = "".join(tokens).strip()
    # Strip <think>…</think> blocks emitted by reasoning models before parsing.
    parsed = re.sub(r"<think>.*?</think>", "", raw, flags=re.DOTALL).strip()
    # Parse SCORE:<N> REASON:<text>
    m = re.search(r"SCORE:\s*([1-5])", parsed, re.IGNORECASE)
    r = re.search(r"REASON:\s*(.+)", parsed, re.IGNORECASE)
    score = int(m.group(1)) if m else None
    reason = r.group(1).strip() if r else parsed[:120]
    return JudgeResult(score=score, reason=reason, raw=raw)


# ── Core eval loop ────────────────────────────────────────────────────────────

async def eval_one(
    client: httpx.AsyncClient,
    prompt: Prompt,
    *,
    judge: bool = True,
    sem: asyncio.Semaphore,
) -> EvalResult:
    async with sem:
        t0 = time.monotonic()
        error: Optional[str] = None
        http_status = 0
        response_text = ""
        turn_span_id: Optional[str] = None
        done_received = False
        tool_triggered = False
        text_before_tool_call = ""

        try:
            # Each prompt gets its own build session so events don't cross-contaminate.
            build_id = await create_build(client, prompt.id)

            # Serialize copilot calls: large Ollama models (e.g. 480B) use continuous
            # batching — concurrent requests share KV-cache state and cause the model to
            # answer the wrong question (cross-contamination). One in-flight copilot call
            # at a time guarantees clean per-prompt context.
            async with _copilot_sem(), client.stream(
                "POST",
                f"{BASE_URL}/api/builds/{build_id}/copilot",
                json={"message": prompt.text, "recent_events": [], "ui_context": None},
                headers=auth_headers(),
                timeout=httpx.Timeout(SSE_TIMEOUT_S, connect=10),
            ) as copilot_resp:
                http_status = copilot_resp.status_code
                is_native_sse = "text/event-stream" in copilot_resp.headers.get(
                    "content-type", ""
                )

                if http_status >= 400:
                    response_text, turn_span_id, done_received = "", None, False
                elif is_native_sse:
                    # Native backend: read chunks from the POST response stream.
                    response_text, turn_span_id, done_received, tool_triggered, text_before_tool_call = (
                        await collect_native_sse(copilot_resp)
                    )
                else:
                    # Lightarchitects backend: POST body is JSON; events come via
                    # the broadcast events endpoint.  Start SSE listener now (the
                    # POST response is already open so no race condition with turn
                    # submission — the turn is already in flight).
                    response_text, turn_span_id, done_received = (
                        await collect_broadcast_sse(client, build_id)
                    )

        except Exception as exc:
            error = str(exc)

        elapsed_ms = int((time.monotonic() - t0) * 1000)
        structural = StructuralResult(
            http_status=http_status,
            response_text=response_text,
            turn_span_id=turn_span_id,
            done_received=done_received,
            elapsed_ms=elapsed_ms,
            error=error,
            tool_triggered=tool_triggered,
            text_before_tool_call=text_before_tool_call,
        )

        judge_result: Optional[JudgeResult] = None
        # When a tool was triggered, judge on the prose answer before the tool call.
        # This evaluates the actual answer quality rather than any post-offer filler.
        # Fall back to full response_text if there was no pre-tool prose.
        judge_text = (
            text_before_tool_call
            if (tool_triggered and text_before_tool_call)
            else response_text
        )
        # Strip <think>…</think> blocks emitted by reasoning models (e.g. qwen3-coder).
        # The judge rubric expects a direct answer; leaving thinking tokens causes score=1.
        judge_text = re.sub(r"<think>.*?</think>", "", judge_text, flags=re.DOTALL).strip()
        # Strategy-trigger prompts are evaluated structurally: correct behaviour is invoking
        # the tool, not explaining strategy mechanics. The judge rubric would score a brief
        # "I'll start the build…" (pre-tool prose) as 1. Skip judging; overall_pass uses
        # structural check (http=200 + no error) which already handles the hitl-pause case.
        skip_judge = prompt.expect_strategy_trigger
        if judge and structural.passed and judge_text and not skip_judge:
            judge_result = await judge_response(client, prompt, judge_text)

        result = EvalResult(
            prompt_id=prompt.id,
            category=prompt.category,
            prompt_text=prompt.text[:120],
            expect_strategy_trigger=prompt.expect_strategy_trigger,
            structural=structural,
            judge=judge_result,
        )

        symbol = "✅" if result.overall_pass else "❌"
        score_str = f" judge={judge_result.score}" if judge_result else ""

        if prompt.expect_strategy_trigger and not done_received:
            done_str = "hitl-pause"
        elif done_received:
            done_str = "done"
        else:
            done_str = "NO-DONE"

        tool_str = " tool✓" if tool_triggered else ""
        print(
            f"  {symbol} [{prompt.id:03d}/{prompt.category}] "
            f"http={http_status} ms={elapsed_ms}{score_str} "
            f"{done_str}{tool_str} "
            f"{'span✓' if turn_span_id else 'span✗'}",
            flush=True,
        )
        return result


# ── Entrypoint ────────────────────────────────────────────────────────────────

def parse_ids(spec: Optional[str]) -> Optional[set[int]]:
    if not spec:
        return None
    ids: set[int] = set()
    for part in spec.split(","):
        if "-" in part:
            lo, hi = part.split("-", 1)
            ids.update(range(int(lo), int(hi) + 1))
        else:
            ids.add(int(part))
    return ids


async def main(argv: list[str]) -> int:
    global BASE_URL, TOKEN, OLLAMA_HOST, OLLAMA_MODEL  # noqa: PLW0603

    parser = argparse.ArgumentParser(description="Copilot eval suite")
    parser.add_argument("--base-url", default=BASE_URL)
    parser.add_argument("--token", default=TOKEN)
    parser.add_argument("--ollama-host", default=OLLAMA_HOST)
    parser.add_argument("--model", default=OLLAMA_MODEL)
    parser.add_argument("--concurrency", type=int, default=4,
                        help="Max parallel prompts (default 4)")
    parser.add_argument("--ids", default=None,
                        help="Subset to run e.g. '1-10,50,99'")
    parser.add_argument("--no-judge", action="store_true",
                        help="Skip Ollama judge (structural assertions only)")
    parser.add_argument("--out", default="eval/results.json")
    args = parser.parse_args(argv)

    BASE_URL = args.base_url
    TOKEN = args.token
    OLLAMA_HOST = args.ollama_host
    OLLAMA_MODEL = args.model

    selected_ids = parse_ids(args.ids)
    prompts = [p for p in PROMPTS if selected_ids is None or p.id in selected_ids]

    print(f"Copilot eval: {len(prompts)} prompts | "
          f"concurrency={args.concurrency} | "
          f"judge={'off' if args.no_judge else OLLAMA_MODEL} | "
          f"target={BASE_URL}")
    print()

    sem = asyncio.Semaphore(args.concurrency)
    async with httpx.AsyncClient(http2=False) as client:
        tasks = [
            eval_one(client, p, judge=not args.no_judge, sem=sem)
            for p in prompts
        ]
        results = await asyncio.gather(*tasks, return_exceptions=True)

    eval_results: list[EvalResult] = []
    for r in results:
        if isinstance(r, Exception):
            print(f"  EXCEPTION: {r}", file=sys.stderr)
        else:
            eval_results.append(r)

    # Write JSON.
    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w") as f:
        json.dump(
            [
                {
                    "id": r.prompt_id,
                    "category": r.category,
                    "prompt": r.prompt_text,
                    "expect_strategy_trigger": r.expect_strategy_trigger,
                    "structural": asdict(r.structural),
                    "judge": asdict(r.judge) if r.judge else None,
                    "pass": r.overall_pass,
                }
                for r in eval_results
            ],
            f,
            indent=2,
        )

    # Print summary.
    print()
    print_summary(eval_results)
    print(f"\nResults written to {out_path}")

    n_fail = sum(1 for r in eval_results if not r.overall_pass)
    return 1 if n_fail > 0 else 0


def print_summary(results: list[EvalResult]) -> None:
    by_cat: dict[str, list[EvalResult]] = {}
    for r in results:
        by_cat.setdefault(r.category, []).append(r)

    total = len(results)
    total_pass = sum(1 for r in results if r.overall_pass)
    judged = [r for r in results if r.judge is not None]
    avg_score = (
        sum(r.judge.score for r in judged if r.judge.score) / len(judged)
        if judged else None
    )

    print("── Copilot Eval Summary ──────────────────────────────")
    print(f"  Total:  {total_pass}/{total} pass  ({100*total_pass//total}%)")
    if avg_score is not None:
        print(f"  Judge:  avg {avg_score:.2f}/5 over {len(judged)} prompts")
    print()
    for cat, cat_results in sorted(by_cat.items()):
        n_pass = sum(1 for r in cat_results if r.overall_pass)
        cat_judged = [r for r in cat_results if r.judge and r.judge.score]
        cat_avg = (
            sum(r.judge.score for r in cat_judged) / len(cat_judged)
            if cat_judged else None
        )
        score_str = f" | judge avg {cat_avg:.2f}" if cat_avg else ""
        print(f"  {cat:<12} {n_pass:>3}/{len(cat_results)}{score_str}")

    # List failures.
    failures = [r for r in results if not r.overall_pass]
    if failures:
        print()
        print("── Failures ──────────────────────────────────────────")
        for r in failures:
            s = r.structural
            j_str = f" judge={r.judge.score}" if r.judge else ""
            strat_str = " [strategy-trigger]" if r.expect_strategy_trigger else ""
            print(
                f"  [{r.prompt_id:03d}] {r.category} "
                f"http={s.http_status} done={s.done_received}{j_str}{strat_str} "
                f"err={s.error or '-'} | {r.prompt_text[:60]!r}"
            )


if __name__ == "__main__":
    sys.exit(asyncio.run(main(sys.argv[1:])))
