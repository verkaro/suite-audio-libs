# suite-audio-libs

The libraries the Dexter family shares (Dexter, dexter-scape, dexter-place, dexter-foley, BRR):
audio engineering, and the plumbing every sibling had written for itself. MIT. Its plan, and
where each tool stands, is `PLAN.md`.

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

- `as-wav`: WAV written crash-safe (24-bit, 16-bit dithered, float) and read, exactly into
  `f64` if asked (64-bit float too), its other chunks kept and written back, BWF's `bext`.
- `as-flac`: FLAC written, 24-bit, with no dependencies; and read, or its header alone, through `claxon`.
  `tags`: its Vorbis comments read, and rewritten without risking the audio (a new file, checked
  against the old, journalled, renamed over).
- `as-tar`: a tar written and read as it streams (ustar, with PAX for long paths and big
  files), for packing a tool's work.
- `as-resample`: a whole sound converted to another rate, as sox's `rate -v`, its taps the
  same on every machine (through `libm`); and a stream read at any speed (`read`), as a
  sampler tunes, about 100 dB clean.
- `as-seam`: a LOOP's seam: its join, the crossfade's law matched to how alike its sides are,
  and FIND.
- `as-draw`: variation from a seed, a pure function of the seed, the voice and the pass.
- `as-tilt`: TILT, one slope in dB an octave through 1 kHz.
- `as-conv`: zero-latency convolution, the same at any block size.
- `as-ir`: an IR made ready: true stereo folded, unit energy, its echoes without its direct
  sound. `numbers`: what it measures as, after ISO 3382-1 (RT, EDT, clarity, by octave).
- `as-opening`: an opening between the outside and a room: its stops, its wall, its routes.
- `as-jack-keeper`: a JACK client of two outputs, its player on JACK's thread, its
  connections kept, reattaching to jackd; and `--sim`.
- `as-jack-capture`: a JACK client of two inputs read into a ring off JACK's thread, its
  connections kept, reattaching to jackd; it says when frames were lost or a new ring began.
- `as-transport`: where JACK's transport is, read and never moved: a client of no ports and no
  sync callback, polled from a thread of its own, `None` while the server is away and
  reattaching when it returns; and a simulated clock of the same shape.
- `as-http`, `as-ws`, `as-meter`, `as-page-kit`: a tool's page: just enough HTTP, the
  WebSocket, the LED ladders' ballistics, and the skins and drawing as one script the page
  loads. `tools/cdp.py` drives a page in Falkon offscreen, for its tests.
- `as-serve`: a tool's server over the two: the listener, each connection's checks, the WebSocket
  at `/ws`, the clients and what's sent to them, a tick. A tool brings its hello, its commands and
  the words of a refusal.
- `as-ws::client`: the WebSocket's other end, for a panel (or anything else) talking to a
  tool's server: the handshake checked, fragments joined, pings answered, frames out masked.
- `as-loudness`: integrated loudness (BS.1770-4, gated) at any rate, its per-channel mean
  beside the standard's sum, a linked group's, true peak, and the trim to a reference.
- `as-biquad`: minimum-phase first- and second-order filters in `f64`, the same on every
  machine: a shelf and its exact inverse, a peak, a high-pass, a matched low-pass; any analog
  second-order section by the bilinear transform, with second-order high- and low-passes,
  cookbook shelves and shelves with their poles at the corner, Butterworth sections, and Q
  from a bandwidth in octaves.
- `as-bands`: a sound's power in ERB-spaced bands (a Hann window of 2048 or 4096, a frame every
  10 ms), computed a piece at a time: pieces of any size give the same bits as the whole.
- `as-speech`: where a voice is speaking: voiced frames and phrases from a stem's band powers
  (`as-bands`), against the stem's own noise floor.
- `as-snapshot`: detented settings: versioned detent tables with their plain words, strict TOML
  snapshots, and an ID that's the same for any two that sound the same.
- `as-provenance`: what a processed file says about itself: a CodingHistory tag, a filename
  suffix, a sidecar log, and refusing a tool's own output.
- `as-defects`: clicks and single-sample errors found by where and how high, so they're fixed
  upstream.
- `as-envelope`: acceptance tests for a process: test signals, a synthetic corpus of
  speech-like and other sounds (including `speech`, a voice with speech's long-term spectrum,
  and `phrases`, that gated into phrases), and the measurements (tone, THD, products, nulls).
- `as-focusrite`: a Focusrite interface on the kernel's `scarlett2` driver, through its
  controls: the card's ioctls, the controls as text and a fake card from that text (the 18i20
  3rd gen's and the Clarett 2Pre's), the interface as its controls describe it (inputs, mixer,
  outputs, meters), its own switches (48V, AIR, INST, the clock, DIM and MUTE, the lines,
  standalone), a plan of mixes set on it, the mixer kept on a thread of its own, routing and
  the mixer set by hand with its inputs shared out by need, and every control put back as it
  was found.
- `as-interfaces`: the audio interfaces by what they are (a USB box by its `/dev/snd/by-id`
  name, with its serial), their rates and channels at each, found again at their `hw:N`;
  whether JACK may run realtime. Only reads, under a root tests fake. No JACK.
- `as-jackd`: the JACK server itself: jackd started from settings (the interface found, setup
  commands run, its own session, a dated marker in the family's jackd log), found by
  `JACK_DEFAULT_SERVER`, watched (xruns, the graph, its going away), stopped by its pid unless a
  named client holds it, and `Dummy`, the tests' server of their own.
- `as-bay`: the patch bay's core, Warren's bay: the graph as a snapshot, bundles and their
  names, the patch syntax parsed, compiled to a plan and printed back, the matrix and its views,
  staged lines, the session and its undo, and a fake backend with its fixture format. No I/O.
  `as_bay::sheet` is a tool's patch bay over it: one bay shared by several screens, each with
  its view, its grouping and its staged lines, TO, CUT and ONLY, APPLY and UNDO, and the tool's
  own rules through a trait. It runs on the fake backend for a tool's `--sim`.
- `as-bay-jack`: the bay's JACK backend: the graph read from any `jack::Client`, a plan run
  through one, a counter of graph notices, and a backend on a client of its own (`bay`, or the
  tool's name).
- `as-localtime`: local wall-clock time through the C library, in the formats Dexter's files
  use.
- `as-rawmidi`: ALSA raw MIDI out: ports found by card under `/dev/snd`, a port opened for
  writing, note on and off, program change, controllers and panic as bytes.
- `as-pitch`: a tone's pitch by YIN and a staged long-lag refinement, good to a few cents from
  A0 to C8; and a frame-by-frame track for vibrato.
- `as-sfz`: an SFZ instrument written as text, its headers, opcodes and values checked.
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
SUITE_JACK_TEST=1 cargo test -p as-transport --test dummy     # likewise
SUITE_JACK_TEST=1 cargo test -p as-jack-capture --test dummy    # likewise
```

Building needs JACK2's dev package, `libjack-jackd2-dev`.
