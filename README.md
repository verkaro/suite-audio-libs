# suite-audio-libs

The libraries the Dexter family shares (Dexter, dexter-scape, dexter-place, dexter-foley, BRR):
audio engineering, and the plumbing every sibling had written for itself. MIT. Its plan is
BRR's `suite-plan.md`.

A tool depends on a crate by this repository's git address, pinned to a commit:

```toml
[workspace.dependencies]
as-wav = { git = "https://github.com/verkaro/suite-audio-libs", rev = "COMMIT" }
```

and, while working, patches that address to the clone beside it in a `.cargo/config.toml`
kept out of git, so a change here is live there at once:

```toml
[patch."https://github.com/verkaro/suite-audio-libs"]
as-wav = { path = "../suite-audio-libs/crates/wav" }
```

A fix goes in here, and each tool moves its pin when it's ready. Nothing is here that's only one
tool's.

## The crates

- `as-wav`: WAV written crash-safe (24-bit, 16-bit dithered, float) and read.
- `as-flac`: FLAC written, 24-bit, with no dependencies; and read, through `claxon`.
- `as-resample`: a whole sound converted to another rate, as sox's `rate -v`.
- `as-seam`: a LOOP's seam: its join, the crossfade's law matched to how alike its sides are,
  and FIND.
- `as-draw`: variation from a seed, a pure function of the seed, the voice and the pass.
- `as-tilt`: TILT, one slope in dB an octave through 1 kHz.
- `as-conv`: zero-latency convolution, the same at any block size.
- `as-ir`: an IR made ready: true stereo folded, unit energy, its echoes without its direct
  sound.
- `as-opening`: an opening between the outside and a room: its stops, its wall, its routes.
- `as-jack-keeper`: a JACK client of two outputs, its player on JACK's thread, its
  connections kept, reattaching to jackd; and `--sim`.
- `as-keep`: nothing overwritten: exclusive numbered files, folders made whole before they
  appear, write-and-rename.

Where each came from is `VENDORED.md`.

## Commands

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test -p CRATE NAME     # one test
BLESS=1 cargo test -p as-wav reference_files   # rewrites wav's testdata/, on purpose only
SUITE_JACK_TEST=1 cargo test -p as-jack-keeper --test dummy   # on a dummy JACK of its own
```

Building needs JACK2's dev package, `libjack-jackd2-dev`.
