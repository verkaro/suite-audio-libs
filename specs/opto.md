# Spec: `as-dynamics`' `opto`, the optical compressor (RACK voicing)

From the suite's session, 2026-11-12. Read `shared.md` and `dynamics.md` first. The shape is
`../RFC-opto.md` (settled 2026-11-12); this chooses the RACK voicing's figures, which it left to the
spec. The LA-4 voicing's figures come from measuring Warren's unit, later; nothing here is fitted
to it. Built after `deess`, before `fet`.

These figures weren't prototyped in the session, unlike the EQ's and the de-esser's. They are
starting points chosen to meet the RFC's shape, and the tests pin the shape. Where a figure has to
move to pass a test, it moves here, dated, with the reason (`shared.md`, "For the session that
builds"). Its character is Warren's ear's.

## The cell, written out

Feedback, by `dynamics.md`'s loop. Per sample:

1. **Level:** mean square of the cell's output `y[n−1]` (both channels, the larger square), a
   symmetric one-pole of **1 ms**, sine-referenced dB. Fast enough not to blur the attack, slow
   enough to smooth a 1 kHz tone's ripple to under 0.2 dB. The light, not the cell: the cell's
   slowness is the reduction's.
2. **Target:** the feedback gain computer at the threshold from Reduce, with the ratio's knee.
3. **Two envelopes** carry the reduction:
   - `ρ_fast` moves toward the target `r_t`;
   - `ρ_slow` moves toward **half** the target, `r_t / 2`;
   - the reduction is `ρ = max(ρ_fast, ρ_slow)`. At steady state that's `r_t` (the static curve is
     the gain computer's); on a release, the fast part lets half go quickly and the slow part holds
     the rest.
4. **Attack** (either envelope, when its target is above it): the loop-corrected attack of
   `dynamics.md`, with `τ_a` depending on how brightly the light is driven, `o = L − T`, the
   output's level over the threshold in dB: `τ_a = 10 ms · sqrt(6 / max(o, 0.1))`, held between
   3 and 30 ms. 10 ms at 6 dB over, 7.1 ms at 12, 14 ms at 3: **faster the harder it's pushed.**
   As the cell closes, the output falls toward the threshold and the attack slows, which is the
   soft grab. (Not `r_t − ρ`: in a feedback loop the target overshoots at first by `R − 1` times
   the level, which would make the attack depend on the ratio.)
5. **Release** (when the target is below): `ρ_fast` with **60 ms**; `ρ_slow` with `τ_s`, read from
   the memory. Half the reduction is back in `60 ms · ln 2 ≈ 42 ms`, inside the RFC's 80. **Unlike
   the FET, the optical's release is loop-corrected too** (`c_r / (1 + s)`, as the attack): left
   alone, a feedback loop lets go up to `R` times faster while the voice is still over the threshold,
   which on dialogue is the pumping the cell is there to prevent. The cell's times are its own at
   any level.
6. **Memory:** `m`, a symmetric one-pole of **2 s** on `ρ` (dB). `held = min(m / M, 1)`, with
   `M = 10 dB` (6 in the 2A position). `τ_s = 0.5 s · 10^held`: 0.5 s with no memory, 5 s at its most.

When the level stays under the knee, `r_t` is exactly 0, the envelopes and the memory decay to
exactly 0 (by the `1e-12` rule of `dynamics.md`), and the gain is exactly 1.

## The controls

| Id | Short | Kind | Range | Default | Detent | Fine | Coarse |
|---|---|---|---|---|---|---|---|
| `in` | IN | switch: out, in | | out (dark in the rack) | | | |
| `reduce` | REDUCE | linear, plain | 0 to 100 | 40 | 0 | 1 | 5 |
| `gain` | GAIN | linear, dB | −10 to +30 | 0 | 0 | 0.5 | 3 |
| `ratio` | RATIO | switch: 2A, 2, 4, 8, 12, 20 | | 4 | 4 | | |

**Reduce's law:** threshold `T = 10 − 50 · (reduce/100)^0.6` dBFS. 0 is +10 dBFS, so with the
widest knee nothing a file can hold is touched; 100 is −40 dBFS; 40 is −18.9 dBFS. The power under
1 gives the start of the knob more travel, as a log-taper pot does. The format shows both:
`40 (−18.9 dBFS)`. (The RFC's status line example read −22; the figure is this law's.)

**The positions:**

| Position | Ratio | Knee | Attack | Memory scale `M` |
|---|---|---|---|---|
| 2A | 3:1 | 20 dB | × 1.5 | 6 dB |
| 2 | 2:1 | 18 dB | × 1 | 10 dB |
| 4 | 4:1 | 15 dB | × 1 | 10 dB |
| 8 | 8:1 | 13 dB | × 1 | 10 dB |
| 12 | 12:1 | 12 dB | × 1 | 10 dB |
| 20 | 20:1 | 11 dB | × 1 | 10 dB |

2A is the LA-2A's manner: soft, the widest knee, the slowest attack, the most memory. Only it
changes the cell's times. A change of position changes the target, and the cell glides to it.

**The LA-4's own face,** for the later voicing: remembered as Input and Output knobs into a fixed
threshold, with the 2, 4, 8, 12 and 20 switch, but that's for Warren's typed list of his own front
panel to settle (`RFC-opto.md`, "Asked of Warren"). RACK keeps Reduce, as settled.

## Readouts

`dynamics.md`'s, and `held`, 0 to 1, as above. With `curve(level_db) -> out_db` for the ratio and
Reduce set.

## Pinned by tests

- **The static curve:** the stepped sine (`dynamics.md`), each step held 5 s, at Reduce 60 and Gain
  0: the output against the input matches `curve()` within 0.3 dB at every position.
- **The attack,** on the DC step from silence to 6 dB over the threshold, at 4:1: the 63% time of
  the reduction is between 9 and 15 ms. (Not "10 ms within 10%" as the RFC has it: the attack slows
  as the output falls toward the threshold, by design, so the measured time is a little longer than
  the law's figure at the start. The builder records the measured figure here.) On the RFC's 1 kHz
  burst: within 25% of the DC figure.
- **Faster when pushed harder:** the 63% time on a step 12 dB over is shorter than on one 3 dB
  over, by at least a quarter, at 4:1 and at 20:1.
- **Two-part release:** after 1 s of DC 12 dB over the threshold, falling to silence: half the
  reduction is back within 80 ms, and the last 10% takes at least 0.5 s more.
- **The memory:** a −6 dBFS 1 kHz tone at Reduce 60, held 0.1 s, then silence: the time from the
  fall back to 3 dB of reduction; the same after 2 s held: the second at least 1.5 times the first.
- **No pumping:** `corpus::speech` and `corpus::phrases` at −12 dBFS, Reduce 60, every position:
  at every sample where the reduction falls, it falls by no more than `1.05 · ρ · (1 − exp(−1/(0.060
  · rate)))`, that is, never faster than the fast release from where it is. This is the RFC's
  "bound the spec sets", and it's the one the loop's correction protects.
- **Below the threshold is the input to the bit:** the corpus 30 dB under the threshold, Gain 0:
  bit-exact. The unit out: bit-exact.
- **2A:** its knee is the widest and its memory the most; the memory test's ratio is larger in 2A
  than in 4.
- **The readouts:** `held` reads exactly 0 after a minute of silence, and rises with time held.
- **The reduction never passes 40 dB.**
- **A held key** (`shared.md`'s rig) on `reduce` and `gain`, `ratio` stepped through every position.
- Linked, the controls, block sizes, twice, non-finite, denormal timing.

## Warren's gate (sonorack's step 6)

Dialogue, by ear, with `held` on the panel. Listen for the three faults: pumping (the room breathing
between words), squashed fronts (consonants losing their bite), a quiet line buried after a loud
one. The figures most likely to move by ear: the fast release (60 ms), the memory's reach (5 s at
most), the knee per position, and Reduce's law.

Then, separately and later: `sonorack measure`, Warren's LA-4 at the rig, and the LA-4 voicing, by
`RFC-opto.md`. A spec of its own when that comes.
