# Spec: `as-dynamics`' `fet`, the FET compressor (RACK voicing)

From the suite's session, 2026-11-12. Read `../../../specs/shared.md` and `spec.md` (beside this)
first. The shape is `../../../RFC-fet.md` (settled 2026-11-12); this chooses the RACK voicing's
figures. The KT76 voicing's come from measuring Warren's unit, later. Built last in `as-dynamics`,
its stage last of all. Moved here from `specs/fet.md` when it was built (step 7, 2026-11-12); "As
built", at the end, has what was measured and where it differs.

As the optical's, these figures weren't prototyped in the session. They're starting points that
meet the RFC's shape; the tests pin the shape, and a figure that has to move to pass one moves here,
dated, with why.

## What was decided here, and why

1. **The 1176's figures,** as commonly published (the session had no manual): attack 20 µs to
   800 µs, release 50 ms to 1.1 s, ratios 4, 8, 12, 20, feedback from the output. The builder checks
   a published manual if one is at hand and notes the source in the crate's README.
2. **The dials' law is log** between those ends, since the unit's controls are pots on RC times:
   - attack `τ_a = 800 µs · (20/800)^((d − 1)/6)`: dial 1 is 800 µs, 4 is 126 µs, 7 is 20 µs;
   - release `τ_r = 1.1 s · (0.05/1.1)^((d − 1)/6)`: dial 1 is 1.1 s, 4 is 235 ms, 7 is 50 ms.
   The defaults read `ATK 3 (234 µs)` and `REL 5 (140 ms)`. (The RFC's status line example, 330 µs
   and 180 ms, was illustration; these are the law's.)
3. **The detector is peak, in dB, of the cell's output before the stage,** with the reduction's own
   attack and release as the only smoothing, as the unit's rectifier into an RC. Before the stage,
   so the colour never changes how hard it compresses, and `curve()` is exact.
4. **The attack is loop-corrected and the release isn't** (`../crates/dynamics/docs/spec.md`): the dial's attack time is
   what a step measures, at any ratio, and the loop can't oscillate at 20 µs; the release lets go
   faster while the signal stays hot, which is the unit's programme-dependence, kept.
5. **The threshold at Input 0 is −18 dBFS** (peak, on the output side), as the RFC asked. Input is a
   gain before the cell; Output a gain after the stage.
6. **The stage** is an offset tanh, its drive rising with the reduction (below). Run at the rate,
   unless the aliasing test fails.

## The path, per sample

1. `u = x · input_gain` (ramped);
2. `v = u · g`, by `../crates/dynamics/docs/spec.md`'s loop on `v[n−1]`'s peak level;
3. `w = stage(v, ρ)`;
4. `w` through a first-order high-pass at 2 Hz (the stage's even order puts a little DC in; 2 Hz
   costs 0.04 dB at 20 Hz);
5. `y = w · output_gain` (ramped).

## The ratios and knees

| Button | Ratio | Knee | Threshold | Times |
|---|---|---|---|---|
| 4 | 4:1 | 6 dB | T | × 1 |
| 8 | 8:1 | 5 dB | T | × 1 |
| 12 | 12:1 | 4 dB | T | × 1 |
| 20 | 20:1 | 3 dB | T | × 1 |
| ALL | 20:1 | 10 dB | T + 3 dB | attack and release × 0.5 |

`T` is −18 dBFS. ALL is the all-buttons state: it grabs and lets go in a lurch. Its figures are the
first for Warren's ear to move.

## The stage

`stage(v, ρ) = (tanh(k·(v + b)) − tanh(k·b)) / (k · (1 − tanh²(k·b)))`

- `k = 0.6 · (1 + 0.05 · ρ)`: 0.6 with no reduction, 0.9 at 10 dB, so **more colour the harder it
  works**;
- `b = 0.05`, the offset that gives the even harmonics;
- the divisor makes the gain at 0 exactly 1, and `stage(0) = 0`, so a quiet signal passes at unity
  and nothing is louder for the stage being in.

By the series of tanh, the third harmonic is about `(k·A)²/12` of a sine of peak `A`: 0.05% at
−18 dBFS, 3% at 0 dBFS with no reduction. The builder measures and, if the even order is off, moves
`b` so the second harmonic sits about 10 dB under the third at 0 dBFS.

## The controls

| Id | Short | Kind | Range | Default, detent | Fine | Coarse |
|---|---|---|---|---|---|---|
| `in` | IN | switch: out, in | | out (dark in the rack) | | |
| `input` | IN | linear, dB | 0 to +40 | 0 | 0.5 | 3 |
| `output` | OUT | linear, dB | −20 to +20 | 0 | 0.5 | 3 |
| `attack` | ATK | linear, dial with its law | 1 to 7 | 3 | 0.1 | 1 |
| `release` | REL | linear, dial with its law | 1 to 7 | 5 | 0.1 | 1 |
| `ratio` | RATIO | switch: 4, 8, 12, 20, ALL | | 4 | | |

(The `in` switch's short label and Input's are both "IN" on the unit; the panel tells them apart by
place. The builder may make the switch's `ON`.) Typing `330us` on ATK lands on the nearest 0.1 of
the dial and shows it.

## Readouts

`../crates/dynamics/docs/spec.md`'s. `curve(level_db) -> out_db` at the Input and ratio set, Output included, the
stage not (it's a colour, not a level).

Status line: `FET  IN +18.0  OUT −6.0  ATK 3 (234 µs)  REL 5 (140 ms)  4:1  GR −6.0 dB`

## Pinned by tests

- **The static curve:** the stepped sine (`../crates/dynamics/docs/spec.md`), steps held 2 s, ATK 4, REL 1, Input +20:
  the fundamental's level against the input matches `curve()` within 0.1 dB at each ratio and ALL.
- **The attack:** on the DC step from silence to a level whose settled output stays above the knee,
  the core's exact test: the remaining error shrinks by `exp(−1/(τ_a·rate))` each sample within
  1e-9, at dials 1, 4 and 7, at each ratio. On the RFC's 1 kHz burst (−30 to −6 dBFS, Input +20):
  the 63% time within 25% of the dial's, at dial 1 and 4 (at dial 7 the time is shorter than the
  sine's period and a burst can't measure it; the DC test does).
- **The release:** on a DC step falling from over the threshold to 40 dB under it: the reduction
  falls by `exp(−1/(τ_r·rate))` each sample within 1e-9 once under the knee, at dials 1, 4 and 7.
- **ALL is its own:** against 20:1, the knee, the threshold and both times read ALL's figures within
  1% on `curve()` and the DC tests.
- **Switching in is never louder:** at the defaults, the corpus (`speech`, `impacts`) at levels from
  −40 to 0 dBFS: the output's peak and its RMS over every 50 ms are never more than 0.1 dB above the
  input's.
- **The stage:** THD (`measure::thd`) below 0.3% at −18 dBFS with no reduction, rising
  monotonically to between 1 and 6% at 0 dBFS (a 1 kHz sine, the stage alone, by a test hook); at
  the same output level, more THD with 10 dB of reduction than with none; aliasing
  (`measure::worst_product`) below −80 dBFS for a 1 kHz sine at −18 dBFS through the whole unit,
  and recorded (not pinned) for 5 kHz at 0 dBFS. If the −80 fails, the stage runs at twice the rate
  with fixed minimum-phase filters adding under 0.5 ms, and this says so.
- **The unit out is the input to the bit.** (In, below the threshold, it isn't: the stage is always
  in while the unit is. That's the RFC's.)
- **The dials:** typing a time lands on the nearest fine step and shows it; dials 1, 4 and 7 read
  800 µs, 126 µs, 20 µs and 1.1 s, 235 ms, 50 ms within 1%.
- **The reduction never passes 40 dB.**
- **A held key** (`shared.md`'s rig) on `input`, `output`, `attack`, `release`, `ratio` stepped
  through every position including ALL.
- Linked, the controls, block sizes, twice, non-finite, denormal timing.

## Warren's gates (sonorack's step 7)

A hit, then a voice, by ear. Listen for the three faults: a crackle on a voice at fast settings that
weren't asked for, a hit flattened dead, an edge where nothing is compressing. The figures most
likely to move by ear: ALL's, the stage's `k` and `b`, and the knees.

Then, later: Warren's KT76 measured at the rig, and the KT76 voicing, by `RFC-fet.md`. A spec of its
own when that comes.

## As built (step 7, 2026-11-12)

`as_dynamics::fet`: `Fet` (`new`, `set`, `process`, `process2`, `settings`, `settled`,
`readouts`), `Settings` (`on`, `voicing`, `rack`; `get`, `set_by_id`, `held`, `button`,
`threshold`, `times`), `Rack`, `Voicing` (RACK alone), `Figures` and `RACK`, `Button`, `Readouts`,
`controls`, `curve`, `stage`, and the dials' laws (`attack_time`, `attack_dial`, `release_time`,
`release_dial`). The tests are `src/fet/tests.rs`. No manual was at hand: the 1176's figures are
as commonly published, not checked (decision 1). The `in` switch's short label is `ON`.

**Measured, and kept at the spec's figures:**

- **The stage, `k` and `b` unmoved:** THD 0.12% at −18 dBFS with no reduction, rising every dB to
  2.86% at 0 dBFS; more with 10 dB of reduction than with none at every level tried; the second
  harmonic 10.9 dB under the third at 0 dBFS. **Aliasing** through the whole unit, a 1 kHz sine at
  −18 dBFS at 44.1 kHz (where folded harmonics don't land on harmonics): the worst product −106.8
  dBFS. **The stage runs at the rate;** no double rate. Recorded: 5 kHz at 0 dBFS, −86.2 dBFS at
  9.1 kHz.
- **The attack:** the core's exact test passes (within 1e-9) at dials 1, 4 and 7, at every button,
  at 44.1, 48 and 96 kHz, with one exception: **ALL at ATK 7 (10 µs) at 44.1 and 48 kHz.** ALL
  settles above its knee only past 40 dB of reduction, so the test drives it 55 dB over and checks
  on the way to 40; at 10 µs the first sample's step, `(1 − p)` of 52 dB, already passes 40. The
  reduction is checked held at 40 there, and the pole is read at 96 kHz.
- **The release:** within 1e-9 of the dial's pole once under the knee, dials 1, 4 and 7, every
  button, every rate.
- **ALL is its own:** its attack and release poles are half of 20:1's within 0.5%; read off
  `curve()`, its threshold is −15 dBFS and its knee 10 dB, within 1% (the knee's foot and one point
  in it, since fed back, ALL's straight part starts 85 dB over its threshold, past the 40 dB hold).
- **The dials:** 1, 4 and 7 read 800, 126 and 20 µs and 1.1 s, 235 and 50 ms within 1%; the
  defaults show `3 (234 µs)` and `5 (140 ms)`; `330us` typed lands on 2.4 and shows `2.4 (338 µs)`.
- **Switched in, never louder:** on `speech` and `impacts` from −40 to 0 dBFS, every 50 ms window's
  peak and RMS at most **+0.0023 dB** over the input's.

**Where it differs:**

- **The high-pass filters only what the stage adds** (differs from "The path", item 4):
  `w = v + highpass(stage(v, ρ) − v)`, not `highpass(stage(v, ρ))`. On the whole signal, the 2 Hz
  filter's settling after a low onset (a decaying 60 Hz impact) lifted a quiet tail's peak by
  0.27 dB at −40 dBFS, failing "never louder"; a lower corner only fits the figure to the signal,
  since a lower hit would bring it back. The filter is there for the stage's DC, and now takes
  only that: unfiltered, a 0 dBFS sine with the stage working hard puts DC 25 dB under its peak;
  filtered, nothing (a pin). The input's own DC passes, as through the optical. Above 2 Hz the
  output is as before.
- **The burst is pinned as found, not within 25% of the dial.** On the RFC's burst (−30 to −6
  dBFS at Input +20, REL 1, 4:1, the burst starting at a zero crossing going up), the 63% time is
  **1.59 times** the dial at ATK 1 and **1.81** at ATK 4. The peak detector pulls only near the
  crests (the duty cycle, `spec.md`'s "As built"); at ATK 4, 126 µs, it's mostly the sine's quarter
  period to its first crest, the reason the spec already gives for leaving out ATK 7. Decision 3
  keeps the detector a bare peak, so the figure moves: pinned at 1.4 to 1.8 at ATK 1, recorded at
  ATK 4. The DC test is the exact pin.
- **The static curve is pinned in two parts, nothing loosened.** The fundamental measures out the
  stage's harmonics but not its compression of the fundamental. **The cell** (the stage skipped, a
  test hook) reads `curve()` within 0.1 dB at every button: within 0.034 to 0.068 dB. **The whole
  unit** reads `curve()` plus the stage's own gain on a sine's fundamental at that level and
  reduction, within 0.1. **The stage pulls the fundamental down by up to 0.60 dB** (4:1, Input +20,
  0 dBFS in: 28 dB of reduction, `k` about 1.45, a peak near −8.5 dBFS); 0.15 to 0.25 dB at the
  other buttons. For Warren's ear: the level drawn and the level heard differ by that at the top.
- **The held key's line above 4 kHz** (Warren, 2026-11-12): −80 dBFS, or 3 dB over the louder end
  held still where that's over −80. With nothing moving, the stage reads −60.1 dBFS at Input +40
  and −72.8 at Output +20: the line is for clicks, not colour. Those two controls' sweeps are
  judged against −57 and −70; every other control of every unit keeps −80. On those two the line
  can't see a click quieter than that above 4 kHz; the step check (no step over 1.1 times the held
  ends') still covers them, and the same `Ramp` passes at −80 on the optical's Gain.
- **The rigs are written once** (`src/rigs.rs`): the stepped sine streamed a step at a time
  (`each_step`, `read_tail`), a control's grid for a held key (`grid`), the denormal timing
  (`tail`) and `same`, gathered there from the units' tests in this step.
