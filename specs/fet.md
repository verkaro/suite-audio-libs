# Spec: `as-dynamics`' `fet`, the FET compressor (RACK voicing)

From the suite's session, 2026-11-12. Read `shared.md` and `../crates/dynamics/docs/spec.md` first. The shape is
`../RFC-fet.md` (settled 2026-11-12); this chooses the RACK voicing's figures. The KT76 voicing's
come from measuring Warren's unit, later. Built last in `as-dynamics`, its stage last of all.

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
