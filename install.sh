#!/bin/sh
# Installs the Claude Code status line.
#
#   ./install.sh            download the prebuilt binary, else build from source
#   ./install.sh binary     download the released universal binary (needs gh)
#   ./install.sh rust       build from source (needs cargo)
#
# Nothing here installs a toolchain for you. If cargo is missing the installer
# says so rather than fetching Rust behind your back.
#
# The binary is copied to ~/.claude/bin, so this checkout can be deleted
# afterwards. Writes the statusLine block into ~/.claude/settings.json and
# leaves every other setting untouched.

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
      echo "Need either gh with access to $REPO, or cargo to build from source." >&2
      echo "Install gh:    brew install gh && gh auth login" >&2
      echo "Install Rust:  https://rustup.rs" >&2
      exit 1
    fi
    ;;
  binary|rust) ;;
  *) echo "usage: $0 [binary|rust]" >&2; exit 1 ;;
esac

if [ "$choice" = binary ]; then
  command -v gh >/dev/null 2>&1 || {
    echo "gh not found. Install it, or run: $0 rust" >&2
    exit 1
  }
  # Releases carry macOS builds only. Without this check the dispatch below
  # matches on architecture alone, so an aarch64 Linux box happily downloads
  # the Apple arm64 binary, verifies it, marks it executable and writes
  # settings.json, then fails to exec with nothing explaining why.
  if [ "$(uname -s)" != "Darwin" ]; then
    echo "Prebuilt binaries are macOS only. On $(uname -s), build from source:" >&2
    echo "  $0 rust" >&2
    exit 1
  fi

  # One binary per architecture rather than a universal one: on Apple Silicon a
  # fat binary containing an x86_64 slice costs about 20 ms per exec, ten times
  # this program's entire runtime.
  case "$(uname -m)" in
    arm64|aarch64) ASSET="statusline-arm64" ;;
    x86_64)        ASSET="statusline-x86_64" ;;
    *) echo "Unsupported architecture: $(uname -m). Build from source: $0 rust" >&2; exit 1 ;;
  esac

  DEST="$ROOT/bin"
  mkdir -p "$DEST"
  rm -f "$DEST/statusline" "$DEST/$ASSET" "$DEST/statusline.sha256"
  echo "Downloading the prebuilt binary for $(uname -m)."
  gh release download --repo "$REPO" --pattern "$ASSET" --dir "$DEST" --clobber

  # Verification is mandatory. Previously a missing checksum was swallowed and
  # the check silently skipped, which is the failure mode you least want: an
  # unverified binary installed with no warning.
  if ! gh release download --repo "$REPO" --pattern statusline.sha256 --dir "$DEST" --clobber; then
    echo "No statusline.sha256 in the release, so the download cannot be verified." >&2
    echo "Build from source instead: $0 rust" >&2
    exit 1
  fi
  # The checksum file lists both architectures; check only the one we fetched.
  ( cd "$DEST" && grep " $ASSET\$" statusline.sha256 | shasum -a 256 -c - ) || {
    echo "Checksum did not match. Refusing to install." >&2
    rm -f "$DEST/$ASSET"
    exit 1
  }

  # Only executable once it has been verified.
  chmod +x "$DEST/$ASSET"
  BUILT="$DEST/$ASSET"
else
  command -v cargo >/dev/null 2>&1 || {
    echo "cargo not found. Install Rust from https://rustup.rs, or run: $0 binary" >&2
    exit 1
  }
  echo "Building from source. About twenty seconds the first time."
  ( cd "$ROOT/rust" && cargo build --release --quiet )
  BUILT="$ROOT/rust/target/release/statusline"
fi

# Copy out of the checkout before pointing anything at it. Otherwise deleting
# the clone later leaves settings.json aimed at a path that no longer exists,
# and the status line just stops with no explanation.
INSTALLED="${HOME}/.claude/bin/statusline"
mkdir -p "${HOME}/.claude/bin"
cp "$BUILT" "$INSTALLED"
chmod +x "$INSTALLED"
COMMAND="$INSTALLED"

# A symlink on PATH, so `statusline --preview` and `statusline --init` work
# without typing the full path. Skipped when ~/.local/bin is not on PATH, since
# putting it there would then be pointless.
echo "  binary     -> $COMMAND"

case ":${PATH}:" in
  *":${HOME}/.local/bin:"*)
    mkdir -p "${HOME}/.local/bin"
    ln -sf "$COMMAND" "${HOME}/.local/bin/statusline"
    echo "  statusline -> ${HOME}/.local/bin/statusline"
    ;;
  *)
    echo "  ~/.local/bin is not on your PATH, so the statusline command is not linked."
    echo "  Run it as $COMMAND, or add that directory to PATH."
    ;;
esac

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
echo
echo "Have a look at the themes:"
if [ -L "${HOME}/.local/bin/statusline" ]; then
  echo "  statusline --preview        all six"
  echo "  statusline --init           write a config you can edit"
else
  echo "  $COMMAND --preview"
  echo "  $COMMAND --init"
fi
