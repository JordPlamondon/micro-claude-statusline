#!/usr/bin/env python3
"""Golden-file regression test.

    python3 scripts/golden.py          check the binary against tests/golden.txt
    python3 scripts/golden.py --update rewrite the golden file

Every case in scripts/cases.py is run through the binary and the raw bytes are
compared to a stored expectation. Unlike comparing two implementations, this
catches a change even when it is the only implementation, and it pins behaviour
that has already been reviewed rather than blessing whatever the code does now.

Both colour depths are covered, since the 256-colour fallback has its own
rounding. Time is pinned via $STATUSLINE_TEST_PINNED_CLOCK so countdowns are deterministic.
"""

import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cases import (  # noqa: E402
    CASES,
    CONFIG_CASES,
    CONFIG_PAYLOAD,
    FIXED_NOW,
    WIDE_PAYLOAD,
    WIDTH_CASES,
)

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


def run(binary, stdin, env, home, args=()):
    full = dict(os.environ)
    # A stable environment, so the output cannot depend on the machine.
    full.pop("HERDR_WORKSPACE_ID", None)
    full["HOME"] = home
    full["STATUSLINE_TEST_PINNED_CLOCK"] = str(FIXED_NOW)
    # Pinned, not inherited: some shells export COLUMNS, which would make every
    # recorded case depend on the width of whatever terminal ran the test.
    # Wide enough that nothing sheds unless a case sets it deliberately.
    full["COLUMNS"] = "999"
    full.update(env)
    cmd = (binary if isinstance(binary, list) else [binary]) + list(args)
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

        # --init tells the user every value it writes is already the default.
        # Rendering with what it writes must therefore produce what no config
        # at all produces. Recording it means a template that drifts from
        # Config::default() shows up here instead of quietly lying to whoever
        # ran --init and believed it.
        init_home = os.path.join(workdir, "initialised")
        os.makedirs(os.path.join(init_home, ".claude"), exist_ok=True)
        written = os.path.join(init_home, ".claude", "statusline.json")
        if os.path.exists(written):
            os.remove(written)
        run(binary, "", dict(env_depth), init_home, args=["--init"])
        out = run(binary, json.dumps(CONFIG_PAYLOAD), dict(env_depth), init_home)
        results.append((f"{depth} | config | --init template", out))

        # Narrow terminals, where segments are shed rather than the row wrapping.
        # Twice: once in ASCII, once with a session name whose characters are
        # two columns wide, since counting those as one column was the whole
        # bug and only the second sweep can catch it coming back.
        for label, payload in (("", CONFIG_PAYLOAD), (" wide", WIDE_PAYLOAD)):
            for cols in WIDTH_CASES:
                env = dict(env_depth)
                env["COLUMNS"] = str(cols)
                out = run(binary, json.dumps(payload), env, plain_home)
                results.append((f"{depth} | width {cols}{label} | shedding", out))

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
