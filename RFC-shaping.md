# RFC: sound shaping, three units for the family

From the family's session, 2026-11-11, at Warren's asking: a good parametric EQ, an 1176-style
(FET) compressor and an LA-4-style (optical) compressor, as components the tools share and a
tool of their own, fitting the family without straying into a DAW. Read from the suite and the
tools with nothing run. Warren asked for push-back if the wish doesn't hold up; it mostly does,
and where it doesn't is said.

**Settled 2026-11-11** (see the end): all five as recommended, and a de-esser added at Warren's asking.

## Does the wish hold up

**Yes, as units; no, as a mixer.** The family shapes sound today in three places, and none has
these: BRR's lanes have LEVEL, PAN, TUNE and TILT and nothing to sit a voice or a hit in its room
but the room; Tape's chain is the tape's character, with no dynamics before it; place's and irma's
voice through a room goes in untouched, where the usual thing before a room is a high-pass and a
little compression. ducky fits EQ and fader moves to a board, but to recommend them for a DAW,
not to apply them. Dexter has none by its Scope, and that stands. So "missing in several
instances" is right: a voice before a room, a hit on a lane, a stem before tape.

Where it wouldn't hold:

- **In the deck.** Dexter is a tracking station (its Scope, 2026-09-27): nothing is shaped on
  the way to disk, and the monitor mixes are the interface's. These units never go there.
- **As models of the units themselves.** An 1176 and an LA-4 are circuits with transformers,
  a FET and a photocell, and a model that claims their sound can't be proved here: there's no
  unit on the bench to null against, and the family's tests use no recordings. What can be
  built and proved is the *behaviour*: the knobs, the ratios, the time constants, the feedback
  and the cell's memory that make each one sit on a voice the way it does, and one chosen
  saturation stage measured as Tape measures its own. Recommended: behavioural, named "style",
  never "model". Warren's ears are the gate for the character, as for Tape.
- **As a chain of plugins.** What keeps this short of a DAW: each unit is a box with its knobs,
  a fixed order where two meet (EQ, then a compressor), no host, no automation, no presets format
  beyond the tool's own settings, no routing but the tool's. A tool that wants one puts it in
  one place and says where.

## What's here already

`as-biquad` (minimum-phase first- and second-order sections in `f64`, the same on every machine,
with a matched low-pass), `as-tilt` (one slope, glided), `as-envelope` (test signals, a synthetic
corpus, and the measurements that say a process stayed subtle: gain, THD, aliasing, nulls),
`as-loudness`, and Tape's chain as the pattern for a stage fed blocks of any size, bit for bit.
The EQ is mostly assembly; the compressors are new.

## The three units

**`as-eq`, a parametric EQ.** Four to six bands, each a peak, a low or high shelf, or a high- or
low-pass, on `as-biquad`; a high-pass with a slope chosen by sections. "Good" means, and the tests
pin: the gain at a band's centre reads what the dial says at 44.1, 48 and 96 kHz; the top band
keeps its shape near Nyquist (a matched peak and shelf beside `as-biquad`'s matched low-pass,
decramped as Orfanidis does); bands are independent; a move is ramped by coefficient so nothing
zips; bit for bit at any block size. One `gain(hz)` for a display, so a panel can draw the curve
without running audio.

**The two compressors,** in one crate, `as-dynamics` (Question 1), with the de-esser as its third unit, since they share the gain
computer, the meters and the test signals, and differ in the detector:

- **FET style (the 1176's manner):** feedback topology, so the ratio and the times depend on the
  program as the original's do; input gain against a fixed threshold, output gain; attack from
  tens of microseconds to under a millisecond, release from tens of milliseconds to a second;
  ratios 4, 8, 12 and 20, and the all-buttons state as its own setting; a gain-reduction meter.
  Per sample, no look-ahead, so block size can't show. One soft stage after the gain cell,
  measured by `as-envelope` (THD at the reference, aliasing bounded; oversampled by two only
  if the test demands it).
- **Optical style (the LA-4's manner):** a fixed attack of about ten milliseconds and the cell's
  release in two parts, the first fast and the rest slow, slower the longer it has been held
  down (the memory that makes it sit on dialogue), with the LA-4's switch of ratios rather than
  the LA-2A's one; peak-reduction and gain knobs; the meter. Also per sample.

- **A de-esser** (Warren, 2026-11-11: important to audio drama and podcast work, and left out above):
  a compressor that listens in one band, from about 4 to 9 kHz, where the esses are, and turns down
  either that band alone (split) or the whole voice (wide), by a set amount at most; a frequency and
  width, a threshold, a range, and a listen setting that plays the band alone so the ear finds the
  ess. Fast by nature: attack under a millisecond, release a few tens of milliseconds. Per sample.
  It stands first in the rack, before the compressors, so the esses don't drive them.

All three: deterministic, settings by value with every change of gain ramped, time constants in
seconds so a rate changes nothing a listener would hear (the same to the bit is claimed only at
one rate, as the family claims it).

**The tests,** with no recordings: the static curve (gain reduction against level, each ratio,
within a tenth of a dB on a stepped sine); the attack and release times on tone bursts against the
dial (within a tenth of their value); the optical memory shown on `as-envelope`'s corpus (the
release after two seconds held longer than after a tenth); the de-esser on a synthetic ess (band noise at 6 kHz over a vowel-like tone from `as-envelope`'s
corpus): the band down by the range set, the tone unmoved within a tenth of a dB; the EQ's magnitudes at three rates;
block sizes 1, 64 and 1,024 bit for bit; never a non-finite sample; and `as-envelope`'s acceptance
of the saturation stage.

## Where they're used

| Tool | How | When |
|---|---|---|
| **sonorack** (Question 2) | in → EQ → de-esser → optical → FET → out, on one JACK path, with a page or panel and its settings; and the same chain over a file (`--render`, a new file). The first home, where Warren's ears judge the three, and the one every other tool points at | first, as sonofader2 is for the surface |
| BRR | a lane's insert, EQ and one compressor, printed into the lane's stem as TILT is; a brief to BRR once the rack has passed Warren's ears | after the rack |
| Tape | a stage before the tape, by Tape's RFC and its acceptance tests, if Tape wants it | Tape's decision |
| place, irma | the voice before the room: a high-pass and the optical, as an option on the line | after the rack |
| ducky | the de-esser (Warren, 2026-11-11): its speech analysis finds the sibilance, its fit says the setting, and its rough mix renders it with `as-dynamics`, as its EQ and fader moves are rendered now; by a brief to ducky once the rack has passed Warren's ears. The EQ's curve for drawing a fit, if ducky ever fits to the family's EQ beside the boards | after the rack |
| Dexter | nothing, by its Scope | never |

## In what order

1. **`as-eq`** first: smallest, mostly `as-biquad`, and two tools want it. Its tests prove the
   pattern (the three rates, the ramp, the display curve).
2. **The rack,** with the EQ alone: the JACK path, the page or panel, the settings, `--render`.
   Warren's gate: a voice through it.
3. **The de-esser** in `as-dynamics`, then into the rack. Warren's gate: a sibilant voice.
4. **The optical,** then into the rack. Warren's gate: dialogue.
5. **The FET,** its saturation stage last. Warren's gate: a hit, a voice.
6. Briefs to BRR and ducky, then place and irma.

## Questions for Warren

1. **One crate for both compressors** (`as-dynamics`, modules `fet` and `opto`), or two? Recommended:
   one; the detector's maths, the meters and the tests are shared, and a tool takes one unit or
   both from it.
2. **A rack of its own,** a tool with a JACK path and a file mode, as the first home? Recommended:
   yes; it's where the ears' gate happens, and the pattern sonofader2 and focusrite-ctrl set. Its
   name is yours.
3. **Behavioural, not modelled,** named "style". Recommended: yes, for the reason above.
4. **The order:** EQ, rack, optical, FET. Recommended: as listed; the FET's saturation is the
   one piece that wants the acceptance tests before it's believed.
5. **The LA-4 rather than the LA-2A:** the switch of ratios makes it the more useful unit on a
   voice and a hit alike; the cell's memory is the same idea. Recommended: the LA-4, as asked, with
   the LA-2A's single ratio reachable by the switch.

6. **The de-esser** (added 2026-11-11 at Warren's asking, its shape mine to choose): a third unit in
   `as-dynamics`, split or wide, first in the rack, and in ducky's rough mix by a brief. Taken as
   settled by his words ("add as you see fitting").

## Settled (Warren, 2026-11-11)

All five as recommended, and the sixth as written:

1. One crate, `as-dynamics`: `fet`, `opto` and `deess`.
2. A rack of its own is the first home, with a JACK path and a file mode: **sonorack** (Warren, 2026-11-11; `../Sonorack.md`).
3. Behavioural, named "style".
4. The order: EQ, the rack, the de-esser, the optical, the FET.
5. The LA-4, with the LA-2A's single ratio reachable by the switch.
6. The de-esser as above, and ducky renders it in its rough mix.

Work starts with `as-eq`, in a suite session, after s1 and s2. **`RFC-eq-dynamics.md`** (2026-11-11)
has the units in detail, for the spec.
