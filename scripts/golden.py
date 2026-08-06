#!/usr/bin/env python3
"""Golden-file regression test.

    python3 scripts/golden.py          check the binary against tests/golden.txt
    python3 scripts/golden.py --update rewrite the golden file

Every case in scripts/cases.py is run through the binary and the raw bytes are
compared to a stored expectation. Unlike comparing two implementations, this
catches a change even when it is the only implementation, and it pins behaviour
that has already been reviewed rather than blessing whatever the code does now.

Both colour depths are covered, since the 256-colour fallback has its own
rounding. Time is pinned via $STATUSLINE_NOW so countdowns are deterministic.
"""

import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cases import CASES, CONFIG_CASES, CONFIG_PAYLOAD, FIXED_NOW  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GOLDEN = os.path.join(ROOT, "tests", "golden.txt")

# Locally built first. bin/ holds a downloaded release, which is by definition
# older than whatever you are about to change, and testing that instead is a
# confusing way to discover your edit had no effect.
BINARIES = [
    os.path.join(ROOT, "rust", "target", "release", "statusline"),
    os.path.join(ROOT, "bin", "statusline"),
]


def find_binary():
    # $STATUSLINE_BIN points the test at something else. Used once, to record
    # the first golden file from the reviewed Python implementation before it
    # was removed, so the expectations were not simply whatever Rust did.
    override = os.environ.get("STATUSLINE_BIN")
    if override:
        return override.split()
    for path in BINARIES:
        if os.path.exists(path):
            return path
    sys.exit(
        "No binary found. Build one with:\n"
        "  cd rust && cargo build --release\n"
        "or download one with:\n"
        "  ./install.sh binary"
    )


def run(binary, stdin, env, home):
    full = dict(os.environ)
    # A stable environment, so the output cannot depend on the machine.
    full.pop("HERDR_WORKSPACE_ID", None)
    full["HOME"] = home
    full["STATUSLINE_NOW"] = str(FIXED_NOW)
    full.update(env)
    cmd = binary if isinstance(binary, list) else [binary]
    return subprocess.run(
        cmd, input=stdin.encode(), capture_output=True, env=full
    ).stdout


def collect(binary, workdir):
    """Every case under both colour depths, as (label, output) pairs."""
    plain_home = os.path.join(workdir, "plain")
    cfg_home = os.path.join(workdir, "configured")
    os.makedirs(os.path.join(plain_home, ".claude"), exist_ok=True)
    os.makedirs(os.path.join(cfg_home, ".claude"), exist_ok=True)

    results = []
    for depth, colorterm in (("24bit", "truecolor"), ("256", None)):
        env_depth = {"COLORTERM": colorterm} if colorterm else {"COLORTERM": ""}

        for c in CASES:
            env = dict(env_depth)
            env.update(c["env"])
            out = run(binary, c["stdin"], env, plain_home)
            results.append((f"{depth} | default | {c['name']}", out))

        for name, cfg in CONFIG_CASES:
            path = os.path.join(cfg_home, ".claude", "statusline.json")
            with open(path, "w") as fh:
                json.dump(cfg, fh)
            out = run(binary, json.dumps(CONFIG_PAYLOAD), dict(env_depth), cfg_home)
            results.append((f"{depth} | config | {name}", out))

    return results


def serialise(results):
    lines = []
    for label, out in results:
        lines.append(f"### {label}")
        # Escaped so the file stays diffable and greppable in a normal editor.
        lines.append(out.decode("utf-8", "replace").replace("\x1b", "\\e").rstrip("\n"))
        lines.append("")
    return "\n".join(lines) + "\n"


def main():
    update = "--update" in sys.argv[1:]
    binary = find_binary()
    shown = " ".join(binary) if isinstance(binary, list) else binary
    print(f"testing: {os.path.relpath(shown.split()[-1], ROOT)}")

    import tempfile

    with tempfile.TemporaryDirectory() as workdir:
        results = collect(binary, workdir)

    text = serialise(results)

    if update:
        os.makedirs(os.path.dirname(GOLDEN), exist_ok=True)
        with open(GOLDEN, "w") as fh:
            fh.write(text)
        print(f"wrote {len(results)} cases to {os.path.relpath(GOLDEN, ROOT)}")
        return 0

    if not os.path.exists(GOLDEN):
        sys.exit(f"{GOLDEN} does not exist. Create it with --update.")

    with open(GOLDEN) as fh:
        expected = fh.read()

    if text == expected:
        print(f"{len(results)}/{len(results)} match")
        return 0

    # Report which cases changed, not a wall of diff.
    def blocks(s):
        out, label = {}, None
        for line in s.split("\n"):
            if line.startswith("### "):
                label = line[4:]
                out[label] = []
            elif label is not None:
                out[label].append(line)
        return out

    got, want = blocks(text), blocks(expected)
    changed = [k for k in want if k in got and got[k] != want[k]]
    added = [k for k in got if k not in want]
    removed = [k for k in want if k not in got]

    total = len(results)
    print(f"{total - len(changed) - len(added)}/{total} match")
    for k in changed[:10]:
        print(f"\n  CHANGED: {k}")
        print(f"    want: {' '.join(want[k])[:150]}")
        print(f"    got:  {' '.join(got[k])[:150]}")
    if len(changed) > 10:
        print(f"\n  ... and {len(changed) - 10} more changed")
    for k in added[:5]:
        print(f"  NEW CASE: {k}")
    for k in removed[:5]:
        print(f"  MISSING CASE: {k}")
    print("\nIf the change is intended, re-record with: python3 scripts/golden.py --update")
    return 1


sys.exit(main())
