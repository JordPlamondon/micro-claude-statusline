#!/bin/sh
# Builds a universal macOS binary and attaches it to a GitHub release, so that
# people can install without a Rust toolchain.
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

mkdir -p "$OUT"
lipo -create -output "$OUT/statusline" \
  "$ROOT/rust/target/$ARM/release/statusline" \
  "$ROOT/rust/target/$INTEL/release/statusline"

echo
echo "Universal binary:"
lipo -archs "$OUT/statusline" | sed 's/^/  architectures: /'
ls -l "$OUT/statusline" | awk '{printf "  size: %.0f KB\n", $5/1024}'

# A checksum so anyone can confirm what they downloaded is what you built.
( cd "$OUT" && shasum -a 256 statusline > statusline.sha256 )
cat "$OUT/statusline.sha256" | sed 's/^/  /'

echo
echo "Smoke test:"
printf '%s' '{"model":{"display_name":"Opus 5"},"workspace":{"current_dir":"/tmp"},"context_window":{"used_percentage":42}}' \
  | "$OUT/statusline" | sed 's/^/  /'

echo
if gh release view "$VERSION" >/dev/null 2>&1; then
  echo "Release $VERSION exists, replacing the asset"
  gh release upload "$VERSION" "$OUT/statusline" "$OUT/statusline.sha256" --clobber
else
  gh release create "$VERSION" "$OUT/statusline" "$OUT/statusline.sha256" \
    --title "$VERSION" \
    --notes "Universal macOS binary, Apple Silicon and Intel. Verify with statusline.sha256.

Install without a Rust toolchain:

    gh release download $VERSION --repo JordPlamondon/claude-statusline --pattern statusline
    chmod +x statusline

Or let the installer do it: ./install.sh"
fi

echo
echo "Done. Install path for others:"
echo "  ./install.sh          picks the release binary automatically"
