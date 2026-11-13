# Spec: `as-dynamics`' `deess`, the de-esser

From the suite's session, 2026-11-12. Read `../../../specs/shared.md` and `spec.md` (beside this)
first. The shape is `../../../RFC-deess.md` (settled 2026-11-12); this chooses what it left to the
spec. Built first of the three, after the core. Moved here from `specs/deess.md` when it was built
(step 5, 2026-11-12); "As built", at the end, says what the builder chose and where it differs.

## What was decided here, and why

The detector was tried in the session on the RFC's synthetic ess (octave-band noise at 6 kHz,
−20 dBFS, over a 220 Hz tone with harmonics to 2 kHz, −12 dBFS, at 48 kHz, two seconds):

| Detector (attack / release, band; whole) | Reads | True | Spread |
|---|---|---|---|
| mean square, 0.5 / 40 ms; 0.5 / 100 ms (the RFC's) | −9.1 dB | −9.7 dB | 0.46 dB |
| mean square, 1 / 1 ms; 5 / 100 ms | −14.2 | −9.7 | 1.5 |
| mean square, 2 / 2 ms; 10 / 10 ms | −9.8 | −9.7 | 1.1 |

The RFC's own figures read nearest the truth with the least flutter: the fast attack on both rides
the peaks alike, so the bias mostly cancels in the ratio. **Kept.** The readout test allows the
0.6 dB the prototype found (below).

1. **The band-pass:** a zero-delay state-variable filter (the trapezoidal form, Zavalishin's and
   Simper's), its band output scaled by `k = 1/Q` so it's unity at the centre exactly, `Q = 1.414`
   (one octave between its −3 dB points, `as_biquad::q_from_octaves(1.0)`).
2. **The detector:** mean square, branching one-poles on the squared signal (attack when the square
   is above the state, release otherwise). Band: 0.5 ms attack, 40 ms release. Whole: 0.5 ms
   attack, 100 ms release.
3. **The gain computer:** feed forward, ratio 10, knee 6 dB, on `ess = band_dB − whole_dB` against
   the threshold. The reduction held to Range.
4. **The floor:** −55 dBFS on the whole's level, **soft over 6 dB**: the reduction is multiplied by
   `clamp((whole_dB − (−61)) / 6, 0, 1)`. The RFC had a hard floor; a hard one would step the gain
   when a quiet voice crosses it. Below −61 the reduction is exactly 0.
5. **Settings glide** by the ramps of `shared.md`: Threshold and Range linear in dB over 5 ms (so the
   reduction they ask for can't jump); Mode, Listen and In by crossfade. Frequency by a one-pole on
   `log2(f)`, 10 ms, the filter's coefficient recomputed each sample while it moves, and set exactly
   to the target once within `1e-9` octave of it.

## The signal path

Per sample, per channel, with `g` from the shared detector:

- `b = bp(x)`, the band;
- **split:** `y = x − (1 − g) · b`. A peaking cut, `(s² + s·g/Q + 1) / (s² + s/Q + 1)`: exactly `g` at
  the centre, 1 far off, minimum phase. At `g = 1.0`, `1 − g` is exactly 0 and `y` is `x` to the bit.
- **wide:** `y = g · x`.
- **Listen band:** the output is `b`. **Listen taken:** the output is `x − y`.

`bp` always runs, in every mode, so a switch never starts it from rest.

## The controls

| Id | Short | Kind | Range | Default, detent | Fine | Coarse |
|---|---|---|---|---|---|---|
| `in` | IN | switch: out, in | | out (dark in the rack) | | |
| `freq` | FREQ | log, Hz | 3 to 10 kHz | 6 kHz | 1/12 oct | 1/3 oct |
| `thr` | THR | linear, dB | −40 to 0 | −20 | 1 | 3 |
| `range` | RANGE | linear, dB | 0 to 12 | 6 | 0.5 | 3 |
| `mode` | MODE | switch: split, wide | | split | | |
| `listen` | LISTEN | switch: off, band, taken | | off | | |

Listen is a setting like the rest; the crate doesn't know it's never saved on. That's sonorack's
(its snapshots write it off).

## Readouts

`../crates/dynamics/docs/spec.md`'s, and:

- `acting`: true if the reduction at any sample of the block was above 0.1 dB;
- `ess_db`: the band over the whole now, `band_dB − whole_dB`, the number the threshold is compared
  with, so a panel draws it as a bar with the threshold across it.

And without audio: `response(&Settings, rate, hz, g) -> dB`, the split's magnitude at a depth `g`,
from the SVF's own digital transfer (it is a biquad in disguise, and its coefficients can be
written out), so the bell drawn is the bell heard. At `g = 1` it's 0 dB everywhere.

## Pinned by tests

On the synthetic ess (made in the test from `as-envelope`'s `noise` and `sine`, or `corpus::sibilants`
over `corpus::voice` if those sit where the figures say), 48 kHz unless said:

- **Split, Range 6, Threshold −40** (well past): the reduction sits at 6.0 dB within 0.05 once
  settled (`gr_db`), so the bell's centre is down 6; the band's power (of `bp` of the output against
  `bp` of the input, over the last second) falls by what `response(…, g)` predicts for that band's
  spectrum, within 0.3 dB. (A bell 6 dB deep takes less than 6 from an octave of noise, since its
  edges are shallower; the RFC's "the band down by 6" was written for a crossover split.) The tone's
  harmonics each move by what `response` says at their frequency, within 0.05 dB, and those at or
  below 1 kHz by less than 0.1 dB. (The bell's skirt takes about 0.2 dB at 2 kHz, so the RFC's "the
  tone unmoved within 0.1 dB" holds only up to there. **Differs from the RFC.**)
- **Wide, the same:** the whole down by 6 within 0.3.
- **The tone alone:** unmoved within 0.01 dB (it puts almost nothing in the band, so `ess` sits far
  under the threshold).
- **The relative threshold:** the same pair 20 dB quieter is reduced the same, within 0.3 dB.
- **At rest is the input to the bit:** split with the tone alone at a threshold no ess reaches; In
  out.
- **The floor:** the pair 60 dB down: the output is the input to the bit.
- **No lisp at the centre:** a full-scale 6 kHz sine, Range 6: the reduction at 6 kHz never passes
  6.05 dB.
- **The bell is a bell:** with `g` held (a test-only setter), a sine sweep's magnitude matches
  `response(…, g)` within 0.1 dB from 200 Hz to 20 kHz, at 44.1, 48 and 96 kHz, at g for 3, 6 and
  12 dB.
- **Listen:** band gives `bp(x)` within 1e-12, taken gives `x − y` within 1e-12, once the crossfade
  has ended.
- **The readout:** `ess_db` on the steady pair reads the true power ratio of `bp(x)` to `x` within
  1.0 dB (the prototype: 0.6 dB high, 0.46 dB spread). **Differs from the RFC** (0.5 dB), by the
  prototype's figure.
- **The timing,** on a DC-free version of the step (the ess pair switched on and off): the band's
  detector state reaches 63% of a step in its power in 0.5 ms within 5%, and the reduction is fully
  set within 5 attack times and fully released within 5 release times.
- **A held key** (`shared.md`'s rig, with the ess pair as the signal and the burst check applied
  above 12 kHz instead of 4): on `freq`, `thr` and `range`, and `mode` and `listen` toggled.
- Linked, the controls, block sizes, twice, non-finite, denormal timing.

## Warren's gate (sonorack's step 5)

A sibilant voice, by ear. Listen for the three faults: a lisp (an ess turned to "th"), the whole
voice dulled for an instant on each ess, pumping on bright words that aren't esses. **taken** is
the quick way to hear the first two. If 12 dB of Range isn't enough on his voices, it's raised.

## As built (step 5, 2026-11-12)

`as_dynamics::deess`: `DeEsser` (`new`, `set`, `process`, `process2`, `settings`, `settled`,
`readouts`), `Settings` (`get`, `set_by_id`, `held`), `Mode`, `Listen`, `Readouts`, `controls`,
`response`, `band_q`, and the figures as constants. The tests are `src/deess/tests.rs`; the signal
is made there: the ess as noise in exactly the octave about 6 kHz (flat, random phases, by an
inverse FFT, so nothing of it lands on the voice's harmonics), the voice as 220 Hz and its
harmonics to 2 kHz falling as 1/h. That pair reproduces the prototype's readings exactly (`ess`
reads −9.10 dB, truly −9.71), so it's taken to be the prototype's.

What was measured, and where it differs:

- **The frequency glides linearly on `log2(f)` over 10 ms, not by a one-pole.** Differs from
  decision 5. A one-pole of 10 ms set exactly within 1e-9 octave takes about 18 time constants
  from a fine step (1/12 octave), 180 ms, and the held key asks every glide to have ended 50 ms
  after the last setting. A linear ramp, retargeted from where it is, ends in 10 ms, as the other
  settings' ramps do (`as_control::Ramp`), and lands on the frequency sent exactly. The held key on
  FREQ passes (no step, nothing above 12 kHz over −80 dBFS) at all three rates.
- **The voice alone is over the default threshold.** The prototype's voice alone reads `ess` at
  −19.0 to −18.5 dB (the detectors ride its crests; its true band-over-whole is −23.7), over the
  default −20, and takes up to 1.5 dB. So "the tone alone unmoved within 0.01 dB" is tested at a
  threshold of −16, where it holds. **For Warren's gate:** a voice as bright as that one is
  touched at the default threshold. If that's heard as pumping on bright words, the default
  moves toward −16. A darker voice (harmonics as 1/h²) is under it, but then the pair's `ess`
  reads 1.5 dB high, not 0.6: the readout's bias follows the voice's crest factor.
- **The detector's time is pinned on a step in its power, not on the pair.** Fed a step in its
  square, the band's detector is at 63% in 0.5 ms within 5% (exactly 24 samples at 48 kHz). On the
  pair, it rides the band's crests and settles about 7 dB over the band's true power, so 63% of its
  own settled state comes at 7.6 ms, which isn't its time. What's pinned on the pair is that it
  reaches 63% of the band's true power within one attack time: 0.21 ms. The reduction is fully set
  (at Range) within five attack times, and fully released, at a threshold the ess is 4 dB over,
  in 113 ms, within five of the band's release times (200 ms).
- **In, Mode and Listen are one crossfade,** between the outputs of what was and what is (In,
  Mode and Listen together), so a change of two at once is one 10 ms fade.
- **The held key bounds a switch's steps by every position held,** not its ends: Listen's band is
  louder than off or taken (the rig, `held_key_on`).
- **No follower on the reduction:** it's the gain computer's on the detectors' levels, held to
  Range and scaled by the floor, sample by sample; the detectors' times are its times.
- **`response`** is the band-pass's bilinear transform written out (the trapezoidal SVF is exactly
  that, prewarped at its centre). The bell measured on sines matches it within 0.1 dB from 200 Hz
  to 20 kHz at 44.1, 48 and 96 kHz, at 3, 6 and 12 dB; at the centre it's the depth within 1e-9.
- On the pair at Threshold −40, Range 6, split: the reduction 6.0 dB, the band's power down 3.75 dB
  (the bell predicts 3.75), the harmonics moved by the bell within 0.05 dB.
