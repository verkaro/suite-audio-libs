# Spec: `as-eq`, and `as-biquad`'s matched designs

From the suite's session, 2026-11-12. Read `specs/shared.md` first: its rules hold here and
aren't repeated. The shape is `RFC-eq.md` (settled 2026-11-12); this chooses what it left to the
spec. Moved here from `specs/eq.md` when the crate was made (step 3, 2026-11-12); the `as-biquad`
part came with it as the record of why those designs were added. "As built", at the end, says what
the builder chose and where it differs.

## What was decided here, and why

The figures behind these were worked out in the session with a short prototype of the designs
against their analog prototypes (pure arithmetic, no audio). The builder re-derives them in tests;
these are what to expect.

1. **Every bell and shelf is a matched design, at every frequency.** The RFC used the bilinear
   transform below a sixth of the rate and the matched designs above, and asked for proof the
   switch moves a band less than 0.1 dB. The prototype showed the matched design is closer to the
   analog bell than the bilinear one **everywhere**, not only near the top: at 44.1 kHz, a 3 kHz
   bell at ±15 dB, Q 0.4, is within 0.5 dB of the analog curve matched and 2.4 dB off bilinear;
   at 1 kHz it's 0.13 against 0.39. So there's no crossover to prove, no switch to sweep across,
   and one design to test. **Differs from the RFC.**
2. **A cut is the boost's exact reciprocal.** Every bell and shelf is designed at |gain| and, for a
   cut, inverted (numerator and denominator swapped, normalised to `a0 = 1`). The matched design's
   zeros are inside the unit circle (it takes the minimum-phase root), so the inverse is stable.
   That makes boost and cut exact inverses to rounding, which the RFC demands and which a matched
   design built at −g would not give, and it keeps the law depending on |g| only.
3. **Proportional Q:** `Q_eff = Q · (0.4 + 0.6 · |g| / 15)`, the RFC's starting law. At ±15 dB it's
   Q; at ±3 dB it's 0.52 Q; linear in |g|, so monotonic and continuous. Kept because it meets every
   requirement and is the simplest that does.
4. **The shelf's Q is 1.0.** The analog second-order shelf (the cookbook's form, its frequency the
   point of half the gain in dB) at Q 1.0 bumps **0.90 dB** past the corner at ±15 dB, 0.63 at ±9,
   **0.23 at ±3**, and dips the same before it. The RFC's bounds were 0.5 to 1.5 dB at ±15 (met)
   and "a fifth of it at 3 dB" (it's about a quarter). **Differs from the RFC** in that one figure;
   the test pins a quarter.
5. **The width a bell is tested by** is its bandwidth between the points of **half its gain in dB**,
   the cookbook's definition, which for this bell doesn't change with gain: an analog bell of Q has
   `2·asinh(1/(2Q))/ln 2` octaves. The RFC's "−3 dB from the peak" can't be measured on a 3 dB bell
   (its edges are at 0 dB). **Differs from the RFC.**
6. **Near the top, what a second-order section can't do, it doesn't claim.** A high shelf at 16 kHz,
   +15 dB, at 44.1 kHz puts most of its rise past Nyquist; no biquad follows the analog curve there,
   and the prototype's best is 2.4 dB off near 20 kHz. Below a corner of 10 kHz it's within 0.3 dB.
   The tests say both. Warren's ear judges the top shelf at sonorack's gate.

## `as-biquad`: what's added

With a `CHANGES.md` line ("added; nothing for the tools to do"), tests in `as-biquad`'s own file.

- **`Biquad::matched(b, a, f0, rate, match_hz)`** (the name is the builder's): any analog
  second-order section, its poles by impulse invariance (as `lowpass_matched` already places them),
  its numerator chosen so the digital magnitude equals the analog one **at DC, at Nyquist and at
  `match_hz`**, the minimum-phase root taken. This is Vicanek's method (*Matched Second Order Digital
  Filters*, 2016), the same as `lowpass_matched` with one more match point. If a setting asks for a
  magnitude the section can't reach (the square root's argument below 0), it's taken as 0 and the
  design goes on; the tests below never reach that, and one test says so over the whole grid.
- **`Biquad::peak_matched(hz, q, gain_db, rate)`**: the bell `(s² + s·A/Q + 1) / (s² + s/(A·Q) + 1)`,
  `A = 10^(|g|/40)`, matched at its centre; inverted for a cut.
- **`Biquad::low_shelf_matched` and `high_shelf_matched(hz, q, gain_db, rate)`**: the cookbook's
  second-order shelves (half the gain in dB at `hz`), matched at the corner for the low shelf, and
  at `min(0.75·hz, rate/6)` for the high shelf, which the prototype found best across the HF band's
  range; inverted for a cut.
- **`Biquad::inverse()`**: the reciprocal section, as `First::inverse` is.

**Pinned by tests** (at 44.1, 48 and 96 kHz):

- each matched design reads the analog magnitude at DC, at Nyquist and at its match point within
  1e-9 dB (by `gain`);
- a section and its `inverse()` in series null an impulse within 1e-12;
- every design on the grid (bells at 20 Hz to 16 kHz by thirds of an octave, ±0.5 to ±15 dB, Q_eff
  0.15 to 4; shelves the same over their bands' ranges) is stable, minimum phase and finite, and
  never takes the "can't reach" path;
- against the analog prototype from 20 Hz to 20 kHz (or 0.45 of the rate if lower): a bell within
  0.6 dB where its centre is at or below 7 kHz and Q_eff at or above 0.7, within 1.0 dB at any Q
  at or below 10 kHz, and within 1.8 dB at 16 kHz; a low shelf within 0.05 dB; a high shelf within
  0.35 dB where its corner is at or below 10 kHz, within 2.5 dB above. (The prototype's worst cases
  on a coarser grid were 0.56, 0.80, 1.59, 0.00, 0.30 and 2.37.)
- **The top** (the RFC's test, here because no EQ band has a settable Q at 16 kHz):
  `peak_matched(16 kHz, Q 2, +6 dB)` at 48 kHz within 0.5 dB of the analog bell from 8 to 20 kHz.
  (The prototype read 0.48.)

**As built (step 2, 2026-11-12).** `Biquad::matched(b, a, f0, rate, match_hz)`, `peak_matched`,
`low_shelf_matched`, `high_shelf_matched` and `inverse`, as named above. Two figures moved, each
measured:

- **The match points within 1e-8 dB,** not 1e-9. A section at 20 Hz at 96 kHz has numerator
  coefficients near 1 that sum to about 1.7e-6 at DC, so f64 rounding in the coefficients alone
  allows about 2e-9 dB there; a Q 4, +15 dB bell read 1.1e-9. Nothing the design can do about it;
  far below hearing.
- **A bell within 0.7 dB** where its centre is at or below 7 kHz and Q_eff at or above 0.7, not
  0.6. The grid by thirds found 0.651 dB at 6.45 kHz, Q 0.7, ±15 dB, at 44.1 kHz (0.609 at 48),
  where the prototype's coarser grid had 0.56. The rest held: 0.80 (bound 1.0), 0.93 (1.8), 0.002
  (low shelf, 0.05), 0.30 (high shelf to 10 kHz, 0.35), 1.89 (above, 2.5).

And one thing the prototype didn't show: Vicanek's formula for the numerator, as written, takes
`(b0 − b2)²` as the difference of two numbers near 4 when the poles and zeros crowd DC, and lost
three digits on a 40 Hz bell (2e-9 dB off at 44.1 kHz, 1.2e-8 at 96). It's computed instead from
the poles one at a time, with `1 − r` kept apart (`expm1`), as `(want − (r0·φ0 − r1·φ1)²)/(4φ0φ1)`.
`lowpass_matched` shares the pole placement and gives the same bits as before.

## `as-eq`

### The layout

A **layout** is a list of bands, each with its kind and ranges; the four-band channel EQ is one
layout, `Layout::CHANNEL` (named for what it is, not for SSL). A later voicing (the 551E,
`RFC-eq.md`, "Later") brings a layout of its own. So:

- `Settings` holds a fixed array of band settings, sized for the largest layout the crate knows
  (six is plenty: the 551E has five and a high-pass), plus the trim and In. No `Vec`: settings are
  `Copy`.
- `Settings` names its layout; `set` with a different layout is a crossfade like any change.
- The controls' descriptions come from the layout.

### The bands, in `CHANNEL`

Processed in this order, the same coefficients on both channels:

**high-pass → LF → LMF → HMF → HF → trim**

| Band | Kind | Frequency (log) | Gain | Width | Default |
|---|---|---|---|---|---|
| HPF | Butterworth, 2nd or 3rd order | 20 to 400 Hz | — | slope 12 or 18 dB an octave | out, 80 Hz, 18 |
| LF | shelf, or bell by its switch | 30 to 450 Hz | ±15 dB | shelf Q 1.0; bell base Q 0.7 | in, 100 Hz, shelf, 0 dB |
| LMF | bell | 200 Hz to 2.5 kHz | ±15 dB | Q 0.4 to 4 | in, 400 Hz, 0 dB, Q 1.0 |
| HMF | bell | 600 Hz to 7 kHz | ±15 dB | Q 0.4 to 4 | in, 3 kHz, 0 dB, Q 1.0 |
| HF | shelf, or bell by its switch | 1.5 to 16 kHz | ±15 dB | shelf Q 1.0; bell base Q 0.7 | in, 10 kHz, shelf, 0 dB |
| Trim | gain | — | ±12 dB | — | 0 dB |
| In | in, out | | | | in |

**The ranges** are the RFC's, which are the E-series channel EQ's as commonly published (LF 30 to
450 Hz, LMF 0.2 to 2.5 kHz, HMF 0.6 to 7 kHz, HF 1.5 to 16 kHz, ±15 dB). The session had no manual
to check them against. The builder checks one if one is at hand, and notes the source in the
crate's README; a few percent off is no harm, since it's the style's. The HPF's 20 to 400 Hz is the
rack's own (the console's is narrower), kept as settled. Q to 4 on the mids is the rack's own, for
notching a ring (Warren, 2026-11-12).

**The HPF:** `as-biquad`'s Butterworth sections (`butterworth`, `Biquad::highpass`,
`First::highpass`), bilinear, which is exact enough at 400 Hz and below. Its frequency is its
−3 dB point.

**A band at 0 dB, or out, isn't run.** It's left out of the chain, so the whole nulls with the input
to the bit when every band is flat or out and the trim is 0 dB.

### Changes: the crossfade, written out

The EQ holds **two filter sets** (every band's sections, their state, for both channels). One is
live. A change of any EQ setting but the trim (frequency, gain, Q, slope, a band's In, shelf or
bell, the layout) builds the coefficients of the new set in `set()`, and:

- **if no crossfade is running:** the new set's state is copied from the live set's (each band's
  sections from the same band's, where the band exists in both and has the same number of sections;
  zero otherwise), and a 10 ms crossfade starts from the live set's output to the new one's;
- **if one is running:** the setting waits, replacing any setting already waiting.
- When a crossfade ends, the new set is live; if a setting is waiting, it starts the next.

Copying the state means the new filter starts where the old one is, so a 30 Hz shelf doesn't start
from rest and thump. The whole EQ's In is the same crossfade, between the live set's output and the
dry input. The trim is a gain ramp (5 ms), not a crossfade.

So a held key sweeping a frequency hears a glide in 10 ms hops, and the coefficients never pass
through an unstable set, because they're never interpolated.

### Display without audio

- `response(&Settings, rate, hz) -> dB`, the whole, and `band_response(&Settings, band, rate, hz)`,
  each from **the digital sections' own coefficients** (`Biquad::gain`), so the curve drawn is what
  is heard, to rounding. No state; callable off the audio thread with no unit.
- `marker(&Settings, band) -> (hz, db)`: where to draw a band's marker on the curve (a bell's
  centre, a shelf's corner at half its gain, the HPF's corner at −3 dB).

### The controls

Ids, local to the unit, as `shared.md` says. Steps from the RFC's table.

| Id | Short | Kind | Range | Default and detent | Fine | Coarse |
|---|---|---|---|---|---|---|
| `in` | IN | switch: out, in | | in | | |
| `hpf.in` | IN | switch: out, in | | out | | |
| `hpf.freq` | FREQ | log, Hz | 20 to 400 | 80 | 1/12 oct | 1/3 oct |
| `hpf.slope` | SLOPE | switch: 12, 18 | | 18 | | |
| `lf.in`, `lmf.in`, `hmf.in`, `hf.in` | IN | switch: out, in | | in | | |
| `lf.freq` | FREQ | log, Hz | 30 to 450 | 100 | 1/12 oct | 1/3 oct |
| `lf.gain` | GAIN | linear, dB | −15 to +15 | 0 | 0.5 | 3 |
| `lf.bell` | BELL | switch: shelf, bell | | shelf | | |
| `lmf.freq` | FREQ | log, Hz | 200 to 2,500 | 400 | 1/12 oct | 1/3 oct |
| `lmf.gain` | GAIN | linear, dB | −15 to +15 | 0 | 0.5 | 3 |
| `lmf.q` | Q | log, Q | 0.4 to 4 | 1.0 | 1/6 oct | 1/2 oct |
| `hmf.freq` | FREQ | log, Hz | 600 to 7,000 | 3,000 | 1/12 oct | 1/3 oct |
| `hmf.gain`, `hmf.q` | | as the LMF's | | | | |
| `hf.freq` | FREQ | log, Hz | 1,500 to 16,000 | 10,000 | 1/12 oct | 1/3 oct |
| `hf.gain`, `hf.bell` | | as the LF's | | | | |
| `trim` | TRIM | linear, dB | −12 to +12 | 0 | 0.5 | 3 |

The long labels: "High-pass", "Low", "Low-mid", "High-mid", "High", "Trim", with the control after
("Low-mid frequency").

## Readouts

`in_peak_db`, `out_peak_db`, `gr_db` always 0. Nothing more: the curve is `response`.

## Pinned by tests

At 44.1, 48 and 96 kHz unless said:

- **Readings:** every band kind, at five frequencies across its range and gains of ±3, ±9, ±15 dB,
  reads its gain at its centre (bell), the dial at DC (LF shelf) and at Nyquist where the corner is
  at or below rate/8 (HF shelf), half its gain at the corner (LF shelf), and −3 dB at its corner
  (HPF), each within 0.1 dB, by `response` and by a sine through the unit
  (`as_envelope::measure::tone`). The HF shelf, matched below its corner, reads half its gain at
  the corner within 0.25 dB where the corner is at or below rate/8.
- **The top:** the RFC's 16 kHz bell test is `as-biquad`'s (above), since no band here has a Q to
  set at 16 kHz. Here, the HF band in bell mode at 16 kHz, ±15 dB, at 44.1 kHz, within 1.8 dB of the
  analog bell of its Q_eff from 20 Hz to 20 kHz.
- **Flat is a null:** every band at 0 dB or out, trim 0 dB: the output is the input to the bit.
  (The RFC asked 1e-12; leaving flat bands out of the chain makes it exact.)
- **The whole is the product:** `response` equals the sum in dB of `band_response` within 1e-9, at
  random settings.
- **Boost and cut are inverses:** the LMF at +9 dB and a second EQ with it at −9 dB, in series, null
  an impulse within 1e-9, at Q 0.4, 0.7, 1, 2 and 4; the same for each shelf and each band's bell
  mode, at three frequencies each.
- **Proportional Q:** a bell's bandwidth between its half-gain points, measured on `response` at
  1 kHz and 48 kHz, is the analog bandwidth of Q_eff within 5%, at ±3, ±9 and ±15 dB, at Q 0.5, 1
  and 4.
- **The shelf's bump:** at ±15 dB, between 0.8 and 1.0 dB; at ±3 dB, between 0.18 and 0.28 dB; dip
  the same, measured on the LF shelf at 100 Hz and 48 kHz.
- **The mids overlap:** LMF and HMF both at 1 kHz, +6 dB each, Q 1: +12.0 dB at 1 kHz within 0.1.
- **The crossfade:** a 1 kHz sine with the LMF stepped 0 to +12 dB once: no step beyond the rig's
  bound, no burst (`shared.md`, the held-key rig's two checks), and the output equals the new set's
  steady output 10 ms after the step's sample, within 1e-9 (once the 400 Hz bell's state has
  settled, 200 ms after it).
- **A held key** (`shared.md`'s rig), on `lmf.freq`, `hf.gain`, `hpf.freq` and `lf.bell` toggled
  every 2 ms.
- **The state copy:** a 40 Hz sine at −6 dBFS through the LF shelf at 30 Hz stepped 0 to +6 dB: no
  burst, against the same with the new set starting from rest (which the test shows would burst,
  so the test proves the copy matters).
- The controls (`shared.md`), block sizes, twice, non-finite, denormal timing.

## Warren's gate (sonorack's step 2)

A voice through the EQ, by ear, from his own patching. What to listen for, which the session that
builds hands him: the shelf's bump on a top lift (sweet, or hissy?), the proportional Q (a big
mid boost focused, a small one broad and gentle), the HF shelf at 16 kHz at 44.1 kHz, and no click
or zip from any key held.

## As built (step 3, 2026-11-12)

The names are as above: `Eq` (`new`, `set`, `process`, `process2`, `settings`, `settled`,
`readouts`), `Settings` (`get`, `set_by_id`, `held`, its fields public and held again by `set`),
`Layout::CHANNEL`, `controls()`, `response`, `band_response`, `marker`, and `q_eff`, the law.
`Settings` holds `[Band; 6]`, each band's `on`, `freq`, `gain`, `q`, `bell` and `steep` (the
high-pass at 18); a field its layout gives no control is ignored. `response` includes the trim,
so it's the sum of `band_response` plus the trim; both read 0 dB where the EQ is out. `marker`
gives where a band is set, in or out, without a rate.

**The new set is run over the last 100 ms of input, not given the old set's state** (Warren,
2026-11-12). **Differs from the spec above** ("Changes: the crossfade, written out"), measured:

- A band at 0 dB isn't run, so on the state-copy test's own step (0 to +6 dB) there was no state
  to copy; and where there was (+3 to +9 dB), the copied direct-form memory, which fits the old
  coefficients, thumped more than rest did. A 40 Hz sine at −6 dBFS, the LF shelf at 30 Hz, at
  48 kHz, the step's peak against the steady peaks after it: copied 1.26 (+3 to +9), 1.67 (0 to
  +15); from rest 1.19 and 1.28.
- Neither burst above 4 kHz (−108 to −137 dB): a thump here is low, so the burst check can't tell.
- So each set keeps the last 100 ms of input, each channel (allocated in `new`), and a new set
  starts from rest and runs over it before the crossfade: it joins its steady state, as if it had
  run all along. A 30 Hz shelf at +15 dB settles within 0.2% in that time. The cost is the pre-roll
  per change, at most one per 10 ms: about 0.1 ms at 48 kHz, two channels, by estimate (not timed).
- **The test** (`the_new_set_starts_settled`), 0 to +6 and 0 to +15 dB at each rate: the output
  within 0.005 of the old and new sets' steady outputs crossfaded (found 0.0007 at most), its peak
  within 0.5% of theirs (found 1.0001), nothing above 4 kHz over −80 dB; and from rest, the same
  step strays by more than 0.03 and peaks more than 5% over (found 0.047 to 0.39, 1.07 to 1.50),
  so the test shows the pre-roll matters.

**The HF shelf reads its dial at Nyquist where its corner is at or below rate/20,** not rate/8.
The analog shelf at Q 1.0 is still over its plateau there: at ±15 dB, 0.47 dB at 4 times its
corner, 0.13 at 8, 0.08 at 10. So between rate/20 and rate/8 the test reads Nyquist against the
analog shelf there, within 1e-6 dB (the match point), and the dial only from rate/20 down. Half
the gain at the corner, within 0.25 dB up to rate/8, holds as written.

**A unit that has run nothing yet takes its first settings at once,** with no crossfade or ramp:
there's nothing to fade from, and a fresh unit set to a setting is the reference the rigs compare
against.

**Readings found:** every reading within 0.13 dB of its target where 0.25 is allowed (the HF
shelf's corner), within 0.1 elsewhere; the HF bell at 16 kHz, ±15 dB, 44.1 kHz, within 1.59 dB of
the analog bell; the shelf's bump 0.904 dB at ±15 and 0.229 at ±3; held keys at 0.91 of the
step bound (a step no larger than at either end) and −94 dB or below above 4 kHz.

**The ranges** are the E-series channel EQ's as commonly published; no manual was at hand to check
them against, and the crate's README says so.

**The denormal test** times blocks of a second of noise against the last of ten seconds of
silence after it, with bands whose memory falls past the denormals within that time. Unflushed,
the end is 25 times slower; flushed, no slower.

**For Warren's gate** (sonorack's step 2), what to listen for: the shelf's bump on a top lift
(sweet, or hissy?), the proportional Q (a big mid boost focused, a small one broad and gentle),
the HF shelf at 16 kHz at 44.1 kHz, no click or zip from any key held, and no thump when a low
shelf is stepped on a bass note.
