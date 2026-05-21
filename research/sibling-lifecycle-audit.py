#!/usr/bin/env python3
"""
Sibling MCP Lifecycle Audit — Phase 2 research script.
Classifies each sibling as `persistent` or `one_shot` by testing:
  1. spawn + initialize handshake
  2. notifications/initialized
  3. tools/list
  4. 3 sequential noop calls (or echo tool if available)

Results committed to docs/sibling-lifecycle.md
"""

import json
import subprocess
import sys
import time
import select
import os
import signal

TIMEOUT_S = 8.0

SIBLINGS = [
    {
        "name": "CORSO",
        "cmd": ["/Users/kft/lightarchitects/corso/bin/corso"],
        "env_extra": {"RUST_LOG": "error", "ANTHROPIC_API_KEY": os.environ.get("ANTHROPIC_API_KEY", "")},
    },
    {
        "name": "EVA",
        "cmd": ["/Users/kft/lightarchitects/eva/bin/eva"],
        "env_extra": {"RUST_LOG": "error", "ANTHROPIC_API_KEY": os.environ.get("ANTHROPIC_API_KEY", "")},
    },
    {
        "name": "SOUL",
        "cmd": ["/Users/kft/lightarchitects/soul/.config/bin/soul"],
        "env_extra": {"RUST_LOG": "error"},
    },
    {
        "name": "QUANTUM",
        "cmd": ["/Users/kft/lightarchitects/quantum/bin/quantum-q", "mcp-server"],
        "env_extra": {"RUST_LOG": "error", "ANTHROPIC_API_KEY": os.environ.get("ANTHROPIC_API_KEY", "")},
    },
    {
        "name": "SERAPH",
        "cmd": ["/Users/kft/lightarchitects/seraph/bin/seraph"],
        "env_extra": {"RUST_LOG": "error"},
    },
    {
        "name": "AYIN",
        "cmd": ["/Users/kft/lightarchitects/ayin/bin/ayin-mcp"],
        "env_extra": {"RUST_LOG": "error"},
    },
]

INIT_MSG = json.dumps({
    "jsonrpc": "2.0",
    "method": "initialize",
    "params": {
        "protocolVersion": "2025-11-25",
        "capabilities": {},
        "clientInfo": {"name": "lifecycle-audit", "version": "0.1.0"},
    },
    "id": 1,
}) + "\n"

INITIALIZED_NOTIF = json.dumps({
    "jsonrpc": "2.0",
    "method": "notifications/initialized",
}) + "\n"

TOOLS_LIST_MSG = json.dumps({
    "jsonrpc": "2.0",
    "method": "tools/list",
    "params": {},
    "id": 2,
}) + "\n"


def read_line(proc, timeout=TIMEOUT_S):
    """Read one newline-terminated JSON line with timeout. Returns None on timeout/EOF."""
    start = time.monotonic()
    buf = b""
    while time.monotonic() - start < timeout:
        remaining = timeout - (time.monotonic() - start)
        r, _, _ = select.select([proc.stdout], [], [], min(remaining, 0.2))
        if not r:
            if proc.poll() is not None:
                return None
            continue
        chunk = proc.stdout.read1(4096)
        if not chunk:
            return None
        buf += chunk
        if b"\n" in buf:
            line, _ = buf.split(b"\n", 1)
            return line.decode("utf-8", errors="replace")
    return None


def is_alive(proc):
    return proc.poll() is None


def audit_sibling(sib):
    name = sib["name"]
    cmd = sib["cmd"]
    env = {**os.environ, **sib["env_extra"]}

    result = {
        "sibling": name,
        "lifecycle": None,
        "initialize": "FAIL",
        "tools_list": "FAIL",
        "tool_count": 0,
        "sequential_calls": 0,
        "error": None,
    }

    try:
        proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            env=env,
        )
    except Exception as e:
        result["error"] = f"spawn failed: {e}"
        result["lifecycle"] = "ERROR"
        return result

    try:
        # Step 1: send initialize
        proc.stdin.write(INIT_MSG.encode())
        proc.stdin.flush()

        # Step 2: read initialize response
        line = read_line(proc)
        if line is None:
            result["error"] = "no initialize response (timeout or EOF)"
            result["lifecycle"] = "one_shot" if not is_alive(proc) else "ERROR"
            return result

        try:
            resp = json.loads(line)
            if "result" in resp and "serverInfo" in resp.get("result", {}):
                result["initialize"] = "PASS"
                result["server_info"] = resp["result"].get("serverInfo", {})
            elif "result" in resp:
                result["initialize"] = "PASS"
            else:
                result["error"] = f"unexpected initialize response: {line[:120]}"
        except json.JSONDecodeError:
            result["error"] = f"invalid JSON in initialize response: {line[:80]}"

        # Step 3: check if alive after initialize
        time.sleep(0.3)
        if not is_alive(proc):
            result["lifecycle"] = "one_shot"
            result["error"] = (result.get("error") or "") + " | process exited after initialize"
            return result

        # Step 4: send notifications/initialized
        proc.stdin.write(INITIALIZED_NOTIF.encode())
        proc.stdin.flush()
        time.sleep(0.1)

        if not is_alive(proc):
            result["lifecycle"] = "one_shot"
            result["error"] = (result.get("error") or "") + " | process exited after notifications/initialized"
            return result

        # Step 5: tools/list
        proc.stdin.write(TOOLS_LIST_MSG.encode())
        proc.stdin.flush()

        tl_line = read_line(proc, timeout=6.0)
        if tl_line is None:
            result["lifecycle"] = "one_shot" if not is_alive(proc) else "ERROR_no_tools_response"
            return result

        try:
            tl_resp = json.loads(tl_line)
            tools = tl_resp.get("result", {}).get("tools", [])
            result["tools_list"] = "PASS"
            result["tool_count"] = len(tools)
            result["tools_sample"] = [t["name"] for t in tools[:3]]
        except json.JSONDecodeError:
            result["tools_list"] = "PARSE_ERROR"

        # Step 6: 3 sequential calls (use first tool or ping)
        if result["tool_count"] > 0:
            tool_name = tools[0]["name"]
            for i in range(3):
                if not is_alive(proc):
                    break
                call_msg = json.dumps({
                    "jsonrpc": "2.0",
                    "method": "tools/call",
                    "params": {"name": tool_name, "arguments": {}},
                    "id": 10 + i,
                }) + "\n"
                try:
                    proc.stdin.write(call_msg.encode())
                    proc.stdin.flush()
                    call_resp = read_line(proc, timeout=5.0)
                    if call_resp is not None:
                        result["sequential_calls"] += 1
                    time.sleep(0.1)
                except (BrokenPipeError, OSError):
                    break

        # Final alive check
        time.sleep(0.2)
        if is_alive(proc):
            result["lifecycle"] = "persistent"
        else:
            result["lifecycle"] = "one_shot"

    finally:
        try:
            if is_alive(proc):
                proc.terminate()
                proc.wait(timeout=3)
        except Exception:
            try:
                proc.kill()
            except Exception:
                pass

    return result


def main():
    print("=== Sibling MCP Lifecycle Audit ===\n")
    results = []
    for sib in SIBLINGS:
        print(f"Testing {sib['name']}...", end=" ", flush=True)
        r = audit_sibling(sib)
        results.append(r)
        lc = r.get("lifecycle", "?")
        tools = r.get("tool_count", 0)
        err = r.get("error", "")
        print(f"{lc} | tools={tools} | init={r['initialize']} | seq={r['sequential_calls']}" + (f" | ERR={err[:60]}" if err else ""))

    print("\n=== Summary Table ===")
    print(f"{'Sibling':<10} {'Lifecycle':<12} {'Tools':>6} {'Sequential':>10}")
    print("-" * 45)
    for r in results:
        print(f"{r['sibling']:<10} {r.get('lifecycle','?'):<12} {r.get('tool_count',0):>6} {r.get('sequential_calls',0):>10}")

    print("\n=== JSON Results ===")
    print(json.dumps(results, indent=2))

    return results


if __name__ == "__main__":
    main()
