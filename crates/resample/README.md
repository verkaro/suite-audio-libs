# as-resample

Sample-rate conversion for the Dexter tools: a sound at one rate made a sound at another, up or
down, as sox's `rate -v` makes it. Written for dexter-foley (exports at a chosen rate, and the
page's sounds played through JACK at its rate) as `foley-rate`, and moved into the suite
(Warren, 2026-10-28: "That converter is a resource to the family of Dexter tools"). It has
no dependencies.

## Using it

```rust
// One channel, once.
let at_48k: Vec<f64> = as_resample::convert(&samples, 96_000, 48_000);
let at_48k: Vec<f32> = as_resample::convert_f32(&samples_f32, 96_000, 48_000);

// Many channels: the taps worked out once.
let c = as_resample::Converter::new(44_100, 48_000);
let (l, r) = (c.run(&left), c.run(&right));
let frames = c.len(left.len()); // what a length at 44.1 kHz comes to at 48
```

A sound comes out as long as it went in, its first sample where it was. The same rate both
ways gives the same samples back. Samples are anything (1.0 full scale, as a rule); nothing is
clipped or dithered, so a sound at 0 dBFS can ring a little past it, and the caller decides
(dexter-foley lowers it by just enough, in tenths of a dB, and says so).

## What it does

Each output sample is read at its exact place between the input's. The rates' ratio in lowest
terms (44.1 to 48 kHz is 147 to 160) gives every phase, each with its own taps, worked out in
full before it runs: nothing is read from a table between its entries. A ratio with more than
4,096 phases (44,100 to 48,001 Hz, say) has each sample's taps worked out as it's needed:
slower, as exact.

- **Flat to 91% of the lower rate's Nyquist:** 20 kHz at 44.1 kHz, 21.8 kHz at 48.
- **At least 140 dB down from that Nyquist up:** a Kaiser window, β 14.5, as long as Kaiser's
  formula asks for that transition at that depth. Converting down aliases nothing; converting
  up adds no images.
- **Linear in phase:** nothing moves in time.
- **Worked in `f64`,** each phase's taps summing to 1, so a steady level holds exactly.

## How good

Its tests (`src/tests.rs`) convert pure tones between 44.1, 48, 96 and 192 kHz, and up from
44.1 to 48,001 Hz, and hold them against the ideal tone at the new rate: every one within 148
to 181 dB, under what 24 bits hold (144 dB). A 23 kHz tone converted from 48 kHz to 44.1 comes
out 146 dB down. Against sox's `rate -v` itself (a 96 kHz sweep to 19 kHz, converted to 44.1),
the two agree to the last bit but at the sweep's abrupt ends, where their transition bands
differ.

## How fast

A minute of stereo, in a release build on the house's machine: 0.25 s from 96 to 48 kHz, 0.35 s
from 48 to 44.1, 0.39 s from 44.1 to 48. A Foley sound of a few seconds is a few hundredths.
In a debug build it's slow: build it optimised (`[profile.dev.package.as-resample] opt-level =
3`).

## What it isn't

It converts whole sounds, off any realtime thread: it allocates, and it reads the whole of what
it's given. A rate that changes as it plays, or a stream converted a period at a time on JACK's
thread, is scape's TUNE (`dexter-scape`, `crates/core/src/tune.rs`), a lighter sinc made for
that.

## Taking it

Depend on `as-resample` from the suite, pinned to a commit, as with any of its crates. It needs
Rust 1.88 or later (`as_chunks`).
