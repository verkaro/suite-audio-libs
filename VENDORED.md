# Moved in from the siblings

What's below lives here now and is changed here. The siblings it came from keep their own
copies until they move over to the suite, each in its own session, through a handoff.

From `dexter-foley` at `9e1ea2226362e43f786c1d781d510e536fccb0a2` (2026-10-29), with `git
archive`, for BRR's step 1:

- `crates/wav` (`foley-wav`, now `as-wav`), with its tests and `testdata/`: renamed, its first
  comment and the tests' scratch name changed; as it was otherwise. foley had it from place,
  place from scape, scape from Dexter.
- `crates/flac` (`foley-flac`, now `as-flac`), with its tests: renamed, its first comment
  changed. foley had it from place.
- `crates/rate` (`foley-rate`, now `as-resample`, in `crates/resample`), with its tests and
  README: renamed, its first comment and the README's "Taking it" changed.
- `rustfmt.toml` and `.gitignore`, as they were.

From `dexter-scape` at `07ae205311e62ff153564bcca97816dc7649670a` (2026-10-29), with `git
show`, for BRR's step 2:

- `crates/core/src/seam.rs`, now `as-seam` (`crates/seam`): a sound is its channels, `&[Vec<f32>]`,
  where scape had its `Sample`; as it was otherwise. Its tests, from scape's
  `crates/core/src/tests.rs`, on synthetic noise and tones.
- `crates/core/src/tilt.rs`, now `as-tilt` (`crates/tilt`): its glide, `GLIDE`, a constant of its
  own (512 frames, scape's DECLICK). Its tests, on the filter alone, and one more: the same at any
  block size.
- `draw` and `mix`, from `crates/core/src/lib.rs`, now `as-draw` (`crates/draw`): what's drawn is
  a number, where scape had its `What`; the layer's seed is `own_seed`; `spread` as scape's;
  `between` and `pick` (never the same twice running) written here.

From `dexter-place` at `784f1ba6ab09b492e68bde1aca56f5b724189664` (2026-10-28), with `git show`,
for BRR's step 4:

- `crates/core/src/conv.rs`, now `as-conv` (`crates/conv`): `touched` made public, for the
  callers' own buffers; as it was otherwise. Its tests, from place's `crates/core/src/tests.rs`.
- `crates/core/src/opening.rs`, now `as-opening` (`crates/opening`): as it was. Its tests, from
  place's, and one more: between two stops, each route theirs crossfaded.

- `crates/jack`, now `as-jack-keeper` (`crates/jack-keeper`), for BRR's step 5: `src/lib.rs`
  (the keeper), `src/client.rs`, `src/sim.rs`, and the tests (`src/tests.rs`,
  `tests/dummy.rs`, `SUITE_JACK_TEST=1`). Made generic: the client's name is the caller's, and
  what plays is a `Player` the keeper makes each time it opens the client, sent messages
  through one ring and handing back what it's done with through another; place's player, its
  controls and its rule for clearing what's waiting stay place's. The tests play a steady
  value of their own.

For BRR's step 6, with `git show`:

- From `dexter-foley` at `9e1ea22`: `crates/server/src/http.rs`, now `as-http`
  (`crates/http`), with place's `listen` added; and `docs/dev/cdp.py`, the DevTools client
  for testing a page in Falkon offscreen, now `tools/cdp.py`.
- From `dexter-place` at `784f1ba`: `crates/ws`, now `as-ws`, and `crates/meter`, now
  `as-meter`, as they were but for their first comments; and its page's skins and drawing
  (`crates/server/page/index.html`: the skins, `onKey`, the LEDs, keys, ladders, readouts,
  lamps and text), now `as-page-kit`'s `page-kit.js`: the page's state taken out of them (taps
  land in `HITS`, the skin in `SKIN`), and `kitCanvas`, `chooseSkin` and `savedSkin` for a
  page to use them.
- From Dexter at `7ae6dbd` (2026-11-07, Dexter's step 2): `crates/ws/tests/echo.rs`, now
  `as-ws`'s `tests/echo.rs`, as it was but for `dexter_ws` renamed `as_ws`.
- From dexter-kms at `b7ca423` (2026-11-07, for Tape's panel): `src/ws.rs`, the client, now
  `as-ws`'s `client` module. Changed on the way: `next` is `read`; the handshake's answer is
  checked and must come within 5 s; the key and the masks differ each time, where they were
  fixed; reading waits for ever unless the reader is given a timeout (dexter-kms's 5 s is now
  its own); a close is answered, and an opcode out of place is an error, where it was taken as
  a message; `send_binary` and `close` added. Its tests written here.

Written here, not moved:

- `as-flac`'s `read` (2026-10-29, at BRR's code review): FLAC read through `claxon`, whole,
  as floats; a stream cut short keeps its whole frames.
- `crates/ir` (`as-ir`, 2026-10-29): true stereo folded, unit energy, the onset, and the echoes
  without the direct sound, after place's `crates/library/src/ir.rs`, `numbers.rs`'s `onset`
  and its design's "The outside's space".

- `crates/keep` (`as-keep`, 2026-10-29), after the way Dexter's export (`renameat2`'s
  `RENAME_NOREPLACE`), place's renders and scape's modules each kept from overwriting.
- `crates/tar` (`as-tar`, 2026-10-31, for BRR's packs, step 14): a tar written and read as it
  streams, ustar with PAX for long paths and big files. Written fresh.

For Tape (its RFC, section 9), 2026-11-01, each written fresh from its RFC and the standards
named in its header:

- `crates/loudness` (`as-loudness`): BS.1770-4 gated loudness at any rate, the per-channel mean
  beside the standard's sum (Tape's A1), the short-file rule (A8), true peak through
  `as-resample`, the trim to a reference; measured a piece at a time (`Meter`, `PeakMeter`).
- `crates/biquad` (`as-biquad`): first- and second-order sections in `f64` through `libm`: a
  first-order shelf and its exact inverse, a high-pass, the cookbook's peak, a matched low-pass
  after Vicanek (2016). Added 2026-11-02, for ducky: `bilinear` (any analog second-order
  section, prewarped at a frequency), second-order high- and low-passes, the cookbook's shelves
  (Bristow-Johnson) and shelves whose poles sit at the corner, `butterworth` sections and
  `q_from_octaves`.
- `crates/bands` (`as-bands`, 2026-11-02, for ducky): band powers on the ERB-number scale
  (Glasberg and Moore, 1990), a frame every 10 ms, streamed a piece at a time. Written fresh.
- `crates/speech` (`as-speech`, 2026-11-02, for ducky): voiced frames and phrases from band
  powers against a percentile noise floor, held across short gaps. Written fresh.
- `crates/transport` (`as-transport`, 2026-11-03, for ducky's task 24): JACK's transport read
  and never moved (a client of no ports, active so a dead server is noticed, polled every 10 ms
  into atomics; forgotten rather than closed when the server goes; reattached each second), and
  a simulated clock. Written fresh, opening and losing the client after `as-jack-keeper`.
- `crates/snapshot` (`as-snapshot`): versioned detent tables, strict TOML snapshots, and an ID
  that's the same for any two that sound the same.
- `crates/envelope` (`as-envelope`): test signals, a synthetic corpus of speech-like and other
  sounds from code, and the measurements (tone, THD, products, nulls). Added 2026-11-02, for
  ducky: `corpus::speech` (`voice` through a +30 dB shelf at 250 Hz) and `corpus::phrases`.
- `crates/provenance` (`as-provenance`): a CodingHistory line, a filename suffix, a sidecar
  log, and refusing a tool's own output.
- `crates/defects` (`as-defects`): clicks and single-sample errors found by where and how high.
- `crates/rawmidi` (`as-rawmidi`, 2026-11-04, for dexter-like110): raw MIDI ports found under
  `/dev/snd` by card id, opened write-only, the messages a keyboard needs, and panic. Written
  fresh, after the way dexter-surface's `mcu` finds its ports.
- `crates/pitch` (`as-pitch`, 2026-11-04, for dexter-like110): YIN (de Cheveigne and Kawahara,
  2002) with a staged long-lag refinement, and a per-frame track. Written fresh.
- `crates/sfz` (`as-sfz`, 2026-11-04, for dexter-like110): an SFZ written as text and checked.
  Written fresh.
- `as-seam`'s `sameness` made public (2026-11-04, for dexter-like110's splice).
- `crates/jack-capture` (`as-jack-capture`, 2026-11-04, for dexter-like110): a JACK client of two
  inputs, a ring read off JACK's thread, its connections kept, reattaching to jackd with a new
  ring (said as a gap). Written fresh, opening and losing the client after `as-jack-keeper`.
- `crates/jack-client` (`as-jack-client`, 2026-11-12, when `as-bench` made a third client; Warren
  chose a crate of its own): opening the client, locking memory, the notices, `reconcile`. Written
  here, not moved: gathered from the copies `as-jack-keeper` and `as-jack-capture` each had.
- `crates/bench` (`as-bench`, 2026-11-12, step 0 of `specs/shared.md`, for sonorack's measure):
  signals, alignment, the check, the reduction, figures, pictures as data, the sitting, and a
  JACK client of two outs and two ins. Written here, not moved. dexter-place's `place-measure`
  is the same idea; nothing was read from it or copied.
- `crates/control` (`as-control`, 2026-11-12, step 1 of `specs/shared.md`, for `as-eq` and
  `as-dynamics`, and sonofader2's desks later): the controls as data. Written here, not moved.
- `crates/eq` (`as-eq`, 2026-11-12, step 3 of `specs/shared.md`, for sonorack, then BRR, ducky,
  place and irma): the channel EQ. Written here, not moved.
- `crates/dynamics` (`as-dynamics`, 2026-11-12, step 4 of `specs/shared.md`, for sonorack, then
  BRR, ducky, place and irma): the core of the de-esser, optical and FET. Written here, not moved.
  `as-control`'s `Ramp` came out of `as-eq`, where it was private, the same day.

From `dexter-foley` at `9e1ea2226362e43f786c1d781d510e536fccb0a2` (2026-11-09), with `git
archive`, for dexter-irma's step 1:

- `crates/tags` (`foley-tags`), now `as_flac::tags` (`crates/flac/src/tags`), with its tests.
  Changed: the journal's JSON is written and read by its own code, where foley used
  `serde_json`, and a line is byte for byte what `serde_json` wrote; setting `SERIAL` is no
  longer refused here, since that rule is the tool's; the file written beside the old is
  `.NAME.tags-…`, where it was `.NAME.foley-…`.

From `dexter-place` at `784f1ba6ab09b492e68bde1aca56f5b724189664` (2026-11-09), with `git
show`, for dexter-irma's step 1:

- `crates/core/src/numbers.rs`, now `as_ir::numbers` (`crates/ir/src/numbers`): a sound is its
  channels, `&[Vec<f32>]`, where place had its `Sound`, and its gain is `as_ir::unit_energy`; as
  it was otherwise. Its tests, from place's `crates/library/src/tests.rs`.

From Dexter at `e98d44f77be270380e4d086240506777ac3c62f6` (2026-11-10), with `git archive`,
for focusrite-ctrl (Warren, 2026-11-10: contributed now, Dexter moving onto it later):

- `crates/focusrite` (`dexter-focusrite`), now `as-focusrite`, with its tests and `testdata/`
  (the 18i20 3rd gen's controls, read from Warren's, a list of controls and not a recording).
  Changed: renamed; its first comment; the tests' variables `SUITE_FOCUSRITE_TEST`,
  `SUITE_FOCUSRITE_JACK` and `SUITE_FOCUSRITE_PUT_BACK`, and `BLESS`, where Dexter's are
  `DEXTER_…`; the tests' scratch names. As it was otherwise: its comments still name Dexter's
  docs, its deck and its mixes. Dexter keeps its own copy until its step 7.

From Dexter at `e98d44f77be270380e4d086240506777ac3c62f6` (2026-11-11), read with `git show`,
for jackbay (Warren, 2026-11-11: the suite's half of Dexter's step 8 first, `RFC-as-jackd.md`):

- `crates/localtime` (`dexter-localtime`), now `as-localtime`, with its tests: renamed, its
  first comment. As it was otherwise.
- `crates/system/src/interfaces.rs`, now `as-interfaces` (`crates/interfaces`), with `cards` and
  `limit` from `crates/system/src/lib.rs` and its tests from `src/tests.rs` (the two Claretts
  faked under a scratch root): renamed, its first comment, `rates` tested directly. As it was
  otherwise.
- `crates/jack/src/jackd.rs`, now `as-jackd` (`crates/jackd`), and `JackConfig` and
  `jackd_args` from `crates/project/src/config.rs`, now `Settings` and `Settings::args`. Changed:
  the settings, interface and setup commands come in a `Start`, not a project's `Config`; the
  server's name is an argument of `args`; the probe's client name, and the clients that refuse
  a stop, are the caller's; the log is `$XDG_STATE_HOME/jack/jackd.log` (Warren, 2026-11-11);
  `Settings::on_dummy` takes the counts, which Dexter makes from its tracks; `watch`'s text stays
  Dexter's. Written here: `Monitor`'s notification handler (Dexter's is its engine's), `Follow`,
  `holder`, `clients`, `Dummy` and `eventually`. Its tests are Dexter's, and
  `tests/dummy.rs`, new. Dexter keeps its own copy until its step 8.

From Dexter at `e98d44f77be270380e4d086240506777ac3c62f6` (2026-11-11), with `git archive`,
for jackbay (Warren, 2026-11-11: the suite's half of Dexter's step 9 first, `RFC-as-bay.md`, b1):

- `crates/bay-core`, now `as-bay` (`crates/bay`), with its tests and `testdata/studio.fake`:
  renamed, its first comment. As it was otherwise.
- `crates/bay-jack`, now `as-bay-jack` (`crates/bay-jack`), with its live test. Changed:
  renamed; its first comment; `Jack::open_as(name)` added, `Jack::open()` still naming it `bay`;
  the live test on `as_jackd::Dummy` (`asbaytest-<pid>`) with `SUITE_JACK_TEST=1`, where it made
  a dummy of its own with `BAY_JACK_TEST=1`. As it was otherwise.

Both are Warren's bay, bay-rs's `crates/core` and `crates/jack` at
`af5c0c7bc5a332b06898c09cae700c0e45bd1e93`, which Dexter took with `git archive` on 2026-09-29
(its `crates/bay-core/VENDORED.md`: `studio.fake` moved beside the crate, then rustfmt). bay-rs
has no licence file; here, as in Dexter, they're MIT (Warren, 2026-10-03). bay-rs's `PLAN.md`
and `SYNTAX.md`, which the comments name, aren't here yet (`RFC-as-bay.md`, Settled 4). From
now on the bay is changed here. Dexter keeps its own copies until its step 9.

Written here, not moved:

- `crates/bay/src/sheet.rs` (`as_bay::sheet`, 2026-11-11, `RFC-as-bay.md` b2, for jackbay):
  written from Dexter's `crates/patch` at `e98d44f`, its sheet without Dexter's rules, which
  stay Dexter's (its tracks and mixes, the guard, the loops, `studio`). A screen's own grouping
  (`View::grouping`, `Session::bundles_with`) is jackbay's, its `docs/dev/SPEC.md` 8.3. Its tests
  are the sheet's of Dexter's, on a graph written out here, and new ones. Dexter keeps its own
  until its step 9.
- `crates/serve` (`as-serve`, 2026-11-09, for dexter-irma): a tool's server over `as-http` and
  `as-ws`. Written fresh, after the shape BRR's, Tape's and ducky's servers share, each of which
  holds its own until it moves. Two things differ from theirs: a client that can't take what
  it's sent is closed, where theirs dropped the message and left it with part of the state;
  and the order of the tool's lock and the clients' is fixed by how a client is admitted.
