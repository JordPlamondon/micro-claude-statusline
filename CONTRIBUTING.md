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

Bump `version` in `rust/Cargo.toml` to match the tag first. The script refuses
otherwise, because `--version` reads that field and would otherwise report a
different number from the release it came out of. It sat at 1.0.0 through the
first ten releases, which is what happens when nothing checks.

Both this and `install.sh rust` source `scripts/rustflags.sh`, which is what
makes the output reproducible: build the same source anywhere and you get the
same bytes, so a release can be checked against the source it claims to come
from. Keep them sourcing it. A release built with a bare `cargo build` will
still work and still be correct, it just quietly stops being checkable, which
is the sort of thing nobody notices for months.

Deliberately not a universal binary. On Apple Silicon, exec-ing a fat binary
containing an x86_64 slice costs around 20 ms every time, roughly ten times this
program's entire runtime. Measured: thin arm64 2.4 ms, fat arm64-only 2.4 ms,
fat arm64 + x86_64 22.1 ms. The fat header is free; the Intel slice is not.

## After changing output

```sh
cd rust && cargo build --release && cd ..
python3 scripts/golden.py
```

432 recorded cases in `tests/golden.txt`, compared byte for byte. Every whole
percent through the meters, token and duration boundaries, all PR states,
worktrees, every theme, config permutations, wide characters, ten terminal
widths swept twice, and missing or malformed input, under both 24-bit and
256-colour.

It also checks that `Cargo.lock` still contains exactly one package. Zero
dependencies is the claim that separates this from every comparable tool, and
it is the kind that stops being true quietly: someone reaches for a crate to
save twenty lines and nobody notices until a reader checks.

Then it fuzzes: 1,500 mutations of a valid payload plus a set of shapes aimed
at the parser's own arithmetic, checking only that the program is still
standing afterwards. There is no correct bar for a corrupted payload, but a
panic is a status line that vanishes mid-session and leaves an abort message
where the bar should be. The seed is fixed, so a failure reports an input you
can reproduce rather than one that happened once. It adds about eight seconds.

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

## After changing the installer

```sh
sh scripts/test-install.sh
```

22 checks against a scratch HOME: install, re-install, uninstall, and the cases
where uninstall has to refuse. It builds from source, so it needs cargo but no
network and no release. It never touches your real `~/.claude`.

Worth having because the risk here is inverted. A renderer bug is an ugly bar,
and the renderer has 432 golden cases. An installer bug edits `settings.json`
and stops halfway, and until now the installer had nothing.

The cases that matter most are the refusals: a `statusLine` pointing at some
other tool must survive, a real file where our symlink would go must not be
deleted, and malformed JSON must abort before anything is removed rather than
leaving a binary behind that settings.json no longer points at.

## Checking the speed claim

```sh
python3 scripts/bench.py
```

Runs the docs bash example against this binary on the same payload and prints
the table the README quotes. Needs `jq`, takes about half a minute, and
measures CPU rather than wall clock: a chain of short-lived processes spends
much of its wall time descheduled rather than computing, which understates the
gap. Interleaved and best-of-five, because a single-trial loop on a warm laptop
varies by a factor of three and will report the wrong winner.

The README says "about 40x". It measures higher than that here, so the figure
in the README is deliberately the conservative end.
