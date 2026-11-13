# Spec: `as-dynamics`, the core its three units share

From the suite's session, 2026-11-12. Read `../../../specs/shared.md` first. The units are
`deess.md`, `opto.md` and `fet.md` in `../../../specs/`; this is what they're built on, built first,
with its own tests. Moved here from `specs/dynamics.md` when the crate was made (step 4,
2026-11-12). Each unit's spec moves in beside it at its own step, as `docs/spec-deess.md` and so on.
"As built", at the end, says what the builder chose and where it differs.

## The crate

One crate, modules `deess`, `opto` and `fet` (Warren, 2026-11-11), over a shared core: the gain
computer, the feedback loop's update, the ramps, the readouts, and the test rigs. A tool takes one
unit or all three.

## The gain computer

In dB, with a quadratic soft knee (the usual form, Giannoulis, Massberg and Reiss, 2012). Two
forms, one for each topology:

**Feed forward** (the de-esser): from a level `x` (dB), a threshold `T`, a ratio `R` and a knee
width `W`, the reduction `r(x) ≥ 0`:

- `x − T ≤ −W/2`: `r = 0`, **exactly**;
- `|x − T| < W/2`: `r = (1 − 1/R) · (x − T + W/2)² / (2W)`;
- `x − T ≥ W/2`: `r = (1 − 1/R) · (x − T)`.

**Feedback** (the two compressors): the detector hears the **output**, so the reduction is a
function of the output's level `y`. For a ratio `R` from input to output, the slope on the output
side is `R − 1`, not `1 − 1/R`:

- `y − T ≤ −W/2`: `r = 0`, exactly;
- `|y − T| < W/2`: `r = (R − 1) · (y − T + W/2)² / (2W)`;
- `y − T ≥ W/2`: `r = (R − 1) · (y − T)`.

At steady state, input level `x = y + r(y)`, which rises strictly with `y`. So:

- **`curve(level_db) -> out_db`**, the static transfer a panel draws, solves `x = y + r(y)` for `y`
  by bisection (60 halvings is exact to rounding; off the audio thread, no state). Above the knee
  it is `T + (x − T)/R`, as a ratio should be.
- **The local slope** `s(y) = dr/dy` (0 below the knee, rising through it, `R − 1` above) is what
  the loop's attack uses (below).

Both forms: `r` is 0 to the bit below the knee, so a signal under the threshold leaves the gain at
exactly 1 (see "The gain", below). Reduction is held to at most 40 dB.

## The feedback loop

The compressors are feedback (`RFC-eq-dynamics.md`, the opto and FET RFCs): `y[n] = x[n] · g[n]`,
with `g[n]` from the detector after it has heard the cell's output up to `y[n−1]`. Written out:

1. the detector updates its level `L` from `y[n−1]` (the cell's output, **before** any make-up
   gain, so the Gain or Output knob never changes how hard it compresses);
2. the target reduction is `r_t = r(L)` from the feedback gain computer;
3. the reduction `ρ` moves toward `r_t`: by the attack if `r_t > ρ`, else by the release (the
   units say how);
4. `g[n] = 10^(−ρ/20)`, and exactly 1.0 when `ρ` is exactly 0;
5. `y[n] = x[n] · g[n]`.

**The attack is corrected for the loop's gain, and that's a decision, not a detail.** A loop with
one sample's delay and a gain of up to `R − 1` is unstable if the reduction is allowed to move
fast: linearised, each sample the error shrinks by `1 − c·(1 + s)`, where `c` is the smoothing
coefficient and `s` the local slope, and that has to stay between −1 and 1. At 20:1 with the FET's
fastest attack (20 µs, about one sample), a plain one-pole has `c ≈ 0.65` and `c·(1 + s) ≈ 13`: it
oscillates. So:

- **attack:** `c_a = (1 − p) / (1 + s(L))`, with `p = exp(−1 / (τ_a · rate))` for the unit's
  attack time `τ_a`. Then the loop closes with a pole at exactly `p`, whatever the ratio: the time a
  test measures from a step **is the dial's time**, and the loop is stable at any setting, at any
  rate, because `1 − p` is always between 0 and 1;
- **release:** `c_r = 1 − exp(−1 / (τ_r · rate))`, not corrected. Releases are slow enough
  (`c_r·R` far below 1) that the loop stays stable, and leaving them uncorrected is what makes a
  feedback compressor's release depend on the programme: it lets go faster while the signal is
  still over the threshold than when it falls away. That's the RFCs' "the ratio and the times shift
  a little with the material". A release measured from a step that falls **below** the knee reads
  the dial's time.

`s(L)` is the slope at the level the detector just heard, so the correction follows the knee
smoothly. All of it deterministic, per sample.

A unit may correct its release the same way, and the optical does (`opto.md`: on dialogue, the
programme-dependent release is pumping). The FET doesn't. So the core offers both.

## Levels

Sine-referenced dB (`shared.md`). Each unit says which detector it uses:

- **peak:** `20·log10(max(|v|, 1e-15))` of the sample, no smoothing (the FET: the reduction's own
  attack and release are the smoothing, as the unit's RC is);
- **mean square:** a one-pole on `v²`, read as `10·log10(max(2·ms, 1e-30))` (the optical, the
  de-esser), with its time constants given per unit.

Two channels: the larger of the two channels' `|v|` (peak) or `v²` (mean square), sample by sample.

## The gain, and passing to the bit

`g` is computed as `libm::pow(10.0, −ρ/20.0)`, except that `ρ == 0.0` gives `g = 1.0` without a
call. Multiplying by 1.0 is exact. So a unit whose reduction is exactly 0 passes its input to the
bit: the optical below its knee, the de-esser at rest. The anti-denormal rule (`shared.md`) is
applied to states only, never to `ρ` in a way that could make it a tiny non-zero: `ρ` below `1e-12`
dB while releasing is set to exactly 0.

## Make-up, In, voicings

- **Make-up** (the optical's Gain, the FET's Output) is a gain ramp, after the cell (and after the
  FET's stage).
- **In:** a crossfade between the unit's output and the dry input. **While out, the unit's detector
  keeps running on the input** (as if in), so lighting it doesn't start from rest and grab. Out is
  the input to the bit.
- **Voicings** (room for the LA-4 and KT76, built later, `RFC-opto.md`, `RFC-fet.md`): each unit's
  figures (times, knees, laws, the stage's constants) live in one `Figures` struct, with the RACK
  voicing a `const`. A second voicing is a second `Figures` and a second control list, not a second
  compressor. `Settings` keeps each voicing's own controls, so flipping finds them as they were.
  Flipping is a crossfade, with both voicings' cells running while the unit is in (a compressor is
  cheap), so the one flipped to is already settled. RACK is all that's built now; the structure is
  what's asked.

## Readouts

`shared.md`'s, and: `gr_db`, the reduction at the block's end, as the needle shows it (positive
dB of reduction; a panel writes it with a minus). Each unit adds its own (`acting` and `ess_db` for
the de-esser, `held` for the optical).

## The rigs, written once

In `as-dynamics`' tests, from `as-envelope`, used by all three units:

- **The stepped sine:** 1 kHz from −60 to 0 dBFS in 1 dB steps, each held for a time the unit says,
  the steady output level taken over the step's last 100 ms by `measure::tone` (the fundamental, so
  the FET's stage is measured out).
- **The DC step,** for timing: a constant from one level to another and back, at a sample position
  stated in the test. **Timing is measured on DC, not on a sine burst.** A peak detector on a sine
  only charges near the peaks, so a sine burst measures the duty cycle as much as the time; on DC
  the loop's time is the time. (The RFCs' 1 kHz bursts are kept as a second, looser check, below;
  measuring Warren's units later uses sine bursts, and the fitting compares the model's sine response
  with the unit's, like with like.) **Differs from the RFCs.**
- **The burst:** 1 kHz, −30 to −6 dBFS and back, the RFCs' signal, for a check within 25% of the
  DC-measured time.
- **The ess and the corpus:** `deess.md`'s pair; `corpus::speech`, `phrases` and `impacts`.
- **The linked check:** a source in the left channel alone, then the same in the right alone, then
  in both: the gain on both channels is the same in each run, sample by sample, and the same between
  the left-only and right-only runs.

## Pinned by tests, for the core

- The feed-forward and feedback gain computers: 0 below the knee exactly, continuous with a
  continuous slope at both knee edges (the slope checked from both sides within 1e-9), the straight
  parts at `1 − 1/R` and `R − 1`.
- `curve()` against `T + (x − T)/R` above the knee within 1e-9; monotonic everywhere.
- The loop, on DC with a test unit (fixed `τ_a`, no other behaviour), stepping from silence to a
  level whose settled output still sits above the knee (so the whole climb is on the straight
  part): sample by sample, the remaining error `(ρ_final − ρ[n+1]) / (ρ_final − ρ[n])` equals
  `p = exp(−1/(τ_a·rate))` within 1e-9, at R 2, 4 and 20, at 44.1, 48 and 96 kHz, `τ_a` from
  20 µs to 30 ms. That is the "time to 63%" claim made exactly, at times shorter than a sample.
  And the reduction never overshoots its final value by more than 0.01 dB (the loop doesn't ring).
- `g` is exactly 1.0 when `ρ` is 0.
- The rigs themselves: the stepped sine's measurement reads a plain gain of −6 dB within 0.001 dB.

## As built (step 4, 2026-11-12)

The core is `src/lib.rs`; the rigs `src/rigs.rs`, test-only, for the units' tests to share; the
core's tests `src/tests.rs`, on a test unit built from the core alone (a feedback compressor with a
peak detector, its threshold ramped, a make-up gain and In).

- **`Knee`** is both gain computers: `ff`, `fb`, their slopes, `curve` (feedback, by bisection)
  and `curve_ff`. **`Follower`** is the reduction's update, attack corrected by `1 + s`, the
  release corrected or not as the unit asks. `peak_db` and `MeanSquare` are the detectors; `gain`
  the reduction as a gain; `Fade<T>` the 10 ms crossfade. A gain or a dB setting ramps by
  `as_control::Ramp`, re-exported.
- **The 40 dB hold is on the reduction, not the gain computer.** `Knee` gives the reduction
  unheld; `Follower` holds `ρ` to 40 dB, and `curve` ends at `x − 40` where it would go further.
  Held in the gain computer, a step far over the threshold (0 dBFS DC, 30 dB over, at 20:1, asks
  570 dB) has a target of 40 and a slope of 0 for its first samples, and the climb isn't geometric:
  the loop test's exact pole would hold only for steps that ask under 40 dB.
- **`Ramp` moved into `as-control`**, out of `as-eq`, where it was private: two units needing one
  is the suite's rule for one copy. As-eq's output is unchanged, bit for bit (its tests). `next` is
  named `advance`, as clippy asks of a method that isn't an iterator's.
- **A crossfade's state is `Fade<T>`,** generic over what's faded between (`bool` for In, a
  voicing's index, a mode). `advance` gives `(from, to, t)`; settled, `t` is exactly 1.0 and a unit
  takes `to`'s path alone, so out is the input to the bit.
- **The rigs** are `Unit` (process, process2, `gr_db`), `stepped_sine` and `read_steps`, `dc_step`,
  `burst`, `blocks` (1, 64, 1,024, one channel and two, and twice), `finite`, `linked`, and
  `held_key` (the unit's own last-setting check handed in).
- **Measured here, for the units:**
  - **The burst reads a peak detector's attack slow, by its duty cycle.** On the test unit (4:1,
    hard knee), the 63% time on the 1 kHz burst is 1.69, 1.65 and 1.54 times the DC step's at
    1, 5 and 20 ms: the attack pulls only while the sine is near its crests, at any attack time.
    Pinned as found (1.4 to 1.8). **`fet.md` asks the burst within 25% of the dial at ATK 1 and 4;
    with a peak detector that looks out of reach,** and is for step 7, with its advisor, to settle
    (the figure, or the method: the 63% time of a mean of the reduction over each cycle, say).
    The optical's mean-square detector may fare better; step 6 will say.
  - **A peak detector settles under the static curve on a sine,** by about `τ_a/τ_r` of the
    reduction: at 1 ms and 1 s, within 0.15 dB of `curve` on the stepped sine. The FET's and
    optical's static-curve tests should expect that, or read with slow releases.
  - **A fast peak detector's own ripple on a 1 kHz sine is near the held key's −80 dB line:**
    at 1 ms attack and 100 ms release, −79.8 dB above 4 kHz at 96 kHz, with nothing being swept.
    The core's held keys run the test unit at 10 ms and 500 ms, so what's measured is the ramp. The
    FET's held key (step 7) will meet this at its own fast settings.
  - A branching mean-square detector with a fast attack rides a sine's crests and reads up to 3 dB
    over its mean square (−3.2 for −6 dBFS at 0.5 ms and 40 ms); symmetric, it reads the mean
    square. The de-esser's thresholds are relative, so that's its spec's to weigh (step 5).
