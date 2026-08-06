# claude-statusline

A status line for [Claude Code](https://code.claude.com/docs/en/statusline). Six
themes, configurable segments, Nord by default.

```
my-feature │ Opus 5 (1M context) │ empirehomes.com ⎇ feature/promo-p3 │ PR #265 ✓ │ xhigh
┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈
████░░░░░░░░░░░░ 25% 254k/1M │ 5h ██▎░░░ 38% 2h 19m │ 7d ███▏░░ 52% 5d 1h │ $5.72
```

Row one is identity: session name, model, directory, git branch, open PR with
review state, reasoning effort. Row two is usage: how full the context window
is, your 5-hour and 7-day rate limits with countdowns to reset, and session cost.

Meters use a positional gradient and eighth-width block characters, so the fill
is accurate to an eighth of a column rather than rounding to whole cells.

Every segment disappears when its data is absent, so early in a session you get
just the model and directory rather than a row of zeroes.

## Install

```sh
gh repo clone JordPlamondon/claude-statusline
cd claude-statusline
./install.sh
```

It applies on your next interaction. No restart.

The installer merges into `~/.claude/settings.json`, so your hooks, model and
permissions are left alone. **Nothing here installs a toolchain for you.**

## Which version

Three ways in, same output from all of them. `install.sh` picks the first that
works, or you can name one.

| | needs | CPU per render |
|---|---|---|
| `./install.sh binary` | `gh`, and repo access | **2.3 ms** |
| `./install.sh rust` | cargo, about 1.5 GB of toolchain | **2.3 ms** |
| `./install.sh python` | nothing | 107 ms |

The released binary is universal, so it runs on Apple Silicon and Intel, and it
ships with a SHA-256 that the installer checks before putting it in place.

### Is the speed worth caring about

Measured on an M-series Mac against the eleven-line bash example from the Claude
Code docs, extended with a token count, so that both show comparable
information. CPU time per render, median of repeated runs:

| | CPU per render |
|---|---|
| docs example (bash + 4 jq) | ~98 ms |
| this, python | ~107 ms |
| this, rust | **~2.3 ms** |

So the Rust build is **40x cheaper**, and the Python build is not faster than
the docs example at all. Be honest with yourself about which you are installing.

The reason is process count. Each `jq` in a shell status line is another
process, so cost grows as you add fields. This parses the payload once, so it
does not.

None of this is perceptible at one session. The status line re-runs on every
event, debounced at 300 ms, so it only compounds when several Claude Code
sessions are working at once. At ten concurrent sessions it is the difference
between 10% and 80% of a CPU core.

Install Python if you want the features. Install the binary if you also want the
speed. Both take one command.

## What it reads

- stdin, the JSON payload Claude Code provides
- `~/.claude/statusline.json`, your config, if you make one
- `<repo>/.git/HEAD`, to show the branch without forking `git`
- `~/.config/herdr/session.json`, only if `$HERDR_WORKSPACE_ID` is set

It writes to stdout and nothing else. No network, no subprocesses, no
credentials.

## herdr is optional

If you use [herdr](https://herdr.dev), the workspace label from its tab bar
leads the row, which is what tells panes apart when several are open on the same
project. If you do not, that slot falls back to Claude Code's own session name,
set with `--name` or `/rename`. If neither exists the segment is omitted.

Nothing to configure either way.

## Terminal support

24-bit colour when `COLORTERM` is `truecolor` or `24bit`, and a 256-colour
approximation otherwise. The glyphs (`⎇ ⑂ ┈ ░ ▏▎▍▌▋▊▉█`) are standard Unicode,
not Nerd Font, so no patched font is needed.

## Config

Optional. Put a `~/.claude/statusline.json` in place and it is picked up on the
next render. Every key is optional, and anything invalid is ignored rather than
fatal, so a stray comma costs you a setting and not the whole bar.

```json
{
  "theme": "catppuccin",
  "line1": ["session", "model", "location", "pr", "modes"],
  "line2": ["context", "limits", "cost"],
  "bar_width": 16,
  "limit_bar_width": 6,
  "session_name_width": 24,
  "gap": "rule",
  "gap_char": "┈",
  "gap_scale": 1.0,
  "bottom_spacing": 1
}
```

The values above are the defaults, so an empty `{}` behaves the same as no file.

**Segments** are named, and the arrays control both what appears and in what
order. Drop one by leaving it out, move it to the other row, or reorder freely.

| | |
|---|---|
| `session` | herdr workspace label, else Claude Code's session name |
| `model` | model display name |
| `location` | directory, worktree, git branch |
| `pr` | open PR number and review state |
| `modes` | reasoning effort, and `fast` when fast mode is on |
| `context` | context meter, percentage, token counts |
| `limits` | 5h and 7d rate limits together |
| `limit_5h`, `limit_7d` | either one on its own |
| `cost` | session cost |

**Themes**: `nord`, `catppuccin`, `tokyo-night`, `gruvbox`, `dracula`,
`one-dark`. List them with `--themes`.

**Individual colours** override whichever theme is active. Accepts `"#88C0D0"`,
`"#8cd"` or `[136, 192, 208]`.

```json
{ "theme": "gruvbox", "colors": { "brightest": "#ff00ff", "dim": [70, 70, 80] } }
```

| slot | used for |
|---|---|
| `separator` | the `│` dividers and the rule between rows |
| `dim` | unfilled meter track, PR draft |
| `muted` | token counts, countdowns, effort |
| `bright` | session cost |
| `brightest` | session name |
| `accent_a` | worktree marker |
| `accent_b` | directory, and the low end of the meter gradient |
| `accent_c` | git branch |
| `model` | model name |
| `good`, `warn`, `bad` | PR states, and the upper gradient stops |
| `notice` | fast mode |

The meter gradient is derived from `accent_b`, `good`, `warn` and `bad`, so
changing those changes the meters too.

**`truecolor`** forces 24-bit on or off. Omit it to detect from `COLORTERM`.

Try a theme before committing to it:

```sh
statusline --preview            all six, with sample data
statusline --preview gruvbox    just that one
statusline --init               write a commented config with every default
```

Rebuild after editing the Rust source:

```sh
cd rust && cargo build --release
```

The binary does not rebuild itself, so an edit does nothing until you run that.
Config changes need no rebuild, since the file is read at each render.

## Cutting a release

```sh
./scripts/build-release.sh v1.0.1
```

Builds both Apple targets, merges them with `lipo`, writes a checksum, smoke
tests the result and uploads it. Needs `rustup target add x86_64-apple-darwin`
once.

## After changing output

```sh
python3 scripts/parity.py                    # 256-colour path
COLORTERM=truecolor python3 scripts/parity.py  # 24-bit path
```

161 cases: every whole percent through the meters, token and duration
boundaries, all PR states, worktrees, missing and malformed input. It compares
raw bytes across whichever implementations are present.

This is worth running. It caught a currency bug that only appeared at
`$0.005`, where scaling by 100 before rounding lands on exactly `0.5` in
floating point and loses the tie-break.

## Notes

`refreshInterval` is seconds, minimum 1, and it applies to every session whether
or not anything on screen is time-based. 60 matches the finest thing displayed,
which is the rate-limit countdown in minutes. Lower values cost real CPU when
several sessions are open.

Negative `padding` in `settings.json` shifts the bar left but the renderer does
not account for it, so the leftmost character is clipped until a keystroke
forces a repaint. Leave it at 0.
