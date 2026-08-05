# claude-statusline

A status line for [Claude Code](https://code.claude.com/docs/en/statusline), in
the Nord palette.

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

That builds the Rust binary if you have `cargo`, and falls back to Python if you
do not. Force either with `./install.sh python` or `./install.sh rust`.

The installer merges into `~/.claude/settings.json`. Your hooks, model and
permissions are left alone.

It applies on your next interaction. No restart.

## Which version

Three implementations, byte-identical output, verified by `scripts/parity.py`.

| | needs | CPU per render |
|---|---|---|
| `rust/` | cargo | 3 ms |
| `scripts/statusline.js` | bun | 24 ms |
| `scripts/statusline.py` | python3 only | 114 ms |

**Python is fine for most people.** The status line re-runs on every event, so
the cost only matters if you run several Claude Code sessions at once, where it
multiplies. At ten concurrent sessions that is the difference between 10% and
80% of a CPU core.

The Rust binary is not distributed prebuilt. Build it yourself from source in
about twenty seconds, which is also how you know what you are running.

## What it reads

- stdin, the JSON payload Claude Code provides
- `<repo>/.git/HEAD`, to show the branch without forking `git`
- `~/.config/herdr/session.json`, only if `$HERDR_WORKSPACE_ID` is set

It writes to stdout and nothing else. No network, no subprocesses, no
credentials, no config files of its own.

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

## Customising

Constants at the top of each implementation, same names in all three:

| | |
|---|---|
| `BAR_WIDTH`, `LIMIT_BAR_WIDTH` | meter widths in columns |
| `GRADIENT` | colour stops the meters interpolate between |
| `GAP_STYLE` | `rule`, `blank` or `none` between the two rows |
| `BOTTOM_SPACING` | blank rows below, to clear the mode indicator |
| `SESSION_NAME_WIDTH` | truncation point for the leading label |

Rebuild after editing Rust:

```sh
cd rust && cargo build --release
```

The binary does not rebuild itself, so an edit does nothing until you run that.

## After changing output

```sh
python3 scripts/parity.py                    # 256-colour path
COLORTERM=truecolor python3 scripts/parity.py  # 24-bit path
```

165 cases: every whole percent through the meters, token and duration
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
