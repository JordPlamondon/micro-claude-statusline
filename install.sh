#!/bin/sh
# Installs the Claude Code status line.
#
#   ./install.sh            download the prebuilt binary, else build, else Python
#   ./install.sh binary     download the released universal binary (needs gh)
#   ./install.sh rust       build from source (needs cargo)
#   ./install.sh python     no toolchain at all
#
# Nothing here installs a toolchain for you. If cargo is missing the installer
# picks another path rather than fetching Rust behind your back.
#
# Writes the statusLine block into ~/.claude/settings.json and leaves every
# other setting untouched.

set -eu

REPO="JordPlamondon/claude-statusline"
ROOT=$(cd "$(dirname "$0")" && pwd)
SETTINGS="${HOME}/.claude/settings.json"
choice="${1:-auto}"

if [ ! -d "${HOME}/.claude" ]; then
  echo "No ~/.claude directory. Is Claude Code installed?" >&2
  exit 1
fi

command -v python3 >/dev/null 2>&1 || {
  echo "python3 is required, at least to write the settings file." >&2
  exit 1
}

have_release() {
  command -v gh >/dev/null 2>&1 || return 1
  gh release view --repo "$REPO" >/dev/null 2>&1
}

case "$choice" in
  auto)
    if have_release; then
      choice=binary
    elif command -v cargo >/dev/null 2>&1; then
      # Worth saying out loud: a gh that cannot see the repo is usually an auth
      # or access problem, not an absent release, and silently building from
      # source hides that.
      if command -v gh >/dev/null 2>&1; then
        echo "gh cannot read $REPO. Check 'gh auth status' and that you have"
        echo "access to the repo. Falling back to building from source."
      fi
      choice=rust
    else
      if command -v gh >/dev/null 2>&1; then
        echo "gh cannot read $REPO, and cargo is not installed."
      fi
      choice=python
    fi
    ;;
  binary|python|rust) ;;
  *) echo "usage: $0 [binary|rust|python]" >&2; exit 1 ;;
esac

if [ "$choice" = binary ]; then
  command -v gh >/dev/null 2>&1 || {
    echo "gh not found. Install it, or run: $0 python" >&2
    exit 1
  }
  DEST="$ROOT/bin"
  mkdir -p "$DEST"
  echo "Downloading the prebuilt universal binary."
  gh release download --repo "$REPO" --pattern statusline --dir "$DEST" --clobber
  gh release download --repo "$REPO" --pattern statusline.sha256 --dir "$DEST" --clobber 2>/dev/null || true
  if [ -f "$DEST/statusline.sha256" ]; then
    ( cd "$DEST" && shasum -a 256 -c statusline.sha256 ) || {
      echo "Checksum did not match. Refusing to install." >&2
      exit 1
    }
  fi
  chmod +x "$DEST/statusline"
  COMMAND="$DEST/statusline"
elif [ "$choice" = rust ]; then
  command -v cargo >/dev/null 2>&1 || {
    echo "cargo not found. Install Rust from https://rustup.rs, or run: $0 python" >&2
    exit 1
  }
  echo "Building from source. About twenty seconds the first time."
  ( cd "$ROOT/rust" && cargo build --release --quiet )
  COMMAND="$ROOT/rust/target/release/statusline"
else
  chmod +x "$ROOT/scripts/statusline.py"
  COMMAND="$ROOT/scripts/statusline.py"
fi

# Merge rather than overwrite: this file holds hooks, model and permissions too.
python3 - "$SETTINGS" "$COMMAND" <<'PY'
import json, os, sys

path, command = sys.argv[1], sys.argv[2]
try:
    with open(path) as fh:
        settings = json.load(fh)
except FileNotFoundError:
    settings = {}
except ValueError:
    sys.exit(f"{path} is not valid JSON. Fix or move it, then re-run.")

settings["statusLine"] = {
    "type": "command",
    "command": command,
    "refreshInterval": 60,
}

os.makedirs(os.path.dirname(path), exist_ok=True)
tmp = path + ".tmp"
with open(tmp, "w") as fh:
    json.dump(settings, fh, indent=2)
    fh.write("\n")
os.replace(tmp, path)
print(f"  statusLine -> {command}")
PY

echo
echo "Done. It appears on your next interaction, no restart needed."
echo "Preview it now with:"
echo "  echo '{\"model\":{\"display_name\":\"Opus 5\"},\"workspace\":{\"current_dir\":\"'\"\$PWD\"'\"},\"context_window\":{\"used_percentage\":42}}' | $COMMAND"
