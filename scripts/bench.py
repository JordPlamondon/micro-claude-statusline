#!/usr/bin/env python3
"""Reproduces the speed claim in the README.

    python3 scripts/bench.py

Runs the bash example from the Claude Code docs against this binary, on the
same payload, and reports CPU time per render. Takes about half a minute,
almost all of it waiting for bash.

CPU rather than wall clock, deliberately. A chain of short-lived processes
spends much of its wall time descheduled rather than computing, which
understates the gap: it measures around 44x on CPU and 22x on wall. CPU is also
the thing that multiplies when several sessions render at once, which is the
only situation where any of this is perceptible at all.

Interleaved and best-of-N, because a single-trial loop on a warm laptop varies
by a factor of three and will cheerfully report the wrong winner. Both sides
are measured in the same trial so thermal drift hits them equally.
"""

import json
import os
import resource
import shutil
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from golden import find_binary  # noqa: E402

RUNS = 30
TRIALS = 5

# From https://code.claude.com/docs/en/statusline, verbatim except for the
# token count, which is added so both sides display comparable information.
# Three jq invocations in the original, four here. Each one re-parses the whole
# payload, and each $( ) is another subshell.
DOCS_EXAMPLE = r"""#!/bin/bash
input=$(cat)

MODEL=$(echo "$input" | jq -r '.model.display_name')
DIR=$(echo "$input" | jq -r '.workspace.current_dir')
PCT=$(echo "$input" | jq -r '.context_window.used_percentage // 0' | cut -d. -f1)
TOK=$(echo "$input" | jq -r '.context_window.total_input_tokens // 0')

echo "[$MODEL] ${DIR##*/} | ${PCT}% context | ${TOK}"
"""

PAYLOAD = json.dumps(
    {
        "session_name": "promo-combine",
        "model": {"display_name": "Opus 5 (1M context)"},
        "workspace": {"current_dir": "/tmp"},
        "context_window": {
            "used_percentage": 42,
            "total_input_tokens": 424000,
            "context_window_size": 1000000,
        },
        "cost": {"total_cost_usd": 5.72},
        "rate_limits": {"five_hour": {"used_percentage": 38}},
    }
).encode()


def measure(cmd, env):
    """CPU and wall milliseconds per run, averaged over RUNS."""
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    start = time.perf_counter()
    for _ in range(RUNS):
        subprocess.run(
            cmd,
            input=PAYLOAD,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=env,
        )
    wall = (time.perf_counter() - start) / RUNS * 1000
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = (
        (after.ru_utime - before.ru_utime) + (after.ru_stime - before.ru_stime)
    ) / RUNS * 1000
    return cpu, wall


def main():
    if not shutil.which("jq"):
        sys.exit("jq is not installed, so the docs example cannot be run.\n  brew install jq")

    binary = find_binary()
    ours = binary if isinstance(binary, list) else [binary]

    with tempfile.TemporaryDirectory() as work:
        script = os.path.join(work, "docs-example.sh")
        with open(script, "w") as fh:
            fh.write(DOCS_EXAMPLE)
        os.chmod(script, 0o755)

        # A scratch HOME, so a config file does not quietly change the work.
        home = os.path.join(work, "home")
        os.makedirs(os.path.join(home, ".claude"))
        env = dict(os.environ)
        env["HOME"] = home
        env["COLUMNS"] = "999"
        env.pop("HERDR_WORKSPACE_ID", None)

        targets = [("docs example (bash + 4 jq)", [script]), ("statusline", ours)]
        best = {name: (float("inf"), float("inf")) for name, _ in targets}

        print(f"  {RUNS} runs x {TRIALS} interleaved trials, best of each\n")
        for trial in range(TRIALS):
            for name, cmd in targets:
                cpu, wall = measure(cmd, env)
                bc, bw = best[name]
                best[name] = (min(bc, cpu), min(bw, wall))
            # Only on a terminal. Piped to a file, \r is just a stray byte that
            # runs the progress line into the table.
            if sys.stdout.isatty():
                print(f"    trial {trial + 1}/{TRIALS} done", end="\r", flush=True)
        if sys.stdout.isatty():
            print(" " * 24, end="\r")

    print(f"    {'':<28}{'CPU/render':>12}{'wall/render':>14}")
    for name, _ in targets:
        cpu, wall = best[name]
        print(f"    {name:<28}{cpu:9.2f} ms {wall:11.2f} ms")

    theirs = best[targets[0][0]]
    mine = best[targets[1][0]]
    print(f"\n    {theirs[0] / mine[0]:.1f}x less CPU per render")
    print(f"    {theirs[1] / mine[1]:.1f}x less wall clock, for the reason in the docstring")
    return 0


if __name__ == "__main__":
    sys.exit(main())
