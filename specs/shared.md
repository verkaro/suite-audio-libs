# Spec: what the shaping units share, and `as-control`

From the suite's session, 2026-11-12, at Warren's asking: the specs for sonorack's four units,
written from the settled RFCs (`../RFC-shaping.md`, `../RFC-eq-dynamics.md`, `../RFC-eq.md`,
`../RFC-deess.md`, `../RFC-opto.md`, `../RFC-fet.md`) and from sonorack's own decisions
(`../../sonorack/CLAUDE.md`, `../../sonorack/docs/design.md`). The RFCs say what each unit is and
why. These say what has to be true when it's built, with every figure the RFCs left to "the spec"
chosen.

Eight files, read in this order:

| File | What it covers | Builds |
|---|---|---|
| `shared.md` (this) | the rules every unit keeps, and the test rigs | |
| `../crates/control/docs/spec.md` | `as-control`, the controls as data | `as-control` (built) |
| `../crates/bench/docs/spec.md` | measuring hardware against a model: play, record, check, reduce | `as-bench` (built) |
| `../crates/eq/docs/spec.md` | `as-biquad`'s matched designs, and `as-eq` | `as-biquad` additions, `as-eq` (built) |
| `../crates/dynamics/docs/spec.md` | `as-dynamics`' core: the gain computer, the feedback loop, levels, rigs | `as-dynamics` core (built) |
| `../crates/dynamics/docs/spec-deess.md` | `deess` | `as-dynamics`' `deess` (built) |
| `opto.md` | `opto`, its RACK voicing | |
| `fet.md` | `fet`, its RACK voicing | |

**Where they live.** Here, in `specs/`, until each crate exists; the workspace takes every folder
under `crates/` as a member, so a spec can't sit in a crate's folder before its `Cargo.toml`.
When a crate is made, its spec moves into it as `docs/spec.md` (`git mv`, in the same commit),
and the line above is changed to point there. One home for each.

**Where a figure here differs from an RFC,** the spec says so and why, under "Differs from the
RFC". The RFCs stay as the record; they aren't edited.

## The steps, and which model builds each

Steps 1 to 7 waited on the suite's s1 and s2 (`../../NOTES.md`, the bookmark), as the RFCs say; the EQ's, 1 to 3, now go ahead of them (Warren, 2026-11-12; `../PLAN.md`), and so do 4 to 7: the RACK voicings are built and heard as the specs have them first (Warren, 2026-11-12). Step
0, the bench, needs nothing of theirs and none of the units, so it can go first, once Warren says
so; sonorack's `measure` then runs it, built first in sonorack (`as-bench`'s spec, settled 2026-11-12).

**One step per session** (Warren, 2026-11-12: cost matters). Each session builds one step, ends it
(below), and stops, so Warren can clear the context and choose the model for the next.

**Opus 5.5 builds every step.** Fable 5.2, about twice the price, comes in only as its **advisor**
(Claude Code's `/advisor`, or the `advisorModel` setting), on the steps where the figures are
untried or a mistake would mislead what comes after. The advisor is consulted at a few named moments, so Fable's tokens are spent only on judgement,
not on writing code. If that isn't enough, the step is rerun on Fable alone (below).

| Step | What | Spec | Model | Advisor | Effort | Why |
|---|---|---|---|---|---|---|
| 0 | `as-bench` | `../crates/bench/docs/spec.md` | Opus 5.5 | **Fable 5.2** | high | built first, so the rig can start early (Warren, 2026-11-12); known methods (Farina's sweep, alignment), but a duplex JACK client and a reduction whose mistakes would mislead every fit later |
| 1 | `as-control` | `../crates/control/docs/spec.md` | Opus 5.5 | none | medium | small and fully specified: types, stepping, parsing |
| 2 | `as-biquad`: `matched`, the matched peak and shelves, `inverse` | `../crates/eq/docs/spec.md` | Opus 5.5 | none | high | the method and its figures were prototyped here; the tests say what to hit |
| 3 | `as-eq` | `../crates/eq/docs/spec.md` | Opus 5.5 | none | high | assembly on step 2, and the crossfade written out |
| 4 | `as-dynamics` core: gain computers, the loop, levels, the rigs | `../crates/dynamics/docs/spec.md` | Opus 5.5 | none | high | written out to the formula, with an exact test for the loop |
| 5 | `deess` | `../crates/dynamics/docs/spec-deess.md` | Opus 5.5 | none | high | its detector was prototyped here |
| 6 | `opto`, RACK | `opto.md` | Opus 5.5 | **Fable 5.2** | high | untried figures; two envelopes, a memory and a loop that interact |
| 7 | `fet`, RACK, its stage last | `fet.md` | Opus 5.5 | **Fable 5.2** | high | untried figures, the aliasing test that may call for an internal double rate |

**When to consult the advisor** (steps 0, 6 and 7). An advisor helps only if it's asked at the right
moments, and the builder can't be counted on to spot them alone, so they're named:

- before writing the step's code: the plan, against the spec;
- before changing any figure in a spec;
- when a test still fails after one honest fix, before a second;
- before choosing the FET's internal double rate;
- before ending the step: the diff and the test results.

Not for routine code, a compile error, or a test that a plain fix makes pass.

sonorack takes each unit as it lands: its step 1 after step 3 here, its step 5 after step 5 here,
and so on. The LA-4 and KT76 voicings and `sonorack measure` come after Warren has measured his
units; these specs only keep room for them (`../crates/dynamics/docs/spec.md`, "Voicings").

**When a step is stuck,** a test still failing after a real try at both the code and the figure
(and the advisor's advice, where there is one): don't keep going round. Commit what passes, write
in the spec under the step's heading what fails, what was tried and what was measured, end the step
as below, and say so. Warren reruns the step with Fable 5.2 as the model from there: on steps 1 to
5 an advisor is tried first, on 0, 6 and 7 it's Fable alone.

### Starting a step

In `suite-audio-libs`, with sonorack readable: `claude --add-dir ../sonorack`. Set `/model` to the
step's model and effort, and `/advisor` to the step's advisor or off. Then: "Build step N of
`specs/shared.md`." The session reads `../CLAUDE.md`, this file, and the step's spec, and checks
`PLAN.md`'s line for the shaping units to see which steps are done.

### Ending a step

Every step ends the same way, so the next session can start cold:

1. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`: all pass.
2. For a new crate: its header comment, README entry, `VENDORED.md` ("Written here, not moved"), the
   `[workspace.dependencies]` line, and CI as the suite has it. Its spec moved into it (`git mv`
   to `docs/spec.md`), and this file's table pointing there.
3. A line in `CHANGES.md` for any interface added or changed.
4. Any figure changed in the spec, dated, with why.
5. `PLAN.md`'s line for the shaping units says which steps are done (the one home of that fact).
6. One commit for the step, in the suite's voice, then `../family.sh check suite-audio-libs`, then
   push.
7. Stop, and say: the step done, the next step and its model, and anything Warren has to hear or
   decide.

## For the session that builds

- **The tests are the spec.** Each "pinned by tests" list is the acceptance. Write the tests
  first where you can, from these figures.
- **A figure that can't be met:** measure why, change the figure here with the date and the
  measurement, and go on. Never loosen a tolerance quietly, and never loosen one that protects
  something Warren will hear (a click, a null, a level that jumps) without asking him.
- **Character is Warren's.** A knee, a bump, a release that sounds right: the spec gives a
  starting figure and the test pins the shape, not the taste. Stop at his gates (sonorack's
  steps) and say what to listen for. Never claim it was heard.
- **Determinism is the rule above the rest.** If a choice is between a faster path and the same
  bits everywhere, it's the same bits.

## Rules for every unit

**Numbers.** `f64` in and out, one block a channel, in place. Transcendentals through `libm`, never
`std`'s, as `as-biquad` does. No `mul_add`, no `fast-math`, no SIMD the compiler may reorder;
arithmetic in a fixed order. `powf`, `exp`, `log10`, `tan` are all `libm::`.

**Per sample.** Every state advances one sample at a time. No look-ahead. Nothing depends on where
a block starts or ends.

**Rate.** `new(rate: f64)`. Everything in seconds or hertz is designed at that rate. 44.1, 48 and
96 kHz are tested; 88.2 and 192 should work untested.

**Levels** are dB, **sine-referenced**: a sine of peak 1.0 reads 0 dB, whatever detects it. A
peak detector reads `20·log10|x|`; a mean-square detector reads `10·log10(2·ms)`. A level never
goes below −300 dB (`max(·, 1e-15)` on a magnitude, `1e-30` on a square), so a log never sees 0.

**Denormals.** A state on the audio path (a filter's memory) whose magnitude falls below `1e-200`
is set to exactly 0, by a plain comparison, each sample. A detector's state uses the floors above.
Nothing is added to the signal, so every null and "to the bit" test stays exact. One test feeds a
−300 dBFS tail for ten seconds and checks a block takes no longer at the end than at the start
(within 2×, timed over many blocks).

**Settings** are a plain `Copy` struct with `Default`, every field held to its range by the setter,
never refused. `set(&Settings)` allocates nothing, makes no syscall and may run on the audio
thread; it sets targets, and the unit moves to them by the ramps below. **A setting takes effect at
the first sample of the next `process` call.** So "the same to the bit at any block size" is
claimed for a stream of settings applied at the same sample positions: the tests apply them at
multiples of 1,024 samples, which are block starts at every size tested.

**Ramps,** so nothing clicks. Three kinds, all counted in samples (`round(t · rate)`), so they're
the same at any block size:

- **A gain** (make-up, output, trim, Input): linear in amplitude over 5 ms. A new target mid-ramp
  starts a new 5 ms ramp from where the gain is.
- **A level setting in dB** (a threshold, Reduce, Range): linear in dB over 5 ms, retargeted the
  same way.
- **A crossfade** (an EQ's new coefficients, In and out, a mode, Listen, a voicing): linear,
  equal-gain (`(1 − t)·a + t·b`, `t = k/N`, `N = round(0.010 · rate)`), 10 ms. A crossfade runs to
  its end. A target that arrives during one waits; when it ends, **the latest target waiting**
  starts the next. So a held key glides in 10 ms hops, and nothing is dropped but the steps between.

**Two channels.** `process(x)` for one, `process2(l, r)` for two. The EQ runs the same coefficients
on both, each with its own state. A compressor and the de-esser use **one detector for both**, fed
the larger of the two channels' values sample by sample (the larger magnitude, or the larger
square), so the image doesn't move. sonorack's DUAL mode is two one-channel instances, set alike;
nothing here knows of it.

**Readouts.** A plain `Copy` struct each unit fills as it runs, read by the tool after a block:
`in_peak_db`, `out_peak_db` (the block's largest, both channels), `gr_db` (the reduction at the
block's last sample, 0 for the EQ), and what a unit adds (`../crates/dynamics/docs/spec.md`). Nothing in a unit draws,
logs or allocates.

**No shared trait** across the crates. sonorack wires the chain.

**What's out:** external side-chains, look-ahead, mid/side, oversampling as a setting, presets in
a crate, automation, routing.

## `as-control`: the controls as data

Built (step 1, 2026-11-12). Its spec is `../crates/control/docs/spec.md`, with what was chosen
as it was built.

## The test rigs every unit uses

In each crate's tests, from `as-envelope` (`signal`, `corpus`, `measure`). Write them once per crate
(the EQ's and `as-dynamics`' are the two), not once per test.

- **Block sizes:** the same input and settings stream through blocks of 1, 64 and 1,024, bit for bit
  equal (`to_bits` on every sample).
- **Twice:** a run repeated on a fresh unit, bit for bit equal.
- **Non-finite:** `as_envelope::signal::torture` and silence, DC at ±1, full-scale noise, and a
  −300 dBFS tail: every output sample finite.
- **A held key:** a 1 kHz sine at −12 dBFS, with a setting changed every 2 ms for a second (the
  control swept across its whole range and back by fine steps). Passes if:
  - **no step:** no sample-to-sample difference is larger than 1.1× the largest found in the same
    signal through the unit held at either end of the sweep;
  - **no burst:** in 4,096-sample Hann windows hopping by 1,024, every bin above 4 kHz is below
    −80 dBFS (sine-referenced);
  - **the last setting holds:** after the sweep and 50 ms, every ramp and crossfade has ended and
    the unit's settings in force (a `settings()` getter) equal the last sent. For the EQ, which
    has no memory beyond its filters, the output also matches a fresh unit set to the last setting
    and fed the same signal, within 1e-6, once both have run a second (a 30 Hz shelf's memory takes
    that long to fade).
- **In and out:** the unit out is the input to the bit.
