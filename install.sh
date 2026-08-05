#!/bin/sh
# Installs the Claude Code status line.
#
#   ./install.sh          build the Rust binary if cargo is present, else Python
#   ./install.sh python   force the Python version, no toolchain needed
#   ./install.sh rust     force the Rust version, requires cargo
#
# Writes the statusLine block into ~/.claude/settings.json and leaves every
# other setting untouched.

set -eu

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

case "$choice" in
  auto)
    if command -v cargo >/dev/null 2>&1; then choice=rust; else choice=python; fi
    ;;
  python|rust) ;;
  *) echo "usage: $0 [python|rust]" >&2; exit 1 ;;
esac

if [ "$choice" = rust ]; then
  command -v cargo >/dev/null 2>&1 || {
    echo "cargo not found. Install Rust from https://rustup.rs, or run: $0 python" >&2
    exit 1
  }
  echo "Building the Rust binary. This takes about 20 seconds the first time."
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
