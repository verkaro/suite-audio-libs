# For sonorack, from the suite: step 1 on `as-eq`

Written 2026-11-12 by the suite's session, after `as-eq` was built (step 3 of `specs/shared.md`,
commit `c66e82a`, pushed). For sonorack's step 1 in `../sonorack/CLAUDE.md`, "The order": the
workspace, and `sonorack --render in.wav` through the EQ, pinned bit for bit at any block size.

## Starting the session

In `sonorack`, with the suite readable: `claude --add-dir ../suite-audio-libs`. **Opus 5.5, no
advisor**, high effort: this step is setup on crates whose tests already pin the hard parts. (The
advisor is worth adding at step 2, the JACK path, asked twice: on the plan, and on the diff.)
Then: "Build step 1 of `CLAUDE.md`'s order, from `../suite-audio-libs/Sonorack.md`."

## What's ready in the suite

Pin `https://github.com/verkaro/suite-audio-libs` at `c66e82a` or later; `../family.sh patches`
writes the local patch.

- **`as-eq`** (`crates/eq/docs/spec.md`, "As built" at its end):
  - `Eq::new(rate)`, `set(&Settings)`, `process(&mut [f64])` for one channel,
    `process2(l, r)` for two.
  - `settings()`, `settled()`, `readouts()`.
  - `Settings` is a plain `Copy` struct, read and set by control id: `get("lmf.freq")`,
    `set_by_id("lmf.freq", 1200.0)`. Every value is held to its range, never refused.
  - `controls()` lists the ids with their ranges, steps and labels (`as-control`).
  - `response`, `band_response` and `marker` draw the curve without a unit; step 1 doesn't need
    them.
- **`as-control`**: `format` and `parse` for each control (`1.2k`, `+3 dB`), and `check`.
- **`as-wav`** to read the input, **`as-keep`** to write `in.sonorack.wav` without overwriting.

## What to know when using it

- **The settings file:** a plain text file keyed by the controls' ids, with the unit's name in
  front (`eq.lmf.freq = 1200`). That is the snapshot's form in `docs/design.md`, so step 3's
  snapshots read the same file and `--render --snapshot` doesn't redo it. Values are read through
  each control's `parse`, so `1.2k` works in a file as on the panel. An id the EQ doesn't know is
  an error that names it. A value out of range is held, as the panel holds it.
- **A fresh unit takes its first settings at once,** with no fade, so a render set before its first
  block starts as set. Any later change crossfades over 10 ms. Before the fade the new filters are
  run over the last 100 ms of input, so they start where they would have been; a step on a bass
  note doesn't thump (Warren, 2026-11-12).
- **Two channels go through `process2`,** not two `process` calls on one unit: the pre-roll runs
  only the channels the unit has seen in use. DUAL mode is two units, each with `process`.
- **The same bits at any block size** holds when settings are applied at the same sample
  positions. A render with one settings file sets once, before the first block, so any block size
  gives the same file. The test to write is sonorack's own: a synthetic signal (`as-envelope`),
  rendered at blocks of 1, 64 and 1,024, compared by `to_bits`.
- **Rates:** 44.1, 48 and 96 kHz are tested; 88.2 and 192 should work untested.
- **Out is the input to the bit:** with `eq.in = out`, or every band flat and the trim at 0 dB, the
  render is a copy of the input. That's worth a test of its own: it proves the file path adds
  nothing.

## To bring into step while there

- `CLAUDE.md`, "Project state": "nothing is built", and "waits on `as-eq` … after the suite's s1 and
  s2". The EQ's steps went ahead of s1 and s2 (Warren, 2026-11-12), and `as-eq` exists.
- `CLAUDE.md`'s order and `docs/dev/TODO.md`: `sonorack measure` no longer comes first. It was
  settled to go ahead of the EQ, with the bench's spec. Now it waits until after the RACK voicings
  (Warren, 2026-11-12; below). Step 1 is next.
- CI from `../ci-template.yml`, with the repository's first `Cargo.toml`.

## Warren's note on the compressors (2026-11-12)

> With the compressors and de-esser, I'd like to hear an AI interpretation of the specs before I
> add in hardware-informed compression models.

So the RACK voicings, as `crates/dynamics/docs/spec-deess.md`, `specs/opto.md` and `specs/fet.md` have them, are each built and
heard first. The LA-4 and KT76 voicings, fitted to his measured units, come only after all three
RACK voicings have passed his ears. As `CLAUDE.md` has it now, the LA-4 is measured and voiced
right after the optical's RACK voicing (step 6), before the FET's (step 7). The note moves both
hardware voicings after step 7. **Measuring waits too** (Warren, 2026-11-12): `sonorack measure`,
the loopback at the rig and both units measured all come after step 7, ahead of the LA-4 and KT76
voicings. Write that into the order.

## Step 5: the de-esser (2026-11-12)

Written by the suite's session after step 5 here (`as_dynamics::deess`; its spec is
`crates/dynamics/docs/spec-deess.md`, "As built" at its end). Pin the suite at the step's commit or
later. **Opus 5.5, no advisor**, as step 1: the unit's tests pin the hard parts.

- `DeEsser` as `Eq`: `new(rate)`, `set(&Settings)`, `process`, `process2`, `settings()`,
  `settled()`, `readouts()`; `Settings` by id (`in`, `freq`, `thr`, `range`, `mode`, `listen`),
  `controls()`. Its settings file keys as the EQ's, `deess.thr = -20`. **Out by default.**
- **One detector for both channels:** two channels through `process2`. DUAL is two units.
- **Readouts:** `gr_db`, `acting` (a light), and `ess_db`, the number the threshold is compared with,
  for a bar with the threshold drawn across it. `response(&s, rate, hz, g)` draws the bell at a
  depth `g = 10^(−gr_db/20)`.
- **Listen** is a setting like the rest; sonorack's snapshots write it off.
- **For Warren's gate,** beside the spec's three faults: a voice as bright as the RFC's test tone
  reads `ess` near −19 dB with no ess in it, over the default threshold of −20, so the default may
  touch bright vowels by a dB or so. If that's what's heard, the default moves toward −16 (a
  decision for him, then a line in the spec).
