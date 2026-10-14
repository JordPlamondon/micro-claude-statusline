#!/usr/bin/env python3
"""The shared case list, used to generate golden files and to check against them.

Cases must be deterministic. Anything time-based is expressed as an offset from
a fixed epoch rather than from now, so the same input always produces the same
bytes. FIXED_NOW is passed to the binary as $STATUSLINE_TEST_PINNED_CLOCK.
"""

import json

# An arbitrary fixed instant. Only the offsets from it matter.
FIXED_NOW = 1_800_000_000

# Paths must exist identically on every machine, so no home directory here.
CASES = []


def case(name, payload, env=None):
    CASES.append({"name": name, "stdin": json.dumps(payload), "env": env or {}})


def raw_case(name, stdin):
    CASES.append({"name": name, "stdin": stdin, "env": {}})


# --- baseline shapes ---------------------------------------------------------
case("empty object", {})
case("model only", {"model": {"display_name": "Opus 5"}})
case(
    "full bar",
    {
        "session_name": "Mole cleanup",
        "model": {"display_name": "Opus 5 (1M context)"},
        "workspace": {"current_dir": "/tmp"},
        "context_window": {
            "used_percentage": 25,
            "total_input_tokens": 254000,
            "context_window_size": 1000000,
        },
        "cost": {"total_cost_usd": 5.72},
        "effort": {"level": "xhigh"},
        "pr": {"number": 265, "review_state": "approved"},
        "rate_limits": {
            "five_hour": {"used_percentage": 38, "resets_at": FIXED_NOW + 8400},
            "seven_day": {"used_percentage": 52, "resets_at": FIXED_NOW + 437400},
        },
    },
)

# --- meters: every whole percent, to catch rounding drift --------------------
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
                "five_hour": {"used_percentage": 40, "resets_at": FIXED_NOW + offset}
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

for kind in ("mr", "weird"):
    case(
        f"pr kind {kind}",
        {
            "model": {"display_name": "M"},
            "workspace": {"current_dir": "/tmp"},
            "pr": {"number": 7, "review_state": "pending", "kind": kind},
        },
    )

# --- location shapes ---------------------------------------------------------
for path in ("/tmp", "/", "/tmp/definitely-not-here", ""):
    case(
        f"cwd {path or '(empty)'}",
        {"model": {"display_name": "M"}, "workspace": {"current_dir": path}},
    )

case(
    "worktree field set",
    {"model": {"display_name": "M"}, "workspace": {"current_dir": "/tmp", "git_worktree": "wt"}},
)
case(
    "worktree matches dir",
    {"model": {"display_name": "M"}, "workspace": {"current_dir": "/tmp", "git_worktree": "tmp"}},
)
case(
    "top-level worktree.name",
    {
        "model": {"display_name": "M"},
        "workspace": {"current_dir": "/tmp"},
        "worktree": {"name": "wt"},
    },
)

# --- session name ------------------------------------------------------------
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

# --- config ------------------------------------------------------------------
# Written to a scratch HOME by the runner, so themes and segment lists are
# covered rather than only the defaults.
CONFIG_CASES = [
    ("theme catppuccin", {"theme": "catppuccin"}),
    ("theme tokyo-night", {"theme": "tokyo-night"}),
    ("theme gruvbox", {"theme": "gruvbox"}),
    ("theme dracula", {"theme": "dracula"}),
    ("theme one-dark", {"theme": "one-dark"}),
    ("segments trimmed", {"line1": ["model", "location"], "line2": ["cost"]}),
    ("segments reordered", {"line1": ["location", "pr", "session"], "line2": ["cost", "context"]}),
    ("split limits", {"line2": ["limit_7d", "context", "limit_5h"]}),
    ("narrow bars", {"bar_width": 4, "limit_bar_width": 2}),
    ("gap blank", {"gap": "blank"}),
    ("gap none, no bottom spacing", {"gap": "none", "bottom_spacing": 0}),
    ("gap scaled and rechar", {"gap_char": "-", "gap_scale": 0.5}),
    ("colour overrides", {"colors": {"brightest": "#ff00ff", "dim": [10, 20, 30]}}),
    ("short session width", {"session_name_width": 6}),
    ("forced truecolor off", {"truecolor": False}),
    ("garbage values ignored", {"theme": "nope", "bar_width": 0, "gap": "weird", "gap_scale": 9}),
    ("preset compact", {"preset": "compact"}),
    ("preset compact, overridden", {"preset": "compact", "bar_width": 16, "gap": "rule"}),
    ("preset compact + theme", {"preset": "compact", "theme": "gruvbox"}),
    ("preset unknown", {"preset": "nope"}),
    ("gap_char is an escape", {"gap_char": "\x1b"}),
    ("colour is multi-byte", {"colors": {"dim": "\u20ac\u20ac"}}),
    ("colour is two-byte pair", {"colors": {"dim": "\u00e9\u00e9\u00e9"}}),
]

CONFIG_PAYLOAD = {
    "session_name": "promo-combine",
    "model": {"display_name": "Opus 5 (1M context)"},
    "workspace": {"current_dir": "/tmp"},
    "context_window": {
        "used_percentage": 42,
        "total_input_tokens": 424000,
        "context_window_size": 1000000,
    },
    "cost": {"total_cost_usd": 5.72},
    "effort": {"level": "high"},
    "pr": {"number": 265, "review_state": "approved"},
    "rate_limits": {
        "five_hour": {"used_percentage": 38, "resets_at": FIXED_NOW + 8400},
        "seven_day": {"used_percentage": 61, "resets_at": FIXED_NOW + 437400},
    },
}

# --- control characters -------------------------------------------------------
# Everything user-derived that reaches stdout. A directory name containing an
# ESC needs no compromise of Claude Code, just working somewhere hostile.
ESC, BEL, DEL = "\x1b", "\x07", "\x7f"
for name, payload in [
    ("osc in session_name", {"session_name": f"{ESC}]0;PWNED{BEL}hi",
                             "model": {"display_name": "M"},
                             "workspace": {"current_dir": "/tmp"}}),
    ("osc in model name", {"model": {"display_name": f"M{ESC}]0;Y{BEL}"},
                           "workspace": {"current_dir": "/tmp"}}),
    ("sgr in effort", {"model": {"display_name": "M"},
                       "workspace": {"current_dir": "/tmp"},
                       "effort": {"level": f"{ESC}[31mred"}}),
    ("unterminated escape", {"session_name": f"a{ESC}[unterminated",
                             "model": {"display_name": "M"},
                             "workspace": {"current_dir": "/tmp"},
                             "context_window": {"used_percentage": 50}}),
    ("del and c1 bytes", {"session_name": f"a{DEL}b\u009fc",
                          "model": {"display_name": "M"},
                          "workspace": {"current_dir": "/tmp"}}),
    ("newline in session_name", {"session_name": "one\ntwo",
                                 "model": {"display_name": "M"},
                                 "workspace": {"current_dir": "/tmp"}}),
]:
    case(name, payload)

# --- wide characters ----------------------------------------------------------
# East Asian Wide and Fullwidth characters occupy two terminal columns. Counted
# as one, a row measured far narrower than it rendered, so shedding decided it
# fitted and the row wrapped anyway. These pin the column arithmetic; the
# shedding consequence is covered by WIDE_PAYLOAD against WIDTH_CASES below.
for name, payload in [
    ("cjk session name", {"session_name": "日本語のセッション",
                          "model": {"display_name": "M"},
                          "workspace": {"current_dir": "/tmp"}}),
    ("hangul session name", {"session_name": "한국어세션이름",
                             "model": {"display_name": "M"},
                             "workspace": {"current_dir": "/tmp"}}),
    ("emoji session name", {"session_name": "ship 🚀 it 🎉",
                            "model": {"display_name": "M"},
                            "workspace": {"current_dir": "/tmp"}}),
    ("fullwidth forms", {"session_name": "ＦＵＬＬＷＩＤＴＨ",
                         "model": {"display_name": "M"},
                         "workspace": {"current_dir": "/tmp"}}),
    ("cjk directory", {"model": {"display_name": "M"},
                       "workspace": {"current_dir": "/tmp/プロジェクト"}}),
    # Mixed, and with a second row, so the gap rule width is pinned too.
    ("wide with both rows", {"session_name": "日本語 mixed 名前",
                             "model": {"display_name": "Opus 5"},
                             "workspace": {"current_dir": "/tmp"},
                             "context_window": {"used_percentage": 42,
                                                "total_input_tokens": 424000,
                                                "context_window_size": 1000000},
                             "cost": {"total_cost_usd": 5.72}}),
]:
    case(name, payload)

# --- malformed ---------------------------------------------------------------
raw_case("raw: garbage", "xx")
raw_case("raw: empty", "")
raw_case("raw: null", "null")
raw_case("raw: array", "[1,2,3]")
raw_case("raw: number", "42")
raw_case("raw: truncated", '{"model":')

# --- responsive ---------------------------------------------------------------
# Widths the row is rendered at, to pin which segments get shed and when.
# 200 fits everything; 30 is below what the protected segments need, so it
# records the overflow rather than pretending it does not happen.
WIDTH_CASES = [200, 120, 100, 90, 80, 70, 60, 50, 40, 30]

# The same sweep with a session name that is twice as wide as it is long. Run
# against CONFIG_PAYLOAD's arithmetic this is where undercounting showed:
# every one of these widths believed the row fitted and let it wrap.
WIDE_PAYLOAD = dict(CONFIG_PAYLOAD, session_name="日本語のセッション名前")
