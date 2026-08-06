# Build flags that make the binary reproducible.
#
# Sourced, not run. The caller sets CRATE_DIR first:
#
#   CRATE_DIR="$ROOT/rust"
#   . "$ROOT/scripts/rustflags.sh"
#
# Without these, two people building identical source get different binaries,
# so nobody can check that a release matches the source it claims to come from.
# The SHA-256 in the release only tells you the download was not corrupted; it
# is generated alongside the binary by whoever built it. Reproducibility is what
# lets you skip trusting that person, which for a program whose whole pitch is
# "read it before you run it" is the point.
#
# Three things differed, none of them about what was built:
#
#   - the crate path, recorded in panic locations
#   - the toolchain path, which also embedded the builder's username five times
#     in every binary they shipped
#   - LC_UUID, which ld derives from the paths of the object files, so it
#     changed even when every byte of content was the same
#
# The first two are also why `strings` on a release used to name whoever built
# it. Dropping LC_UUID costs nothing here: the binary is stripped and aborts
# rather than unwinding, so there is no symbolication to preserve.

SYSROOT=$(rustc --print sysroot)
RUSTFLAGS="--remap-path-prefix=${SYSROOT}=/rustc"
RUSTFLAGS="${RUSTFLAGS} --remap-path-prefix=${CRATE_DIR}=/build"
RUSTFLAGS="${RUSTFLAGS} -C link-arg=-Wl,-no_uuid"
export RUSTFLAGS
