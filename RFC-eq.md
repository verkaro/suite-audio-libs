# RFC: as-eq, a four-band channel EQ

From sonorack's session, 2026-11-12, at Warren's asking: the EQ's shape, for a spec session to
build from without coming back for the idea. **It replaces the "`as-eq`" section of
`RFC-eq-dynamics.md`** (six general slots, 2026-11-11). Everything else there stands, and so does
what it says "all units" share: `f64`, per sample, the same to the bit at any block size, settings
by value and held to range, ramps, two channels, display without audio. Nothing here is code, and
no figure is final until the spec's tests pin it.

**Settled (Warren, 2026-11-12), all as recommended in the session:**

1. Four bands: a low and a high band, each a shelf switchable to a bell, and two bells for the mids
   that overlap in the middle of the voice. Not band-pass: a band boosts or cuts around its
   frequency and leaves the rest alone.
2. The manner of an SSL 4000-style channel EQ, as "style", never "model", as the compressors are.
3. Proportional Q on the bells: broad for a small move, narrower for a big one.
4. A high-pass ahead of the bands, for rumble. No low-pass.
5. Clean: no saturation in the EQ. Colour, if it's wanted, is the FET's stage, or a later decision.
6. **Every control reachable from the keyboard alone,** and the FaderPort later. The crate makes
   that easy by describing its controls as data (below), so a panel's keys, a page, a typed value
   and a surface's encoder all step the same way.

## Why this shape

Warren wants a musical EQ that responds like a good analog one, and doesn't want to learn filter
theory to use it. A console's channel EQ is what answers that: few knobs, ranges chosen so every
setting is useful, and a manner that forgives a big move. Six general slots ask the user to build
an EQ; four fixed bands are one.

What makes an analog EQ sound "musical", and how each is kept here:

- **Proportional Q.** On many analog bells the width depends on the gain: a 2 dB lift is broad and
  gentle, a 12 dB one is focused. A digital bell that keeps its width at every gain makes big moves
  sound harsh and small ones sound thin. Here the bells' width follows the gain (below).
- **Minimum phase.** Analog filters shift phase near their frequency, and the ear is used to it.
  Every section is `as-biquad`'s, minimum phase, from its analog prototype. No linear phase.
- **An open top.** A plain digital bell or shelf high up is squeezed by the sample rate (it
  "cramps"), so a boost at 12 kHz sounds closed. Here the high bands use the matched designs
  `RFC-eq-dynamics.md` already plans (`peak_matched`, `shelf_matched` in `as-biquad`).
- **A shelf with a little shape.** An analog shelf often overshoots a little just past its corner
  and dips a little on the other side, which is much of why a top lift sounds sweet rather than
  hissy. Here the shelf has a small fixed bump, part of its shape, not a knob.

## The bands

Processed in this order, the same coefficients on both channels:

**high-pass → low → low-mid → high-mid → high → output trim**

| Band | Kind | Frequency (log) | Gain | Width | Default |
|---|---|---|---|---|---|
| High-pass | 2nd or 3rd order Butterworth | 20 to 400 Hz | — | slope 12 or 18 dB an octave | out, 80 Hz, 18 |
| Low (LF) | shelf, or bell by its switch | 30 to 450 Hz | −15 to +15 dB | shelf: fixed; bell: fixed base Q 0.7 | in, 100 Hz, shelf, 0 dB |
| Low-mid (LMF) | bell | 200 Hz to 2.5 kHz | −15 to +15 dB | Q 0.4 to 4 | in, 400 Hz, 0 dB, Q 1.0 |
| High-mid (HMF) | bell | 600 Hz to 7 kHz | −15 to +15 dB | Q 0.4 to 4 | in, 3 kHz, 0 dB, Q 1.0 |
| High (HF) | shelf, or bell by its switch | 1.5 to 16 kHz | −15 to +15 dB | shelf: fixed; bell: fixed base Q 0.7 | in, 10 kHz, shelf, 0 dB |
| Output trim | gain | — | −12 to +12 dB | — | 0 dB |

And for the whole: in or out. Each band, and the high-pass, also has its own in or out, so the ear
can compare a band with and without it, keeping its setting. **In or out is a crossfade,** as the
whole's bypass is (`RFC-eq-dynamics.md`, "Ramps").

The frequency ranges are the SSL 4000's as remembered in the session, **to be checked by the spec
session** against the unit's published figures; they are the style's, so a figure a little off is
no harm, but say where they came from. The mids overlap from 600 Hz to 2.5 kHz, where a voice's
body and its presence meet. Q reaches 4 on the mids, narrower than the console's, so a ring or a
hum's harmonic in dialogue can be notched (Question 1, settled).

**The high-pass** is the one band with no gain. Second order is Butterworth; third is a first-order
section and a Butterworth second, as `RFC-eq-dynamics.md` had it. 6 and 24 dB are dropped: 6 is too
gentle for rumble and 24 rings on a voice. Its frequency is its −3 dB point.

**A band at 0 dB** is unity whatever its width, so it's passed over and the whole nulls with the
input (the test below).

### Proportional Q

The width a user sets is the bell's width **at full gain (±15 dB)**. As the gain comes toward 0 dB
the bell broadens, smoothly. The spec writes the law; what it must meet:

- at ±15 dB, the Q is the Q set;
- at ±3 dB, about half the Q set (twice as broad), within a quarter;
- monotonic in |gain| and continuous, with no step as the gain crosses any value;
- **the same for a boost and a cut of the same size,** so +g and −g at the same frequency and Q
  are exact inverses (a peaking bell built as the usual analog prototype, H(s) =
  (s² + s·A/Q + 1) / (s² + s/(A·Q) + 1), has this, and the law must depend on |g| only to keep it);
- the low and high bands in bell mode use the same law on their fixed base Q.

A starting law to try: Q_eff = Q · (0.4 + 0.6 · |g| / 15). The spec may choose another and says why.

### The shelves

The shelf's frequency is where it reaches half its gain in dB. Its shape is the analog second-order
shelf with a Q a little above 0.707, so it has a small bump past the corner and a small dip before
it. The spec picks the Q; what it must meet:

- the plateau reads the dial within 0.1 dB, at DC for the low shelf and at a fifth of the rate for
  the high shelf;
- the bump at ±15 dB is between 0.5 and 1.5 dB, and scales with the gain (a 3 dB shelf has a
  fifth of it);
- boost and cut of the same size are exact inverses, as the bells are.

Warren's ear judges whether the bump is right (the gate in sonorack). If he wants it gone, it's the
Q set to 0.707; it stays one figure, not a knob.

### Near the top

As `RFC-eq-dynamics.md` planned, unchanged: bells and shelves above a sixth of the rate use the
matched designs, added to `as-biquad` first (`peak_matched`, `shelf_matched`, Vicanek 2016 the
reference), and the spec proves a band swept across the change moves by less than 0.1 dB at any
frequency. At 44.1 kHz that's above 7.35 kHz, so the HF band and the top of the HMF use them.

## The controls as data: for the keyboard and the FaderPort

Keyboard-only control is a rule for the whole family (`STYLE.md`: nothing hidden behind a pointer).
For an EQ it means more than "there's a key": stepping a frequency or a gain by keys has to land on
useful values, read clearly, and agree with how a page's drag, a typed value and later a surface's
encoder step. So the crate describes its controls, and every client reads the description. This
is the one thing here beyond `RFC-eq-dynamics.md`'s plan.

**Each control** (a band's frequency, gain, Q, shelf-or-bell, in-or-out; the high-pass's
frequency, slope and in-or-out; the trim; the whole's in-or-out) is described by:

- an id that never changes (`lf.freq`, `lmf.q`, …, the spec chooses the spelling), for settings
  files and protocols;
- a short label for a panel (`LF`, `FREQ`, `GAIN`, `Q`, `BELL`, `IN`) and a long one for `?` and a
  page's title text;
- its kind: continuous, or a switch with named positions;
- its range, its default, and **its detent** (0 dB for a gain, the default for the rest): the
  value a "back to the middle" key returns to;
- **its fine and coarse step,** in the control's own scale:

| Control | Fine step | Coarse step |
|---|---|---|
| Frequency | 1/12 octave | 1/3 octave |
| Gain | 0.5 dB | 3 dB |
| Q | 1/6 octave of Q (a log step) | 1/2 octave of Q |
| Trim | 0.5 dB | 3 dB |

A stepped value lands on the step's grid (from the detent for a gain, from the range's low end on a
log grid for a frequency), so stepping up and back returns exactly where it was, and the same number
of key presses always gives the same value.

- **its format,** one function both ways: a value to text (`1.2 kHz`, `+3.0 dB`, `Q 1.4`, `BELL`)
  and text to a value, forgiving: `1200`, `1.2k`, `1k2`, `1.2 kHz` all read 1200 Hz; `3`, `+3`,
  `3dB` read +3 dB. A value typed out of range is held to it and shown held, never refused.

That's what lets a panel be used entirely by keys: move to a control, step it fine or coarse, send
it to its detent, or type it. The keys are sonorack's panel's to choose, inside `STYLE.md` (a sketch
for that session, not settled):

- Tab, the next unit in the rack; `j` `k`, the next control in the unit and the one before;
- `h` `l` (and ← →), a fine step down and up, heard at once (a tier-1 move: it ramps, never clicks);
- Shift with `h` `l` or ← →, a coarse step, never a jump to the range's end (Question 2, settled);
- `0` sends the control to its detent;
- `i` types a value in place, Enter keeps it, Esc leaves, as the family's common keys have it;
- a band's in-or-out and shelf-or-bell on a letter each, listed in `?`;
- every value spoken in full in the status line as it changes (`LMF  1.20 kHz  +3.0 dB  Q 1.4`),
  so a move can be followed without looking at the curve.

Holding `h` or `l` repeats at the keyboard's rate. The crate has to take that: a new setting can
arrive every few milliseconds while a 10 ms crossfade is running. The spec writes how (the usual
way: a crossfade runs to its end, and the latest target waiting starts the next one, so a held key
glides and nothing is dropped but the steps between).

**The FaderPort, later.** sonofader2 makes the rack a desk when its turn comes (`Sonofader2.md`).
With the controls as data, its encoder steps the chosen control by the same fine step (coarse with
SHIFT), its fader can be the chosen band's gain or the trim on the same taper, and its buttons can
move between bands. Nothing in `as-eq` knows of a surface; the description is what makes it cheap.

## Display without audio

As planned: `response(hz) -> dB` for the whole and for each band, so a panel draws the curve and
each band's own hill without running sound. And one thing more, for a panel that can't draw well
on a small screen: each band's frequency, gain and width are enough to draw a marker on the curve
where the band sits.

## What "good" means, pinned by tests

From `RFC-eq-dynamics.md`, kept: at 44.1, 48 and 96 kHz, every kind at frequencies across its range
reads its gain at its centre (bell), its plateau (shelf) or −3 dB at its corner (high-pass) within
0.1 dB, by `response(hz)` and by a sine; a 16 kHz bell at +6 dB, Q 2, at 48 kHz within 0.5 dB of
the analog prototype from 8 to 20 kHz; everything at 0 dB or out nulls with the input within 1e-12;
the response of the whole is the product of the bands within 1e-9; a ramp test on a gain step and a
frequency step; block sizes 1, 64 and 1,024 the same to the bit; a run twice the same; no
non-finite sample on silence, DC, full-scale noise or a −300 dBFS tail.

New with this shape:

- **Boost and cut are inverses:** the LMF at +9 and an EQ of the same at −9 in series null within
  1e-9, at five Q settings; the same for each shelf and each band's bell mode.
- **Proportional Q:** the −3 dB-from-peak width of a bell at ±3, ±9 and ±15 dB, against the law,
  within 5%.
- **The shelf's bump:** its size at ±15 and ±3 dB within the bounds above.
- **The mids overlap:** LMF and HMF both at 1 kHz, +6 each, read +12 at 1 kHz within 0.1 dB.
- **A held key:** settings changed every 2 ms for a second, a frequency swept across the band's
  range; no sample-to-sample step beyond the ramp's bound, no broadband burst above −80 dBFS, and
  the final state the last setting sent.
- **The controls:** every control's step, from its detent, up n and down n returns to it exactly;
  format then parse returns the value for every step of every control; the forgiving inputs above
  parse as said; out of range is held.

## Order of work

Unchanged from `RFC-eq-dynamics.md`: the matched designs in `as-biquad` first, then `as-eq`, in a
suite session after s1 and s2, with its `docs/spec.md`, README and `VENDORED.md` ("written here").
sonorack takes it at its step 1 (`../Sonorack.md`), and the first gate is Warren's: a voice through
the EQ, by ear, with the shelf's bump and the proportional Q the things to listen for.

## For the spec session to decide (not Warren)

- The proportional-Q law, and the shelf's Q.
- The frequency ranges, checked against the console's figures.
- The controls' ids and the struct and function names, in the suite's plain voice.
- How a held key's stream of settings meets the crossfade.
- Whether the step grid lives in `as-eq` alone or a small shared type, if `as-dynamics` wants the same
  (it will: its knobs need the same description). If both want it, it's one module both use, said in
  `CHANGES.md`, not a third crate unless a third tool wants it.

## Questions for Warren

1. **Q to 4 on the mids,** narrower than the console's, for notching a ring or hum in dialogue?
   Recommended: yes. Wider than that is a surgical tool, which the rack isn't.
2. **The coarse step's key.** The family's `H` `L` mean "the first, the last". On a value that would
   be a jump to +15 dB, which is loud and unasked. Recommended: on a value, Shift with ← → or `h` `l`
   is the coarse step, never the range's end, written into `STYLE.md` as the meaning of `H` `L` on a
   value, for every tool that steps values. It's a family key, so it's yours to settle.

**Settled (Warren, 2026-11-12):** both as recommended. Q reaches 4 on the mids. On a value, Shift
with ← → or `h` `l` (so `H` `L`) is the coarse step, never the range's end; `STYLE.md`'s line for it
is the family's session's to write.
