#!/usr/bin/env python3
"""Byte-for-byte parity check across the three implementations.

Feeds identical stdin and environment to each, then compares raw bytes. Run it
after any change that touches output. It is what catches the subtle ones, such
as a currency rounding difference that only shows at $0.005.

    python3 scripts/parity.py

Set COLORTERM=truecolor to exercise the 24-bit path, or unset it for the
256-colour fallback. Both should report every case identical.

Implementations that are not present are skipped rather than failing, so this
works whether or not bun and the Rust binary are available.
"""

import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PY = os.path.join(ROOT, "scripts", "statusline.py")
JS = os.path.join(ROOT, "scripts", "statusline.js")
RS = os.path.join(ROOT, "rust", "target", "release", "statusline")

HOME = os.path.expanduser("~")
NOW = int(time.time())

cases = []


def case(name, payload, env=None):
    cases.append((name, payload, env or {}))


# --- baseline shapes ---------------------------------------------------------
case("empty object", {})
case("model only", {"model": {"display_name": "Opus 5"}})
case(
    "full bar",
    {
        "session_name": "Mole cleanup",
        "model": {"display_name": "Opus 5 (1M context)"},
        "workspace": {"current_dir": HOME},
        "context_window": {
            "used_percentage": 25,
            "total_input_tokens": 254000,
            "context_window_size": 1000000,
        },
        "cost": {"total_cost_usd": 5.72},
        "effort": {"level": "xhigh"},
        "pr": {"number": 265, "review_state": "approved"},
        "rate_limits": {
            "five_hour": {"used_percentage": 38, "resets_at": NOW + 8400},
            "seven_day": {"used_percentage": 52, "resets_at": NOW + 437400},
        },
    },
)

# --- meters: every whole percent, to catch rounding divergence ---------------
for pct in range(0, 101):
    case(
        f"meter {pct}%",
        {
            "model": {"display_name": "M"},
            "workspace": {"current_dir": "/tmp"},
            "context_window": {
                "used_percentage": pct,
                "total_input_tokens": pct * 10000,
                "context_window_size": 1000000,
            },
        },
    )

for pct in (0.4, 12.7, 49.9, 50.5, 99.6):
    case(
        f"meter frac {pct}",
        {
            "model": {"display_name": "M"},
            "workspace": {"current_dir": "/tmp"},
            "context_window": {"used_percentage": pct},
        },
    )

# --- token formatting boundaries --------------------------------------------
for used, size in (
    (999, 1000000),
    (1000, 1000000),
    (1500, 1000000),
    (999999, 1000000),
    (1000000, 1000000),
    (1500000, 2000000),
    (254000, 200000),
):
    case(
        f"tokens {used}/{size}",
        {
            "model": {"display_name": "M"},
            "workspace": {"current_dir": "/tmp"},
            "context_window": {
                "used_percentage": 50,
                "total_input_tokens": used,
                "context_window_size": size,
            },
        },
    )

# --- duration boundaries -----------------------------------------------------
for offset in (-50, 0, 60, 900, 3599, 3600, 8400, 86340, 86400, 90000, 437400, 604800):
    case(
        f"resets_at +{offset}",
        {
            "model": {"display_name": "M"},
            "workspace": {"current_dir": "/tmp"},
            "rate_limits": {
                "five_hour": {"used_percentage": 40, "resets_at": NOW + offset}
            },
        },
    )

# --- PR states ---------------------------------------------------------------
for state in ("approved", "changes_requested", "pending", "draft", "weird", None):
    payload = {
        "model": {"display_name": "M"},
        "workspace": {"current_dir": "/tmp"},
        "pr": {"number": 12},
    }
    if state is not None:
        payload["pr"]["review_state"] = state
    case(f"pr {state}", payload)

# --- location / git shapes ---------------------------------------------------
for path in (HOME, "/tmp", "/", os.path.join(HOME, "does-not-exist"), ""):
    case(
        f"cwd {path or '(empty)'}",
        {"model": {"display_name": "M"}, "workspace": {"current_dir": path}},
    )

case(
    "worktree field set",
    {
        "model": {"display_name": "M"},
        "workspace": {"current_dir": "/tmp", "git_worktree": "wt"},
    },
)
case(
    "worktree matches dir",
    {
        "model": {"display_name": "M"},
        "workspace": {"current_dir": "/tmp", "git_worktree": "tmp"},
    },
)
case(
    "top-level worktree.name",
    {
        "model": {"display_name": "M"},
        "workspace": {"current_dir": "/tmp"},
        "worktree": {"name": "wt"},
    },
)

# --- session name / herdr ----------------------------------------------------
case(
    "session_name short",
    {"session_name": "abc", "model": {"display_name": "M"}, "workspace": {"current_dir": "/tmp"}},
)
case(
    "session_name long",
    {
        "session_name": "Investigating terminal performance degradation across workspaces",
        "model": {"display_name": "M"},
        "workspace": {"current_dir": "/tmp"},
    },
)
case(
    "session_name blank",
    {"session_name": "   ", "model": {"display_name": "M"}, "workspace": {"current_dir": "/tmp"}},
)
case(
    "herdr id set, no session file",
    {"model": {"display_name": "M"}, "workspace": {"current_dir": "/tmp"}},
    env={"HERDR_WORKSPACE_ID": "no-such-workspace"},
)

# --- modes -------------------------------------------------------------------
case(
    "effort + fast",
    {
        "model": {"display_name": "M"},
        "workspace": {"current_dir": "/tmp"},
        "effort": {"level": "max"},
        "fast_mode": True,
    },
)
case(
    "fast only",
    {"model": {"display_name": "M"}, "workspace": {"current_dir": "/tmp"}, "fast_mode": True},
)

# --- cost --------------------------------------------------------------------
for cost in (0, 0.001, 0.005, 1.005, 5.72, 12.5, 1234.567):
    case(
        f"cost {cost}",
        {
            "model": {"display_name": "M"},
            "workspace": {"current_dir": "/tmp"},
            "cost": {"total_cost_usd": cost},
        },
    )

# --- malformed ---------------------------------------------------------------
RAW_CASES = [
    ("raw: garbage", "xx"),
    ("raw: empty", ""),
    ("raw: null", "null"),
    ("raw: array", "[1,2,3]"),
    ("raw: number", "42"),
    ("raw: truncated", '{"model":'),
]


def run(cmd, raw, env):
    full = dict(os.environ)
    full.pop("HERDR_WORKSPACE_ID", None)
    full.update(env)
    return subprocess.run(cmd, input=raw.encode(), capture_output=True, env=full).stdout


def main():
    impls = [("python", [PY])]
    if subprocess.run(["which", "bun"], capture_output=True).returncode == 0:
        impls.append(("js", ["bun", JS]))
    else:
        print("  skipping js, bun is not installed")
    if os.path.exists(RS):
        impls.append(("rust", [RS]))
    else:
        print("  skipping rust, build it with: cd rust && cargo build --release")

    if len(impls) < 2:
        print("only one implementation available, nothing to compare")
        return 0

    failures = []
    total = 0

    work = [(n, json.dumps(p), e) for n, p, e in cases]
    work += [(n, raw, {}) for n, raw in RAW_CASES]

    for name, raw, env in work:
        outputs = [(label, run(cmd, raw, env)) for label, cmd in impls]
        total += 1
        base_label, base = outputs[0]
        for label, out in outputs[1:]:
            if out != base:
                failures.append((f"{name}  [{base_label} vs {label}]", base, out))
                break

    mode = "24-bit" if os.environ.get("COLORTERM") in ("truecolor", "24bit") else "256-colour"
    names = ", ".join(label for label, _ in impls)
    print(f"{total - len(failures)}/{total} identical  ({names}, {mode})")
    for name, a, b in failures[:10]:
        print(f"\n  MISMATCH: {name}\n    {a!r}\n    {b!r}")
    if len(failures) > 10:
        print(f"\n  ... and {len(failures) - 10} more")
    return 1 if failures else 0


sys.exit(main())
