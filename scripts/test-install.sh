#!/bin/sh
# Exercises install.sh against a scratch HOME.
#
#     sh scripts/test-install.sh
#
# The installer is the first thing anyone else runs and was the only part of
# this repo with no coverage, while the renderer has 432 golden cases and 1,500
# fuzz inputs. That is backwards on risk. A renderer bug is an ugly bar; an
# installer bug is a half-install that edits settings.json and then stops, which
# has already happened once.
#
# Never touches the real HOME. Everything lives under a temp directory that goes
# away on exit. Builds from source, so no network and no release required.

set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

command -v cargo >/dev/null 2>&1 || {
  echo "cargo is needed: these tests build from source so they need no network." >&2
  exit 1
}

# rustup keeps its toolchains under the real HOME and these tests move HOME, so
# without this cargo cannot find a default toolchain and every case fails for a
# reason that has nothing to do with the installer.
: "${RUSTUP_HOME:=$HOME/.rustup}"
: "${CARGO_HOME:=$HOME/.cargo}"
export RUSTUP_HOME CARGO_HOME

PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); printf '  ok    %s\n' "$1"; }
no() { FAIL=$((FAIL + 1)); printf '  FAIL  %s\n' "$1"; }

# assert <message> <command...>   command must succeed
assert() { m=$1; shift; if "$@" >/dev/null 2>&1; then ok "$m"; else no "$m"; fi; }
# refute <message> <command...>   command must fail
refute() { m=$1; shift; if "$@" >/dev/null 2>&1; then no "$m"; else ok "$m"; fi; }

fresh() { h="$WORK/$1"; rm -rf "$h"; mkdir -p "$h/.claude"; echo "$h"; }
# mode <file> <octal>   permission bits must match; python3 is already required
mode() { [ "$(python3 -c 'import os,stat,sys; print(oct(stat.S_IMODE(os.stat(sys.argv[1]).st_mode))[2:])' "$1")" = "$2" ]; }
installer() { HOME="$1" PATH="$1/.local/bin:$PATH" sh "$ROOT/install.sh" "$2"; }

SETTINGS='{"model":"opus","hooks":{"PreToolUse":[]},"permissions":{"allow":["Bash(ls:*)"]}}'

echo "install"
H=$(fresh install)
printf '%s' "$SETTINGS" > "$H/.claude/settings.json"
chmod 600 "$H/.claude/settings.json"
installer "$H" rust >/dev/null 2>&1
assert "puts the binary in ~/.claude/bin"      test -x "$H/.claude/bin/statusline"
assert "keeps a 0600 settings.json at 0600"    mode "$H/.claude/settings.json" 600
assert "links it onto PATH"                    test -L "$H/.local/bin/statusline"
assert "writes the statusLine block"           grep -q '"statusLine"' "$H/.claude/settings.json"
assert "leaves hooks alone"                    grep -q '"hooks"' "$H/.claude/settings.json"
assert "leaves permissions alone"              grep -q '"permissions"' "$H/.claude/settings.json"
assert "the installed binary runs"             "$H/.claude/bin/statusline" --version

echo "install again, the upgrade path"
assert "re-running is not an error"            installer "$H" rust
assert "still linked afterwards"               test -L "$H/.local/bin/statusline"

echo "uninstall"
echo '{"theme":"gruvbox"}' > "$H/.claude/statusline.json"
HOME="$H" sh "$ROOT/install.sh" --uninstall >/dev/null 2>&1
refute "removes the binary"                    test -e "$H/.claude/bin/statusline"
refute "removes the symlink"                   test -e "$H/.local/bin/statusline"
refute "drops the statusLine block"            grep -q '"statusLine"' "$H/.claude/settings.json"
assert "keeps the rest of settings.json"       grep -q '"permissions"' "$H/.claude/settings.json"
assert "still 0600 after uninstall"            mode "$H/.claude/settings.json" 600
assert "keeps your statusline.json"            test -f "$H/.claude/statusline.json"
assert "running it twice is not an error"      env HOME="$H" sh "$ROOT/install.sh" --uninstall

echo "settings.json permissions and symlinks"
H=$(fresh newsettings)
installer "$H" rust >/dev/null 2>&1
assert "a new settings.json is created 0600"   mode "$H/.claude/settings.json" 600

H=$(fresh symlinked)
mkdir -p "$H/dotfiles"
printf '%s' "$SETTINGS" > "$H/dotfiles/settings.json"
ln -s "$H/dotfiles/settings.json" "$H/.claude/settings.json"
installer "$H" rust >/dev/null 2>&1
assert "a symlinked settings.json stays a symlink" test -L "$H/.claude/settings.json"
assert "  and the target gets the block"       grep -q '"statusLine"' "$H/dotfiles/settings.json"
refute "  and no temp file is left behind"     sh -c "ls -A '$H/dotfiles' '$H/.claude' | grep -q '\.tmp$'"

echo "uninstall refuses to overreach"
H=$(fresh other)
printf '%s' '{"statusLine":{"type":"command","command":"/usr/local/bin/other-bar"},"model":"opus"}' \
  > "$H/.claude/settings.json"
HOME="$H" sh "$ROOT/install.sh" --uninstall >/dev/null 2>&1
assert "leaves a status line pointing elsewhere" grep -q 'other-bar' "$H/.claude/settings.json"

H=$(fresh regularfile)
mkdir -p "$H/.local/bin"
echo 'someone elses script' > "$H/.local/bin/statusline"
echo '{}' > "$H/.claude/settings.json"
HOME="$H" sh "$ROOT/install.sh" --uninstall >/dev/null 2>&1
assert "does not delete a real file at the link path" test -f "$H/.local/bin/statusline"

echo "uninstall on odd input"
H=$(fresh nosettings)
assert "no settings.json at all is fine"       env HOME="$H" sh "$ROOT/install.sh" --uninstall

H=$(fresh malformed)
mkdir -p "$H/.claude/bin"
cp "$ROOT/rust/target/release/statusline" "$H/.claude/bin/statusline"
printf '%s' '{"model": "opus",,,' > "$H/.claude/settings.json"
refute "malformed settings.json is refused"    env HOME="$H" sh "$ROOT/install.sh" --uninstall
assert "  and nothing is removed"              test -e "$H/.claude/bin/statusline"
assert "  and the file is left as it was"      grep -q ',,,' "$H/.claude/settings.json"

echo "bad usage"
refute "an unknown argument is an error"       env HOME="$H" sh "$ROOT/install.sh" --nonsense

H=$(fresh noclaude)
rm -rf "$H/.claude"
refute "no ~/.claude is an error"              env HOME="$H" sh "$ROOT/install.sh" rust

echo
if [ "$FAIL" -eq 0 ]; then
  echo "$PASS/$PASS passed"
else
  echo "$PASS passed, $FAIL failed"
  exit 1
fi
