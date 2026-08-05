#!/usr/bin/env python3
"""Claude Code status line, Nord palette.

Line 1: session name, model, project, branch/worktree, open PR, effort mode
Line 2: context usage, 5h + 7d rate limits, session cost

Meters use a positional gradient (frost teal to aurora red) and eighth-width
block characters, so the fill is accurate to 1/8 of a cell.
"""

import json
import os
import re
import sys
import time

ANSI_RE = re.compile(r"\033\[[0-9;]*m")

RESET = "\033[0m"

# Nord palette, https://www.nordtheme.com/docs/colors-and-palettes
NORD = {
    "polar2": (0x43, 0x4C, 0x5E),
    "polar3": (0x4C, 0x56, 0x6A),
    "snow4": (0xD8, 0xDE, 0xE9),
    "snow6": (0xEC, 0xEF, 0xF4),
    # Not a Nord token. Sits between polar3 and snow4, which have a large gap
    # with nothing between them, for secondary text that still needs reading.
    "muted": (0x7D, 0x86, 0x96),
    "frost7": (0x8F, 0xBC, 0xBB),
    "frost8": (0x88, 0xC0, 0xD0),
    "frost9": (0x81, 0xA1, 0xC1),
    "red": (0xBF, 0x61, 0x6A),
    "orange": (0xD0, 0x87, 0x70),
    "yellow": (0xEB, 0xCB, 0x8B),
    "green": (0xA3, 0xBE, 0x8C),
    "purple": (0xB4, 0x8E, 0xAD),
}

# Meter gradient: position along the bar maps to colour.
GRADIENT = [
    (0.00, NORD["frost8"]),
    (0.40, NORD["green"]),
    (0.70, NORD["yellow"]),
    (1.00, NORD["red"]),
]

EIGHTHS = " ▏▎▍▌▋▊▉█"
TRACK = "░"

BAR_WIDTH = 16
LIMIT_BAR_WIDTH = 6

# Session name, truncated. AI-generated titles run long, and this leads the row.
SESSION_NAME_WIDTH = 24

# herdr writes live workspace state here, including the label shown in its tab
# bar. Reading it costs ~0.1 ms, against ~16 ms to ask the herdr server.
HERDR_SESSION = os.path.expanduser("~/.config/herdr/session.json")

# U+2800 renders blank but is not whitespace, so it survives trimming.
BLANK_ROW = "⠀"

# Gap between the identity and usage rows. A terminal row is the smallest unit,
# so there is no half-height gap; a faint rule reads smaller than empty space.
#   "rule"  — dim dotted line, the tightest-looking gap
#   "blank" — fully empty row, the largest gap
#   "none"  — rows touch
GAP_STYLE = "rule"

# Rule character, and its width as a fraction of the widest row. 1.0 spans fully.
GAP_RULE_CHAR = "┈"
GAP_RULE_SCALE = 1.0

# Blank rows printed below the bar, to keep it off the mode indicator. 0 disables.
BOTTOM_SPACING = 1

PR_STATE = {
    "approved": ("green", "✓"),
    "changes_requested": ("red", "✗"),
    "pending": ("yellow", "●"),
    "draft": ("polar3", "○"),
}


CUBE = (0, 95, 135, 175, 215, 255)

# True when the terminal advertises 24-bit colour. Everything else gets the
# 256-colour approximation, which every terminal worth using supports.
TRUECOLOR = os.environ.get("COLORTERM") in ("truecolor", "24bit")


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


def display_width(text):
    """Visible column count, ignoring ANSI colour escapes."""
    return len(ANSI_RE.sub("", text))


def paint(text, key):
    return f"{fg(NORD[key])}{text}{RESET}"


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
        cells.append(f"{fg(NORD['polar3'])}{TRACK * (width - filled)}")

    return "".join(cells) + RESET


def fmt_tokens(n):
    if n >= 1_000_000:
        return f"{n / 1_000_000:.1f}M".replace(".0M", "M")
    if n >= 1_000:
        return f"{round(n / 1_000)}k"
    return str(n)


def fmt_duration(seconds):
    """Countdown whose precision matches its horizon.

    Minutes matter on the 5h window, where you might wait it out. On the 7d
    window they are noise, and hours alone force a mental divide by 24.
    Spaces between units because "121h25m" runs together at a glance.
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
    """Current branch, read straight from .git/HEAD.

    Forking `git` dominated this script's runtime, and the status line re-renders
    on every event in every open session. Two file reads cost a fraction of that,
    and unlike a cache they can never go stale.
    """
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
        # Directory is the worktree; mark it once instead of naming it twice.
        segments = [paint(f"⑂ {place}", "frost8")]
    else:
        segments = [paint(place, "frost8")] if place else []
        if tree:
            segments.append(paint(f"⑂ {tree}", "frost7"))

    if branch and branch != place and branch != tree:
        segments.append(paint(f"⎇ {branch}", "frost9"))

    return " ".join(segments)


def herdr_workspace_name():
    """Label of the herdr workspace this pane belongs to, if it has one.

    Only custom labels are returned. herdr falls back to the directory basename
    for unlabelled workspaces, which the location segment already shows.
    """
    workspace_id = os.environ.get("HERDR_WORKSPACE_ID")
    if not workspace_id:
        return None
    try:
        with open(HERDR_SESSION) as handle:
            state = json.load(handle)
    except (OSError, ValueError):
        # Absent, unreadable, or caught mid-write by herdr.
        return None
    for workspace in state.get("workspaces") or []:
        if workspace.get("id") == workspace_id:
            return (workspace.get("custom_name") or "").strip() or None
    return None


def session_label(data):
    """Prefer the herdr workspace label, which is what the tab bar shows."""
    name = herdr_workspace_name() or (data.get("session_name") or "").strip()
    if not name:
        return None
    if len(name) > SESSION_NAME_WIDTH:
        name = name[: SESSION_NAME_WIDTH - 1].rstrip() + "…"
    return name


def join(parts):
    return f" {fg(NORD['polar2'])}│{RESET} ".join(p for p in parts if p)


def identity_line(data):
    parts = []

    # Leads the row: it is the only segment that differs between panes on the
    # same project, so it is what the eye needs first when scanning tabs.
    name = session_label(data)
    if name:
        parts.append(paint(name, "snow6"))

    model = data.get("model", {}).get("display_name")
    if model:
        parts.append(paint(model, "purple"))

    workspace = data.get("workspace") or {}
    cwd = workspace.get("current_dir") or data.get("cwd") or ""

    parts.append(location(data, cwd))

    pr = data.get("pr") or {}
    if pr.get("number"):
        key, icon = PR_STATE.get(pr.get("review_state"), ("polar3", ""))
        parts.append(paint(f"PR #{pr['number']}{' ' + icon if icon else ''}", key))

    modes = []
    effort = (data.get("effort") or {}).get("level")
    if effort:
        modes.append(paint(effort, "muted"))
    if data.get("fast_mode"):
        modes.append(paint("fast", "orange"))
    if modes:
        parts.append(" ".join(modes))

    return join(parts)


def usage_line(data):
    parts = []

    ctx = data.get("context_window") or {}
    pct = ctx.get("used_percentage")
    if pct is not None:
        pct = int(pct)
        tip = fg(gradient_at(pct / 100))
        segment = f"{meter(pct, BAR_WIDTH)} {tip}{pct}%{RESET}"
        # used_percentage counts input tokens only (input + cache create + cache
        # read), so the token figure uses the same basis rather than adding output.
        used = ctx.get("total_input_tokens") or 0
        size = ctx.get("context_window_size")
        if used and size:
            segment += " " + paint(f"{fmt_tokens(used)}/{fmt_tokens(size)}", "muted")
        parts.append(segment)

    limits = data.get("rate_limits") or {}
    for key, label in (("five_hour", "5h"), ("seven_day", "7d")):
        window = limits.get(key) or {}
        limit_pct = window.get("used_percentage")
        if limit_pct is None:
            continue
        limit_pct = int(limit_pct)
        tip = fg(gradient_at(limit_pct / 100))
        segment = (
            f"{paint(label, 'muted')} {meter(limit_pct, LIMIT_BAR_WIDTH)} "
            f"{tip}{limit_pct}%{RESET}"
        )
        resets_at = window.get("resets_at")
        if resets_at:
            segment += " " + paint(fmt_duration(resets_at - time.time()), "muted")
        parts.append(segment)

    cost = (data.get("cost") or {}).get("total_cost_usd")
    if cost:
        parts.append(paint(f"${cost:.2f}", "snow4"))

    return join(parts)


def main():
    try:
        data = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        return

    lines = [line for line in (identity_line(data), usage_line(data)) if line]
    if not lines:
        return

    if GAP_STYLE == "none":
        gap = []
    elif GAP_STYLE == "rule":
        span = max(display_width(line) for line in lines)
        width = max(1, round(span * GAP_RULE_SCALE))
        gap = [f"{fg(NORD['polar2'])}{GAP_RULE_CHAR * width}{RESET}"]
    else:
        gap = [BLANK_ROW]

    rows = []
    for line in lines:
        if rows:
            rows += gap
        rows.append(line)

    rows += [BLANK_ROW] * BOTTOM_SPACING
    print("\n".join(rows))


if __name__ == "__main__":
    main()
