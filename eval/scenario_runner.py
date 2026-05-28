"""Scenario-based copilot evaluation runner.

Extends the base eval runner with scenario-specific prompts and domain rubrics.
Evaluates whether the copilot routes to the correct sibling, triggers the
expected conversation format, and provides quality responses.

Usage:
    python -m eval.scenario_runner [--base-url URL] [--token TOKEN]
                                    [--ollama-host HOST] [--model MODEL]
                                    [--concurrency N] [--domains build,security]
                                    [--out scenario_results.json]

Environment variables (override flags):
    LIGHTARCHITECTS_WEBSHELL_TOKEN   auth token
    LIGHTARCHITECTS_BASE_URL         base URL (default http://localhost:8733)
    OLLAMA_HOST                      Ollama host (default http://localhost:11434)
    OLLAMA_MODEL                     judge model (default llama3.2:3b)
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

from .prompts import JUDGE_RUBRICS as BASE_RUBRICS
from .scenario_prompts import SCENARIO_JUDGE_RUBRICS, SCENARIO_PROMPTS, ScenarioPrompt

BASE_URL     = os.environ.get("LIGHTARCHITECTS_BASE_URL", "http://localhost:8733")
TOKEN        = os.environ.get("LIGHTARCHITECTS_WEBSHELL_TOKEN", "test-token")
OLLAMA_HOST  = os.environ.get("OLLAMA_HOST", "http://localhost:11434")
OLLAMA_MODEL = os.environ.get("OLLAMA_MODEL", "deepseek-v4-flash:cloud")
SSE_TIMEOUT_S    = 180
JUDGE_TIMEOUT_S  = 120
JUDGE_NUM_PREDICT = 2048

_JUDGE_SEM: asyncio.Semaphore | None = None
_COPILOT_SEM: asyncio.Semaphore | None = None


def _judge_sem() -> asyncio.Semaphore:
    global _JUDGE_SEM
    if _JUDGE_SEM is None:
        _JUDGE_SEM = asyncio.Semaphore(1)
    return _JUDGE_SEM


def _copilot_sem() -> asyncio.Semaphore:
    global _COPILOT_SEM
    if _COPILOT_SEM is None:
        _COPILOT_SEM = asyncio.Semaphore(1)
    return _COPILOT_SEM


# ── Data classes ────────────────────────────────────────────────────────────────

@dataclass
class StructuralResult:
    http_status: int
    response_text: str
    turn_span_id: Optional[str]
    done_received: bool
    elapsed_ms: int
    error: Optional[str] = None
    tool_triggered: bool = False
    text_before_tool_call: str = ""

    @property
    def passed(self) -> bool:
        return self.http_status < 400 and self.done_received and self.error is None


@dataclass
class JudgeResult:
    score: Optional[int]
    reason: str
    raw: str
    error: Optional[str] = None

    @property
    def passed(self) -> bool:
        return self.score is not None and self.score >= 3


@dataclass
class RoutingAssertion:
    """Result of checking whether the copilot routed to the expected sibling."""
    expected_sibling: str
    detected_keywords: list[str]
    routing_correct: bool
    note: str = ""


@dataclass
class ScenarioEvalResult:
    prompt_id: int
    domain: str
    prompt_text: str
    expected_sibling: str
    expected_preset: str
    expected_format: str
    expected_path: str
    structural: StructuralResult
    routing: Optional[RoutingAssertion] = None
    judge: Optional[JudgeResult] = None

    @property
    def overall_pass(self) -> bool:
        s = self.structural.passed or self.expected_path == "strategy_run"
        r = self.routing is None or self.routing.routing_correct
        j = (self.judge is None) or self.judge.passed
        return s and r and j


# ── HTTP helpers ────────────────────────────────────────────────────────────────

def auth_headers() -> dict:
    return {"Authorization": f"Bearer {TOKEN}", "Content-Type": "application/json"}


async def create_build(client: httpx.AsyncClient, prompt_id: int) -> str:
    isolated_cwd = f"/tmp/la-scenario-eval-{prompt_id:03d}"
    os.makedirs(isolated_cwd, exist_ok=True)
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
                if not tool_triggered:
                    tool_triggered = True
            elif t == "complete":
                done = True
                break
            elif t == "error":
                done = True
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


# ── Routing detection ──────────────────────────────────────────────────────────

SIBLING_KEYWORDS: dict[str, list[str]] = {
    "corso": ["quality", "guard", "clippy", "test", "review", "build", "verify", "code"],
    "eva":   ["deploy", "operations", "CI/CD", "emotions", "enrich", "identity", "persona"],
    "soul":  ["helix", "knowledge", "documentation", "vault", "voice", "FTS5", "search"],
    "quantum": ["research", "investigation", "forensic", "evidence", "prior art"],
    "seraph": ["pentest", "vulnerability", "OWASP", "CVE", "security", "scope", "injection"],
    "ayin":  ["trace", "span", "latency", "error_rate", "anomaly", "observe", "metric", "telemetry"],
    "laex":  ["canon", "standards", "compliance", "alignment", "constitution"],
}


def detect_routing(response_text: str, expected_sibling: str) -> RoutingAssertion:
    """Check if the response contains keywords suggesting the expected sibling.

    This is a heuristic check — the copilot may not explicitly mention
    sibling names, so we check for domain-specific vocabulary that indicates
    correct routing.
    """
    text_lower = response_text.lower()
    expected_keywords = SIBLING_KEYWORDS.get(expected_sibling, [])

    detected = [kw for kw in expected_keywords if kw.lower() in text_lower]

    # Also check if the sibling name itself is mentioned
    if expected_sibling.lower() in text_lower:
        detected.append(expected_sibling)

    return RoutingAssertion(
        expected_sibling=expected_sibling,
        detected_keywords=detected,
        routing_correct=len(detected) > 0,
        note=f"Found {len(detected)}/{len(expected_keywords)} expected keywords"
    )


# ── Ollama judge ────────────────────────────────────────────────────────────────

async def judge_response(
    client: httpx.AsyncClient,
    prompt: ScenarioPrompt,
    response_text: str,
) -> JudgeResult:
    rubric_key = prompt.judge_rubric
    rubric = SCENARIO_JUDGE_RUBRICS.get(rubric_key, SCENARIO_JUDGE_RUBRICS["routing"])
    judge_prompt = rubric.format(prompt=prompt.prompt_text[:500], response=response_text[:3000])

    tokens: list[str] = []
    try:
        async with _judge_sem(), client.stream(
            "POST",
            f"{OLLAMA_HOST}/api/chat",
            json={
                "model": OLLAMA_MODEL,
                "stream": True,
                "messages": [{"role": "user", "content": judge_prompt}],
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
    parsed = re.sub(r"<think>.*?</think>", "", raw, flags=re.DOTALL).strip()
    m = re.search(r"SCORE:\s*([1-5])", parsed, re.IGNORECASE)
    r = re.search(r"REASON:\s*(.+)", parsed, re.IGNORECASE)
    score = int(m.group(1)) if m else None
    reason = r.group(1).strip() if r else parsed[:120]
    return JudgeResult(score=score, reason=reason, raw=raw)


# ── Core eval loop ──────────────────────────────────────────────────────────────

async def eval_one(
    client: httpx.AsyncClient,
    prompt: ScenarioPrompt,
    *,
    judge: bool = True,
    sem: asyncio.Semaphore,
) -> ScenarioEvalResult:
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
            build_id = await create_build(client, prompt.id)

            # For strategy-triggered scenarios, subscribe to the broadcast SSE
            # stream BEFORE sending the copilot message to avoid missing early events.
            # The broadcast channel has a 4096-message buffer, so there's no race
            # with the strategy dispatch, but subscribing early ensures we catch
            # the very first progress event.
            broadcast_task = None
            if prompt.expect_strategy_trigger:
                broadcast_task = asyncio.create_task(
                    collect_broadcast_sse(client, build_id)
                )

            async with _copilot_sem(), client.stream(
                "POST",
                f"{BASE_URL}/api/builds/{build_id}/copilot",
                json={"message": prompt.prompt_text, "recent_events": [], "ui_context": None},
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
                    response_text, turn_span_id, done_received, tool_triggered, text_before_tool_call = (
                        await collect_native_sse(copilot_resp)
                    )
                else:
                    # Non-SSE response — could be a strategy dispatch (JSON acknowledgement)
                    # or a legacy Ollama response.  Read the body first.
                    body = await copilot_resp.aread()
                    body_text = body.decode("utf-8", errors="replace")
                    try:
                        body_json = json.loads(body_text)
                        strategy_dispatched = body_json.get("status") == "strategy_dispatched"
                    except (json.JSONDecodeError, AttributeError):
                        strategy_dispatched = False

                    if strategy_dispatched:
                        # Strategy pre-emption: the strategy engine is running
                        # asynchronously.  Collect events from the broadcast SSE stream.
                        tool_triggered = True
                        if broadcast_task is not None:
                            # Already subscribed before sending — await the result.
                            broadcast_text, broadcast_span, broadcast_done = (
                                await broadcast_task
                            )
                        else:
                            # Fallback: subscribe now (may miss early events).
                            broadcast_text, broadcast_span, broadcast_done = (
                                await collect_broadcast_sse(client, build_id)
                            )
                        response_text = broadcast_text
                        turn_span_id = broadcast_span or body_json.get("turn_span_id")
                        done_received = broadcast_done
                        if not text_before_tool_call:
                            text_before_tool_call = response_text
                    else:
                        # Legacy Ollama response — body is the response text.
                        response_text = body_text
                        turn_span_id = None
                        done_received = True

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

        # Routing detection
        judge_text = (
            text_before_tool_call
            if (tool_triggered and text_before_tool_call)
            else response_text
        )
        judge_text = re.sub(r"<think>.*?</think>", "", judge_text, flags=re.DOTALL).strip()
        routing = detect_routing(judge_text, prompt.expected_sibling) if judge_text else None

        # Judge scoring (skip for strategy triggers — evaluated structurally)
        judge_result: Optional[JudgeResult] = None
        skip_judge = prompt.expect_strategy_trigger
        if judge and structural.passed and judge_text and not skip_judge:
            judge_result = await judge_response(client, prompt, judge_text)

        result = ScenarioEvalResult(
            prompt_id=prompt.id,
            domain=prompt.domain,
            prompt_text=prompt.prompt_text[:120],
            expected_sibling=prompt.expected_sibling,
            expected_preset=prompt.expected_preset,
            expected_format=prompt.expected_format,
            expected_path=prompt.expected_path,
            structural=structural,
            routing=routing,
            judge=judge_result,
        )

        symbol = "✅" if result.overall_pass else "❌"
        score_str = f" judge={judge_result.score}" if judge_result else ""
        route_str = ""
        if routing:
            route_str = f" route={'✓' if routing.routing_correct else '✗'}"

        if prompt.expect_strategy_trigger and not done_received:
            done_str = "hitl-pause"
        elif done_received:
            done_str = "done"
        else:
            done_str = "NO-DONE"

        print(
            f"  {symbol} [{prompt.id:03d}/{prompt.domain}] "
            f"http={http_status} ms={elapsed_ms}{score_str}{route_str} "
            f"{done_str} "
            f"{'span✓' if turn_span_id else 'span✗'}",
            flush=True,
        )
        return result


# ── Entrypoint ──────────────────────────────────────────────────────────────────

def parse_domains(spec: Optional[str]) -> Optional[set[str]]:
    if not spec:
        return None
    return set(spec.split(","))


async def main(argv: list[str]) -> int:
    global BASE_URL, TOKEN, OLLAMA_HOST, OLLAMA_MODEL

    parser = argparse.ArgumentParser(description="Scenario-based copilot eval suite")
    parser.add_argument("--base-url", default=BASE_URL)
    parser.add_argument("--token", default=TOKEN)
    parser.add_argument("--ollama-host", default=OLLAMA_HOST)
    parser.add_argument("--model", default=OLLAMA_MODEL)
    parser.add_argument("--concurrency", type=int, default=4)
    parser.add_argument("--domains", default=None,
                        help="Subset of domains to run e.g. 'build,security,canon'")
    parser.add_argument("--ids", default=None,
                        help="Subset of IDs to run e.g. '1-10,50,99'")
    parser.add_argument("--no-judge", action="store_true",
                        help="Skip Ollama judge (structural + routing assertions only)")
    parser.add_argument("--no-routing", action="store_true",
                        help="Skip routing detection assertions")
    parser.add_argument("--out", default="eval/scenario_results.json")
    args = parser.parse_args(argv)

    BASE_URL = args.base_url
    TOKEN = args.token
    OLLAMA_HOST = args.ollama_host
    OLLAMA_MODEL = args.model

    selected_domains = parse_domains(args.domains)
    selected_ids: Optional[set[int]] = None
    if args.ids:
        selected_ids = set()
        for part in args.ids.split(","):
            if "-" in part:
                lo, hi = part.split("-", 1)
                selected_ids.update(range(int(lo), int(hi) + 1))
            else:
                selected_ids.add(int(part))

    prompts = [
        p for p in SCENARIO_PROMPTS
        if (selected_domains is None or p.domain in selected_domains)
        and (selected_ids is None or p.id in selected_ids)
    ]

    print(f"Scenario eval: {len(prompts)} prompts | "
          f"domains={','.join(sorted(set(p.domain for p in prompts)))} | "
          f"concurrency={args.concurrency} | "
          f"judge={'off' if args.no_judge else OLLAMA_MODEL} | "
          f"routing={'off' if args.no_routing else 'on'} | "
          f"target={BASE_URL}")
    print()

    sem = asyncio.Semaphore(args.concurrency)
    async with httpx.AsyncClient(http2=False) as client:
        tasks = [
            eval_one(client, p, judge=not args.no_judge, sem=sem)
            for p in prompts
        ]
        results = await asyncio.gather(*tasks, return_exceptions=True)

    eval_results: list[ScenarioEvalResult] = []
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
                    "domain": r.domain,
                    "prompt": r.prompt_text,
                    "expected_sibling": r.expected_sibling,
                    "expected_preset": r.expected_preset,
                    "expected_format": r.expected_format,
                    "expected_path": r.expected_path,
                    "structural": asdict(r.structural),
                    "routing": {
                        "expected": r.routing.expected_sibling,
                        "detected_keywords": r.routing.detected_keywords,
                        "routing_correct": r.routing.routing_correct,
                        "note": r.routing.note,
                    } if r.routing else None,
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


def print_summary(results: list[ScenarioEvalResult]) -> None:
    by_domain: dict[str, list[ScenarioEvalResult]] = {}
    for r in results:
        by_domain.setdefault(r.domain, []).append(r)

    total = len(results)
    total_pass = sum(1 for r in results if r.overall_pass)
    judged = [r for r in results if r.judge is not None]
    avg_score = (
        sum(r.judge.score for r in judged if r.judge.score) / len(judged)
        if judged else None
    )
    routed = [r for r in results if r.routing is not None]
    routing_pass = sum(1 for r in routed if r.routing.routing_correct)

    print("── Scenario Eval Summary ─────────────────────────────")
    print(f"  Total:    {total_pass}/{total} pass  ({100*total_pass//max(total,1)}%)")
    if avg_score is not None:
        print(f"  Judge:    avg {avg_score:.2f}/5 over {len(judged)} prompts")
    if routed:
        print(f"  Routing:  {routing_pass}/{len(routed)} correct  ({100*routing_pass//max(len(routed),1)}%)")
    print()
    for domain, domain_results in sorted(by_domain.items()):
        n_pass = sum(1 for r in domain_results if r.overall_pass)
        domain_judged = [r for r in domain_results if r.judge and r.judge.score]
        domain_avg = (
            sum(r.judge.score for r in domain_judged) / len(domain_judged)
            if domain_judged else None
        )
        domain_routed = [r for r in domain_results if r.routing is not None]
        domain_route_pass = sum(1 for r in domain_routed if r.routing.routing_correct)
        score_str = f" | judge {domain_avg:.2f}" if domain_avg else ""
        route_str = f" | route {domain_route_pass}/{len(domain_routed)}" if domain_routed else ""
        print(f"  {domain:<14} {n_pass:>3}/{len(domain_results)}{score_str}{route_str}")

    # List failures.
    failures = [r for r in results if not r.overall_pass]
    if failures:
        print()
        print("── Failures ──────────────────────────────────────────")
        for r in failures:
            s = r.structural
            j_str = f" judge={r.judge.score}" if r.judge else ""
            rt_str = f" route={'✓' if r.routing and r.routing.routing_correct else '✗'}" if r.routing else ""
            strat_str = " [strategy-trigger]" if r.expected_path == "strategy_run" else ""
            print(
                f"  [{r.prompt_id:03d}] {r.domain} "
                f"http={s.http_status} done={s.done_received}{j_str}{rt_str}{strat_str} "
                f"err={s.error or '-'} | {r.prompt_text[:60]!r}"
            )


if __name__ == "__main__":
    sys.exit(asyncio.run(main(sys.argv[1:])))