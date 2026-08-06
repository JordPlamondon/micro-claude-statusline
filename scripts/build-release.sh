#!/bin/sh
# Builds one macOS binary per architecture and attaches them to a GitHub
# release, so people can install without a Rust toolchain.
#
#   ./scripts/build-release.sh v1.0.0
#
# Needs cargo with both Apple targets, and gh authenticated. Run it yourself;
# there is no CI, because a private repo's Actions minutes are not worth it for
# a binary that changes a few times a year.

set -eu

VERSION="${1:-}"
[ -n "$VERSION" ] || { echo "usage: $0 <version>   e.g. $0 v1.0.0" >&2; exit 1; }

ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT="$ROOT/dist"
ARM="aarch64-apple-darwin"
INTEL="x86_64-apple-darwin"

for target in "$ARM" "$INTEL"; do
  rustup target list --installed | grep -qx "$target" || {
    echo "Missing target $target. Add it with: rustup target add $target" >&2
    exit 1
  }
done

echo "Building $ARM"
( cd "$ROOT/rust" && cargo build --release --quiet --target "$ARM" )
echo "Building $INTEL"
( cd "$ROOT/rust" && cargo build --release --quiet --target "$INTEL" )

rm -rf "$OUT"
mkdir -p "$OUT"

# One binary per architecture, deliberately NOT a universal binary. On Apple
# Silicon, exec-ing a fat binary that contains an x86_64 slice costs about 20 ms
# every time, presumably deciding whether Rosetta is needed. That is ten times
# the entire runtime of this program. Measured on an M-series Mac:
#
#   thin arm64            2.4 ms CPU
#   fat, arm64 only       2.4 ms
#   fat, arm64 + x86_64  22.1 ms
#
# The fat header itself is free. It is the presence of the Intel slice that
# costs, so shipping them separately is the whole fix.
cp "$ROOT/rust/target/$ARM/release/statusline" "$OUT/statusline-arm64"
cp "$ROOT/rust/target/$INTEL/release/statusline" "$OUT/statusline-x86_64"

echo
for f in statusline-arm64 statusline-x86_64; do
  printf '  %-20s %-8s ' "$f" "$(file -b "$OUT/$f" | grep -oE 'arm64|x86_64' | head -1)"
  ls -l "$OUT/$f" | awk '{printf "%.0f KB\n", $5/1024}'
done

# Checksums so anyone can confirm what they downloaded is what you built.
( cd "$OUT" && shasum -a 256 statusline-arm64 statusline-x86_64 > statusline.sha256 )
echo
sed 's/^/  /' "$OUT/statusline.sha256"

case "$(uname -m)" in
  arm64) SMOKE="$OUT/statusline-arm64" ;;
  *)     SMOKE="$OUT/statusline-x86_64" ;;
esac
echo
echo "Smoke test, native slice:"
printf '%s' '{"model":{"display_name":"Opus 5"},"workspace":{"current_dir":"/tmp"},"context_window":{"used_percentage":42}}' \
  | "$SMOKE" | sed 's/^/  /'

NOTES="One binary per architecture. Verify with statusline.sha256.

Not a universal binary on purpose. On Apple Silicon, exec-ing a fat binary that
contains an x86_64 slice costs around 20 ms every time, roughly ten times the
runtime of this program. Shipping them separately avoids that.

Install without a Rust toolchain: ./install.sh"

echo
if gh release view "$VERSION" >/dev/null 2>&1; then
  echo "Release $VERSION exists, replacing the assets"
  gh release upload "$VERSION" \
    "$OUT/statusline-arm64" "$OUT/statusline-x86_64" "$OUT/statusline.sha256" --clobber
else
  gh release create "$VERSION" \
    "$OUT/statusline-arm64" "$OUT/statusline-x86_64" "$OUT/statusline.sha256" \
    --title "$VERSION" --notes "$NOTES"
fi

echo
echo "Done. Install path for others:"
echo "  ./install.sh          picks the binary for their architecture"
