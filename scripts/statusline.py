#!/usr/bin/env python3
"""Claude Code status line.

Reads the statusLine JSON payload on stdin, prints a two-row bar. Segments,
widths and palette come from ~/.claude/statusline.json when it exists.

Needs nothing but python3, which is why it is the default install. The Rust
build in rust/ does the same job at about a fortieth of the CPU, which only
matters if you run several Claude Code sessions at once.

Output must stay byte-identical to the js and rust implementations. See
scripts/parity.py.
"""

import json
import os
import re
import sys
import time

ANSI_RE = re.compile(r"\033\[[0-9;]*m")
RESET = "\033[0m"

EIGHTHS = " ▏▎▍▌▋▊▉█"
TRACK = "░"
# U+2800 renders blank but is not whitespace, so it survives trimming.
BLANK_ROW = "⠀"

# Every theme fills the same thirteen slots, so swapping one changes the whole
# bar without touching render code. The meter gradient is derived from four of
# them rather than defined separately, which keeps a new theme to one row.
#
#   separator  separators and the rule between rows, should recede
#   dim        unfilled meter track, PR "draft", dimmest readable tone
#   muted      secondary text: token counts, countdowns, effort
#   bright     session cost
#   brightest  session or workspace name
#   accent_a   worktree marker
#   accent_b   directory name, and the low end of the gradient
#   accent_c   git branch
#   model      model name
#   good       PR approved, second gradient stop
#   warn       PR pending, third gradient stop
#   bad        PR changes requested, top of the gradient
#   notice     fast mode
THEMES = {
    "nord": {
        "separator": (0x43, 0x4C, 0x5E), "dim": (0x4C, 0x56, 0x6A),
        "muted": (0x7D, 0x86, 0x96), "bright": (0xD8, 0xDE, 0xE9),
        "brightest": (0xEC, 0xEF, 0xF4), "accent_a": (0x8F, 0xBC, 0xBB),
        "accent_b": (0x88, 0xC0, 0xD0), "accent_c": (0x81, 0xA1, 0xC1),
        "model": (0xB4, 0x8E, 0xAD), "good": (0xA3, 0xBE, 0x8C),
        "warn": (0xEB, 0xCB, 0x8B), "bad": (0xBF, 0x61, 0x6A),
        "notice": (0xD0, 0x87, 0x70),
    },
    "catppuccin": {
        "separator": (0x45, 0x47, 0x5A), "dim": (0x58, 0x5B, 0x70),
        "muted": (0x7F, 0x84, 0x9C), "bright": (0xBA, 0xC2, 0xDE),
        "brightest": (0xCD, 0xD6, 0xF4), "accent_a": (0x94, 0xE2, 0xD5),
        "accent_b": (0x89, 0xDC, 0xEB), "accent_c": (0x89, 0xB4, 0xFA),
        "model": (0xCB, 0xA6, 0xF7), "good": (0xA6, 0xE3, 0xA1),
        "warn": (0xF9, 0xE2, 0xAF), "bad": (0xF3, 0x8B, 0xA8),
        "notice": (0xFA, 0xB3, 0x87),
    },
    "tokyo-night": {
        "separator": (0x29, 0x2E, 0x42), "dim": (0x41, 0x48, 0x68),
        "muted": (0x56, 0x5F, 0x89), "bright": (0xA9, 0xB1, 0xD6),
        "brightest": (0xC0, 0xCA, 0xF5), "accent_a": (0x1A, 0xBC, 0x9C),
        "accent_b": (0x7D, 0xCF, 0xFF), "accent_c": (0x7A, 0xA2, 0xF7),
        "model": (0xBB, 0x9A, 0xF7), "good": (0x9E, 0xCE, 0x6A),
        "warn": (0xE0, 0xAF, 0x68), "bad": (0xF7, 0x76, 0x8E),
        "notice": (0xFF, 0x9E, 0x64),
    },
    "gruvbox": {
        "separator": (0x3C, 0x38, 0x36), "dim": (0x50, 0x49, 0x45),
        "muted": (0x92, 0x83, 0x74), "bright": (0xD5, 0xC4, 0xA1),
        "brightest": (0xEB, 0xDB, 0xB2), "accent_a": (0x8E, 0xC0, 0x7C),
        "accent_b": (0x83, 0xA5, 0x98), "accent_c": (0x45, 0x85, 0x88),
        "model": (0xD3, 0x86, 0x9B), "good": (0xB8, 0xBB, 0x26),
        "warn": (0xFA, 0xBD, 0x2F), "bad": (0xFB, 0x49, 0x34),
        "notice": (0xFE, 0x80, 0x19),
    },
    "dracula": {
        "separator": (0x44, 0x47, 0x5A), "dim": (0x62, 0x72, 0xA4),
        "muted": (0x8A, 0x93, 0xB8), "bright": (0xE0, 0xE0, 0xDC),
        "brightest": (0xF8, 0xF8, 0xF2), "accent_a": (0x8B, 0xE9, 0xFD),
        "accent_b": (0x8B, 0xE9, 0xFD), "accent_c": (0xBD, 0x93, 0xF9),
        "model": (0xFF, 0x79, 0xC6), "good": (0x50, 0xFA, 0x7B),
        "warn": (0xF1, 0xFA, 0x8C), "bad": (0xFF, 0x55, 0x55),
        "notice": (0xFF, 0xB8, 0x6C),
    },
    "one-dark": {
        "separator": (0x3E, 0x44, 0x51), "dim": (0x4B, 0x52, 0x63),
        "muted": (0x82, 0x89, 0x97), "bright": (0xAB, 0xB2, 0xBF),
        "brightest": (0xDC, 0xDF, 0xE4), "accent_a": (0x56, 0xB6, 0xC2),
        "accent_b": (0x61, 0xAF, 0xEF), "accent_c": (0x52, 0x8B, 0xFF),
        "model": (0xC6, 0x78, 0xDD), "good": (0x98, 0xC3, 0x79),
        "warn": (0xE5, 0xC0, 0x7B), "bad": (0xE0, 0x6C, 0x75),
        "notice": (0xD1, 0x9A, 0x66),
    },
}

DEFAULTS = {
    "theme": "nord",
    "line1": ["session", "model", "location", "pr", "modes"],
    "line2": ["context", "limits", "cost"],
    "bar_width": 16,
    "limit_bar_width": 6,
    "session_name_width": 24,
    "gap": "rule",
    "gap_char": "┈",
    "gap_scale": 1.0,
    "bottom_spacing": 1,
    "truecolor": None,
}

CUBE = (0, 95, 135, 175, 215, 255)


def parse_colour(v):
    """"#88C0D0", "#8cd" or [136, 192, 208]."""
    if isinstance(v, list) and len(v) == 3:
        try:
            return tuple(max(0, min(255, int(c))) for c in v)
        except (TypeError, ValueError):
            return None
    if not isinstance(v, str):
        return None
    s = v.strip().lstrip("#")
    try:
        if len(s) == 3:
            return tuple(int(c * 2, 16) for c in s)
        if len(s) == 6:
            return tuple(int(s[i : i + 2], 16) for i in (0, 2, 4))
    except ValueError:
        pass
    return None


def load_config():
    """Absent or malformed means defaults, never an error: a status line that
    disappears because of a stray comma is worse than one that ignores you."""
    cfg = dict(DEFAULTS)
    cfg["palette"] = dict(THEMES["nord"])

    home = os.environ.get("HOME")
    if not home:
        return cfg
    try:
        with open(os.path.join(home, ".claude", "statusline.json")) as handle:
            user = json.load(handle)
    except (OSError, ValueError):
        return cfg
    if not isinstance(user, dict):
        return cfg

    name = str(user.get("theme", "")).strip().lower()
    if name in THEMES:
        cfg["theme"] = name
        cfg["palette"] = dict(THEMES[name])

    for slot, raw in (user.get("colors") or {}).items():
        rgb = parse_colour(raw)
        if rgb and slot in cfg["palette"]:
            cfg["palette"][slot] = rgb

    for key in ("line1", "line2"):
        if isinstance(user.get(key), list):
            cfg[key] = [str(s).strip().lower() for s in user[key] if isinstance(s, str)]

    for key, floor in (
        ("bar_width", 1),
        ("limit_bar_width", 1),
        ("session_name_width", 2),
        ("bottom_spacing", 0),
    ):
        v = user.get(key)
        if isinstance(v, int) and not isinstance(v, bool) and v >= 0:
            cfg[key] = max(floor, v)

    if str(user.get("gap", "")).strip().lower() in ("rule", "blank", "none"):
        cfg["gap"] = user["gap"].strip().lower()
    if isinstance(user.get("gap_char"), str) and user["gap_char"]:
        cfg["gap_char"] = user["gap_char"]
    scale = user.get("gap_scale")
    if isinstance(scale, (int, float)) and not isinstance(scale, bool) and 0 < scale <= 1:
        cfg["gap_scale"] = float(scale)
    if isinstance(user.get("truecolor"), bool):
        cfg["truecolor"] = user["truecolor"]

    return cfg


CFG = load_config()
PALETTE = CFG["palette"]
TRUECOLOR = (
    CFG["truecolor"]
    if CFG["truecolor"] is not None
    else os.environ.get("COLORTERM") in ("truecolor", "24bit")
)

# Meter stops, derived so a new theme does not have to restate them.
GRADIENT = [
    (0.00, PALETTE["accent_b"]),
    (0.40, PALETTE["good"]),
    (0.70, PALETTE["warn"]),
    (1.00, PALETTE["bad"]),
]

PR_STATE = {
    "approved": ("good", "✓"),
    "changes_requested": ("bad", "✗"),
    "pending": ("warn", "●"),
    "draft": ("dim", "○"),
}


def to_256(rgb):
    """Nearest xterm-256 index: the 6x6x6 cube, or the grey ramp when the
    channels are close enough to neutral for grey to be a better match."""
    r, g, b = rgb
    ri, gi, bi = (min(range(6), key=lambda i: abs(v - CUBE[i])) for v in (r, g, b))
    cube_err = (r - CUBE[ri]) ** 2 + (g - CUBE[gi]) ** 2 + (b - CUBE[bi]) ** 2

    # Grey ramp is 232..255 at 8 + 10n.
    grey_i = max(0, min(23, round((sum(rgb) // 3 - 8) / 10)))
    grey_v = 8 + grey_i * 10
    grey_err = sum((c - grey_v) ** 2 for c in rgb)

    return 232 + grey_i if grey_err < cube_err else 16 + 36 * ri + 6 * gi + bi


def fg(rgb):
    if TRUECOLOR:
        return f"\033[38;2;{rgb[0]};{rgb[1]};{rgb[2]}m"
    return f"\033[38;5;{to_256(rgb)}m"


def paint(text, slot):
    return f"{fg(PALETTE[slot])}{text}{RESET}"


def display_width(text):
    """Visible column count, ignoring ANSI colour escapes."""
    return len(ANSI_RE.sub("", text))


def gradient_at(t):
    t = max(0.0, min(1.0, t))
    for (t0, c0), (t1, c1) in zip(GRADIENT, GRADIENT[1:]):
        if t0 <= t <= t1:
            span = t1 - t0
            f = 0.0 if span == 0 else (t - t0) / span
            return tuple(round(c0[i] + (c1[i] - c0[i]) * f) for i in range(3))
    return GRADIENT[-1][1]


def meter(pct, width):
    """Gradient bar accurate to an eighth of a cell."""
    eighths = max(0, min(width * 8, round(pct / 100 * width * 8)))
    full, remainder = divmod(eighths, 8)

    cells = [f"{fg(gradient_at((i + 0.5) / width))}█" for i in range(full)]
    filled = full

    if remainder and full < width:
        cells.append(f"{fg(gradient_at((full + 0.5) / width))}{EIGHTHS[remainder]}")
        filled += 1

    if filled < width:
        cells.append(f"{fg(PALETTE['dim'])}{TRACK * (width - filled)}")

    return "".join(cells) + RESET


def fmt_tokens(n):
    if n >= 1_000_000:
        return f"{n / 1_000_000:.1f}M".replace(".0M", "M")
    if n >= 1_000:
        return f"{round(n / 1_000)}k"
    return str(int(n))


def fmt_duration(seconds):
    """Countdown whose precision matches its horizon.

    Minutes matter on the 5h window, where you might wait it out. On the 7d
    window they are noise, and hours alone force a mental divide by 24.
    """
    if seconds <= 0:
        return "now"
    minutes = int(seconds) // 60
    hours, minutes = divmod(minutes, 60)
    days, hours = divmod(hours, 24)
    if days:
        return f"{days}d {hours}h"
    if hours:
        return f"{hours}h {minutes:02d}m"
    return f"{minutes}m"


def git_branch(cwd):
    """Current branch, read straight from .git/HEAD rather than forking git."""
    path = os.path.abspath(cwd) if cwd else ""
    if not path or not os.path.isdir(path):
        return None

    # Walk up to the repo. .git is a directory normally, a file in a worktree.
    while True:
        dotgit = os.path.join(path, ".git")
        if os.path.exists(dotgit):
            break
        parent = os.path.dirname(path)
        if parent == path:
            return None
        path = parent

    try:
        if os.path.isfile(dotgit):
            # Linked worktree: "gitdir: /abs/path/.git/worktrees/<name>"
            with open(dotgit) as handle:
                pointer = handle.read().strip()
            if not pointer.startswith("gitdir:"):
                return None
            dotgit = pointer.split(":", 1)[1].strip()
            if not os.path.isabs(dotgit):
                dotgit = os.path.join(path, dotgit)

        with open(os.path.join(dotgit, "HEAD")) as handle:
            head = handle.read().strip()
    except OSError:
        return None

    prefix = "ref: refs/heads/"
    if head.startswith(prefix):
        return head[len(prefix):] or None
    # Detached HEAD, mid-rebase or on a tag: a short SHA beats showing nothing.
    return head[:7] or None


def location(data, cwd):
    """Directory, worktree and branch, with repeated names collapsed.

    Worktrees are commonly created with the directory, worktree and branch all
    sharing one name, which would otherwise print that name three times.
    """
    place = os.path.basename(cwd.rstrip("/"))
    # git_worktree lives under workspace; worktree.name is top level.
    tree = (data.get("workspace") or {}).get("git_worktree") or (
        data.get("worktree") or {}
    ).get("name")
    branch = git_branch(cwd)

    if tree and tree == place:
        segments = [paint(f"⑂ {place}", "accent_b")]
    else:
        segments = [paint(place, "accent_b")] if place else []
        if tree:
            segments.append(paint(f"⑂ {tree}", "accent_a"))

    if branch and branch != place and branch != tree:
        segments.append(paint(f"⎇ {branch}", "accent_c"))

    return " ".join(segments)


def herdr_workspace_name():
    """Label of the herdr workspace this pane belongs to, if it has one.

    Only custom labels are returned. herdr falls back to the directory basename
    for unlabelled workspaces, which the location segment already shows.
    """
    workspace_id = os.environ.get("HERDR_WORKSPACE_ID")
    home = os.environ.get("HOME")
    if not workspace_id or not home:
        return None
    try:
        with open(os.path.join(home, ".config", "herdr", "session.json")) as handle:
            state = json.load(handle)
    except (OSError, ValueError):
        return None
    for workspace in state.get("workspaces") or []:
        if workspace.get("id") == workspace_id:
            return (workspace.get("custom_name") or "").strip() or None
    return None


def session_label(data):
    """Prefer the herdr workspace label, which is what its tab bar shows."""
    name = herdr_workspace_name() or (data.get("session_name") or "").strip()
    if not name:
        return None
    width = CFG["session_name_width"]
    if len(name) > width:
        name = name[: width - 1].rstrip() + "…"
    return name


SEP = None  # built lazily, needs fg()


def sep():
    global SEP
    if SEP is None:
        SEP = f" {fg(PALETTE['separator'])}│{RESET} "
    return SEP


def segment(name, data):
    """Renders one named segment, or None when it has no data to show."""
    if name == "session":
        label = session_label(data)
        return paint(label, "brightest") if label else None

    if name == "model":
        model = (data.get("model") or {}).get("display_name")
        return paint(model, "model") if model else None

    if name == "location":
        workspace = data.get("workspace") or {}
        cwd = workspace.get("current_dir") or data.get("cwd") or ""
        return location(data, cwd) or None

    if name == "pr":
        pr = data.get("pr") or {}
        if not pr.get("number"):
            return None
        slot, icon = PR_STATE.get(pr.get("review_state"), ("dim", ""))
        return paint(f"PR #{pr['number']}{' ' + icon if icon else ''}", slot)

    if name == "modes":
        modes = []
        effort = (data.get("effort") or {}).get("level")
        if effort:
            modes.append(paint(effort, "muted"))
        if data.get("fast_mode"):
            modes.append(paint("fast", "notice"))
        return " ".join(modes) if modes else None

    if name == "context":
        ctx = data.get("context_window") or {}
        pct = ctx.get("used_percentage")
        if pct is None:
            return None
        pct = int(pct)
        tip = fg(gradient_at(pct / 100))
        out = f"{meter(pct, CFG['bar_width'])} {tip}{pct}%{RESET}"
        # used_percentage counts input tokens only (input + cache create + cache
        # read), so the token figure uses the same basis rather than adding output.
        used = ctx.get("total_input_tokens") or 0
        size = ctx.get("context_window_size")
        if used and size:
            out += " " + paint(f"{fmt_tokens(used)}/{fmt_tokens(size)}", "muted")
        return out

    if name in ("limits", "limit_5h", "limit_7d"):
        wanted = {
            "limit_5h": [("five_hour", "5h")],
            "limit_7d": [("seven_day", "7d")],
        }.get(name, [("five_hour", "5h"), ("seven_day", "7d")])
        limits = data.get("rate_limits") or {}
        out = []
        for key, label in wanted:
            window = limits.get(key) or {}
            pct = window.get("used_percentage")
            if pct is None:
                continue
            pct = int(pct)
            tip = fg(gradient_at(pct / 100))
            seg = (
                f"{paint(label, 'muted')} {meter(pct, CFG['limit_bar_width'])} "
                f"{tip}{pct}%{RESET}"
            )
            resets_at = window.get("resets_at")
            if resets_at:
                seg += " " + paint(fmt_duration(resets_at - time.time()), "muted")
            out.append(seg)
        return sep().join(out) if out else None

    if name == "cost":
        cost = (data.get("cost") or {}).get("total_cost_usd")
        return paint(f"${cost:.2f}", "bright") if cost else None

    # Unknown names are ignored, so a typo costs a segment not the bar.
    return None


def build_line(names, data):
    parts = [segment(n, data) for n in names]
    return sep().join(p for p in parts if p)


def main():
    if "--themes" in sys.argv[1:]:
        print("\n".join(THEMES))
        return

    try:
        data = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        return
    if not isinstance(data, dict):
        return

    lines = [
        line
        for line in (build_line(CFG["line1"], data), build_line(CFG["line2"], data))
        if line
    ]
    if not lines:
        return

    if CFG["gap"] == "none":
        gap = []
    elif CFG["gap"] == "rule":
        span = max(display_width(line) for line in lines)
        width = max(1, round(span * CFG["gap_scale"]))
        gap = [f"{fg(PALETTE['separator'])}{CFG['gap_char'] * width}{RESET}"]
    else:
        gap = [BLANK_ROW]

    rows = []
    for line in lines:
        if rows:
            rows += gap
        rows.append(line)

    rows += [BLANK_ROW] * CFG["bottom_spacing"]
    print("\n".join(rows))


if __name__ == "__main__":
    main()
