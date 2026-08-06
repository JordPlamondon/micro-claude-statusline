# Working on this

Everything here is for changing the program. Using it needs none of it, see the
[README](README.md).

## Cutting a release

```sh
./scripts/build-release.sh v1.0.1
```

Builds both Apple targets as separate binaries, writes checksums, smoke tests
the native one and uploads all three. Needs `rustup target add
x86_64-apple-darwin` once.

Deliberately not a universal binary. On Apple Silicon, exec-ing a fat binary
containing an x86_64 slice costs around 20 ms every time, roughly ten times this
program's entire runtime. Measured: thin arm64 2.4 ms, fat arm64-only 2.4 ms,
fat arm64 + x86_64 22.1 ms. The fat header is free; the Intel slice is not.

## After changing output

```sh
cd rust && cargo build --release && cd ..
python3 scripts/golden.py
```

372 recorded cases in `tests/golden.txt`, compared byte for byte. Every whole
percent through the meters, token and duration boundaries, all PR states,
worktrees, every theme, config permutations, ten terminal widths, and missing
or malformed input, under both 24-bit and 256-colour.

Time is pinned with `$STATUSLINE_TEST_PINNED_CLOCK`, `$COLUMNS` is pinned, and
`$HOME` is redirected to a scratch directory, so the same input produces the
same bytes on any machine at any time of day.

If a change is intended, re-record it and read the diff before committing:

```sh
python3 scripts/golden.py --update
git diff tests/golden.txt
```

This is worth running. The case list caught a currency bug that only appeared at
`$0.005`, where scaling by 100 before rounding lands on exactly `0.5` in
floating point and loses the tie-break.

The expectations were originally recorded from a separate Python implementation
that has since been removed, so they pin reviewed behaviour rather than
whatever the current code happens to do.
