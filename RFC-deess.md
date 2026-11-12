# RFC: the de-esser, `as-dynamics`'s `deess`

From sonorack's session, 2026-11-12, at Warren's asking: the de-esser's shape, written the way
`RFC-eq.md` writes the EQ's, for a spec session to build from. **It replaces the "`deess`, the
de-esser" section of `RFC-eq-dynamics.md`** (2026-11-11), which stays as the record. What that RFC
says all units share still holds: `f64`, per sample with no look-ahead, the same to the bit at any
block size, settings by value and held to range, ramps, one detector for two channels, readouts
copied off the thread. Nothing here is code, and no figure is final until the spec's tests pin it.

## What it's for

Audio drama and podcast voices (Warren, 2026-11-11: important to that work). An ess, a sh, a t
put too much energy between about 4 and 9 kHz, and close miking, compression and a top lift on the
EQ all make it worse. A de-esser turns down that band only while an ess is sounding, and leaves the
rest of the voice alone. In the rack it stands first after the EQ, before the compressors, so the
esses don't drive them.

**What a good one sounds like, and what a bad one does.** Done well, the esses sit level with the
voice and nothing else changes. Done badly it lisps (the ess is taken down so far it turns to
"th"), it dulls the whole voice for an instant on each ess, or it pumps on bright words that aren't
esses. Every choice below is made against those three.

## The manner

No unit is named as the style (Question 3, settled). The manner is that of the better hardware and plug-in
de-essers of the last thirty years, in two ideas:

- **A threshold that follows the voice.** The ess is judged against the voice it's in, not
  against a fixed level, so a whisper and a shout de-ess alike, and a voice that moves on the mic
  doesn't need the knob chased. `RFC-eq-dynamics.md` already had this (the relative threshold);
  it's kept, and it's the de-esser's main knob.
- **A dynamic bell, not a crossover.** In split mode the reduction is a bell cut at the ess's
  frequency that deepens while an ess sounds, the way an analog EQ's bell would if a hand pulled
  it down on every ess. It's minimum phase, like the EQ, and at rest it's exactly nothing.

### The split, written out

The earlier RFC left the split to the spec, between a crossover pair and a subtracted band. This
chooses the subtracted band, because written properly it **is** the dynamic bell:

- `bp` is a second-order band-pass at the ess's frequency, unity gain at its centre, fixed while
  the frequency is fixed (a state-variable filter in the zero-delay form is the recommended
  build, as it stays well behaved when its frequency is swept by a held key);
- the output is `y = x − (1 − g) · bp(x)`, where `g` is the gain the detector asks for, from
  1 (nothing) down to the range's floor.

That transfer is a peaking cut, (s² + s·g/Q + 1) / (s² + s/Q + 1): exactly `g` at the centre,
1 far from it, minimum phase. Only the one number `g` moves per sample; the filter's coefficients
don't, so nothing can go unstable and nothing zips. **At `g = 1` the output is the input to the
bit** (`1 − g` is exactly 0), which makes the null exact rather than within 1e-9.

**Wide mode** puts the same `g` on the whole signal, `y = g · x`, for a voice where the split
sounds unnatural. Split is the default.

## The controls

| Control | Range (held to) | Default | Fine step | Coarse step |
|---|---|---|---|---|
| Frequency | 3 to 10 kHz, log | 6 kHz | 1/12 octave | 1/3 octave |
| Threshold | −40 to 0 dB, relative | −20 dB | 1 dB | 3 dB |
| Range | 0 to 12 dB | 6 dB | 0.5 dB | 3 dB |
| Mode | split, wide | split | — | — |
| Listen | off, band, taken | off | — | — |
| In | in, out | out (dark in the rack) | — | — |

The width is fixed at about one octave (Question 1, settled). The ess sits low in a deep voice (4 to
5 kHz) and higher in a light one (7 to 8 kHz), so the frequency moves, but its width hardly
changes from voice to voice, and one knob fewer is one less thing to step through by keys.

**Frequency** narrowed from 2 to 12 kHz in the earlier RFC: below 3 kHz is the voice's presence,
not its esses, and above 10 kHz is air. **Range** stays 0 to 12 dB, as recommended earlier; past
about 8 dB a split de-esser starts to lisp on most voices, and 12 leaves room for a harsh one.

**Threshold, relative:** how far above the voice's own level the band has to rise before it's
turned down. Both are detected the same way (below), and the band's level against the whole's is
what's compared, in dB. At −20 the band has to be within 20 dB of the whole voice's level before
anything happens. Lower acts more.

**A floor, fixed:** below −55 dBFS on the whole (the spec sets the figure), nothing acts, so
breaths, room tone and the tail of a word never trigger it. Not a knob.

**Listen** is how the ear finds the ess (`Sonorack.md`: "holds LISTEN to find the ess"):

- **band:** the output is `bp(x)` alone, the band the de-esser listens to, so the frequency can be
  swept until the esses jump out;
- **taken:** the output is what's being removed, `x − y`, so a lisp or a dulled vowel is heard at
  once (Question 2, settled);
- switching is a crossfade, as everything is. Listen is never saved as anything but off.

On the keyboard a held key isn't reliable (it repeats), so on a panel Listen is a toggle, and it
goes off by itself when the cursor leaves the de-esser. On the FaderPort, later, a held button can
be the hold the story describes. Neither is the crate's business: it takes the setting.

All controls are described as data exactly as `RFC-eq.md` describes the EQ's (an id, labels, kind,
range, default, detent, fine and coarse step, format and forgiving parse, held to range never
refused), **from the same small module** in `as-dynamics` or shared with `as-eq`, as that RFC left
the spec to decide. The steps above are the de-esser's rows in it. The detents: the threshold's is
its default, the range's is 6 dB.

## The detector

Feed forward: it listens to the input, so the reduction is exactly what's computed and the tests
can say so.

- **Two levels:** the band's (`bp(x)`) and the whole's (`x`), each rectified and smoothed the same
  way, so the ratio between them means something. Stereo: the louder channel's value for each, so
  the image doesn't move.
- **Timing, fixed:** attack about 0.5 ms, so the front of an ess is caught (there's no look-ahead,
  by the family's rule); release about 40 ms, short enough to let go before the vowel that follows,
  long enough not to flutter within an ess. The whole's level has a slower release (about 100 ms),
  so a word's end doesn't make the next ess look relatively louder than it is. The spec fixes the
  figures and says why.
- **The gain computer** (`as-dynamics`' shared one): the band-over-whole level in dB against the
  threshold, a high ratio (about 10 to 1), and a **soft knee of about 6 dB**, which is most of
  what keeps it from sounding switched; the reduction held to Range. Then `g` is that reduction as a
  gain.
- **No added ramp on `g`** beyond the detector's own smoothing: it already moves no faster than the
  attack allows, and the test below holds it to that. Changes of a setting (threshold, range,
  mode) take effect through the detector, so they glide.
- **A held frequency key** sweeps `bp`'s frequency. The zero-delay state-variable filter takes a
  new frequency every sample without a thump; the spec moves its coefficient by a one-pole over
  about 10 ms toward the target, so a stream of settings glides.

## Readouts, for a panel and for the keys

As the other units: in and out peak, gain reduction now (dB), and whether it's acting. And one
more, because without it the threshold is set blind: **the band's level over the whole's, now, in
dB**, the number the threshold is compared with. A panel draws it as a bar with the threshold
across it; the status line says it in words, so a keyboard user hears where the esses sit:

`DE-ESS  6.0 kHz  THR −20  ESS −14  −4.5 dB  SPLIT`

`response(hz, g) -> dB` draws the bell at any depth without audio: at rest it's flat, and at Range
it's the deepest the bell can go, which a panel can show as a ghost behind the live curve.

## What "good" means, pinned by tests

Kept from `RFC-eq-dynamics.md`, on the same synthetic ess (band noise centred at 6 kHz, an octave
wide, at −20 dBFS, over a vowel-like tone of 220 Hz with harmonics to 2 kHz at −12 dBFS):

- split, Range 6: the band down by 6 dB within 0.3, the tone unmoved within 0.1 dB;
- wide, Range 6: the whole down by 6 within 0.3;
- the tone alone (no ess): unmoved within 0.01 dB;
- the relative threshold: the same pair 20 dB quieter is reduced the same within 0.3 dB;
- the reduction reaches 63% within the attack, and recovers to 37% within the release, within 10%.

New with this shape:

- **At rest is the input to the bit:** split with no ess, and In out, both bit-exact with the
  input, not within a tolerance.
- **The floor:** the ess pair 60 dB down (below the floor) is untouched, bit-exact.
- **No lisp at the centre:** the reduction at the ess's centre never exceeds Range by more than
  0.05 dB on any input, including a full-scale 6 kHz sine.
- **The bell is a bell:** with `g` held (a test hook, or Range reached on a steady ess), the
  response measured by sine matches `response(hz, g)` within 0.1 dB from 200 Hz to 20 kHz.
- **Listen:** band gives `bp(x)` within 1e-12; taken gives `x − y` within 1e-12; neither saved.
- **Held keys:** the frequency changed every 2 ms for a second across its range, and the
  threshold the same: no sample-to-sample step beyond the bound, no broadband burst above
  −80 dBFS, the last setting in force at the end.
- **The readout:** the band-over-whole level on the steady ess pair reads the level set within
  0.5 dB.
- **The controls:** as `RFC-eq.md`'s: steps return exactly, format then parse round-trips, out of
  range is held.
- Block sizes 1, 64 and 1,024 the same to the bit; a run twice the same; no non-finite sample on
  silence, DC, full-scale noise or a −300 dBFS tail.

## Order of work

Unchanged: in `as-dynamics`, the shared gain computer and test signals first, then `deess`, before
`opto` and `fet`. sonorack takes it at its step 5 (`../Sonorack.md`). Warren's gate: a sibilant
voice, by ear, listening for the three faults above (a lisp, a dulled voice, pumping), with
**taken** the quick way to hear them.

## For the spec session to decide (not Warren)

- The detector's figures (attack, release, the whole's slower release), the knee, the ratio.
- The floor's level.
- The band-pass's exact width (about an octave, Q near 1.4) and the form of the state-variable
  filter.
- The controls' ids and names, and where the control-description module lives.

## Questions for Warren

1. **Width fixed at about an octave**, not a knob? Recommended: fixed. The frequency is what
   changes from voice to voice; a width knob is one more control to step through for little gain.
   If a voice ever wants it, it's added then.
2. **Listen with two settings,** the band and what's taken away? Recommended: yes. The band finds
   the ess; what's taken is the quickest way to hear a lisp.
3. **No named unit as the style**, unlike the EQ and the compressors? Recommended: yes. There's no
   one de-esser everyone's ear knows the way they know an SSL or an 1176; the manner above (a
   threshold that follows the voice, a dynamic bell) is what the good ones share.

**Settled (Warren, 2026-11-12):** all three as recommended. The width is fixed at about an octave;
Listen has two settings, the band and what's taken; no unit is named as the style.
