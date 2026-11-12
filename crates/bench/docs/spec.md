# Spec: `as-bench`, measuring hardware against a model

From the suite's session, 2026-11-12, at Warren's asking: the measuring the LA-4 and KT76 voicings
need (`../../../RFC-opto.md`, "Measuring Warren's LA-4"; `../../../RFC-fet.md`, "The KT76 voicing"), made a suite
crate and built first, so it's ready before the first sitting at the rig and useful sooner than the
voicings are. Read `../../../specs/shared.md` first: its rules for numbers and tests hold here.
Built in step 0 (2026-11-12) and moved here; "As built" at the end says what the build settled.

## What it's for

Warren's time at the rig is the scarce thing, and measuring real hardware is his gate (the family's
rules): the code can be built and tested here, the sitting can't. So the bench has three jobs:

1. **Play and record** known signals through the interface, out to a unit and back, sample-locked,
   without touching anything Warren hasn't patched.
2. **Check each capture at once,** pass or fail in words, so a clipped, mis-patched or inverted
   capture is redone in the same sitting, not found a week later.
3. **Reduce captures to figures and pictures,** the same way for the unit and for a model, so the
   two can be put side by side by Warren and by an AI working with him. An AI can't see a screen or
   hear: it reads numbers and image files. So everything the bench shows is also written as a file.

**Not a live oscilloscope** (no free-running display, triggers or timebase), and not a tool. It's the
shared part. The front that runs it is a tool's (below, "The front").

**Why a suite crate:** sonorack measures the LA-4 and KT76 with it, and it's the shape of every
"play something known through it and see what comes back" in the family. dexter-place's
`place-measure` (a sweep, and an IR made from its recording) is the same idea. Place is held as it is
(`../../../../CLAUDE.md`, 2026-11-11), so nothing is moved from it and nothing is copied: this is written
fresh, and if place stays, its measuring can come onto this crate later.

## What's in it

### Signals

Built from `as-envelope`'s signals, at the session's rate, each with a short **sync mark** in front
(below) and 0.5 s of silence after:

| Name | What | For |
|---|---|---|
| `silence` | 5 s of nothing | the noise floor |
| `sweep` | an exponential sine sweep, 20 Hz to 20 kHz, 10 s, faded in and out (Farina's) | the response at rest, and harmonics |
| `steps` | 1 kHz stepped from −60 to 0 dBFS in 1 dB steps, each held a time the measuring list says | the static curve; distortion against level |
| `bursts` | 1 kHz bursts between two levels, each held a time the list says, repeated three times | attack and release |
| `hold` | a tone at one level held a stated time, then a quiet tone (or silence, if the list says) | the optical's memory (0.1 s and 2 s) |
| `steady_sweep` | a sweep over a steady tone that holds the unit at a set reduction | the response while compressing |

**The sync mark:** a short linear chirp, 2 kHz to 8 kHz over 20 ms, at −20 dBFS, then 200 ms of
silence. Every capture is aligned by it (below). A sweep's or step's level is never set by it.
Its ends are tapered over 2 ms by a raised cosine, so its correlation's sidelobes are low; it's
played at −20 dBFS or the ceiling, the lower (2026-11-12, as built).

**The ceiling:** every signal is scaled so its peak is at most the session's ceiling, **−6 dBFS by
default**, which Warren raises by typing it. Out of a +4 interface, 0 dBFS can be +18 to +24 dBu,
more than some inputs want. The ceiling is written into every capture's notes.

**Levels are dB under the ceiling** (2026-11-12, as built): 0 is the ceiling, and nothing a
signal is given can pass it. So `steps` runs from 60 dB under the ceiling to the ceiling (−66 to
−6 dBFS by default), not −60 to 0 dBFS, which a −6 ceiling can't play. `sweep` and `steps` peak at
the ceiling; the others at their loudest level given, and never under the mark. Each part of a
signal carries the dBFS it was played at, which the static curve and the reduction read.

### Play and record: one client, sample-locked

A JACK client of two outputs and two inputs (`bench_out_L/R`, `bench_in_L/R`), on the server
`JACK_DEFAULT_SERVER` names, never starting jackd and never opening a card. **Its ports are connected
nowhere until Warren patches them** (in jackbay or Dexter's bay); the bench never connects a port.

- The signal is loaded whole before it's played, and the record buffer allocated whole (its length
  is the signal's plus a second). On JACK's thread: copy out, copy in, count, nothing else. No
  allocation, lock or syscall there.
- **Playing and recording start in the same cycle,** so the round trip's latency is fixed for the
  capture and found from it.
- After the last sample, the buffer goes off the thread through a ring and is written by a thread
  of its own, through `as-keep`, never over a file.
- An xrun during a capture fails it (the check says so).
- A capture plays only when asked, one at a time.

This is a third JACK client in the suite, beside `as-jack-keeper` (outputs) and `as-jack-capture`
(inputs). What it shares with them (opening on the server named, the reattach, the wanted
connections) is used from them or moved into one place they all use, **never a third copy**
(`../../../../CLAUDE.md`, "one of each thing"). The builder decides which, in the step, with a `CHANGES.md`
line if either crate's interface moves.

### The loopback, first

Before any unit: the interface's out patched straight to its in. Three captures (`silence`, `sweep`,
`steps`) give the **calibration** every later figure is corrected by:

- the round-trip latency, in samples, from the sync mark;
- the interface's response, to be divided out of a unit's;
- its gain, dBFS out to dBFS in, at 1 kHz;
- its polarity;
- its noise floor and its own distortion, the floor under which nothing of a unit's is believed.

A calibration is kept as a file and named by date; a unit's session names the calibration it uses.
Re-run it if anything on the interface changes.

### The check, after every capture

Run at once, said in words, one line each, then **PASS** or **REDO**:

| Check | Passes if |
|---|---|
| clipped | no input sample at or above −0.1 dBFS |
| level | the return's peak between −40 and −1 dBFS (a peak at −0.1 or over is said by "clipped" alone) |
| aligned | the sync mark found, its correlation peak at least 10× the next, the latency within 2 samples of the calibration's |
| polarity | the same as the calibration's (the mark's correlation positive) |
| signal over noise | at least 40 dB over the floor, over the part measured: the calibration's floor or the capture's own (the last 100 ms before the signal proper), the higher |
| xruns | none |
| length | the whole signal came back |

A REDO says what to look at ("clipped: lower the ceiling or the unit's output"; "not aligned: is
the unit's in patched?"). A capture that fails is kept, marked failed, never used by the reduction.

### Reduction: captures to figures

Offline, deterministic (`../../../specs/shared.md`'s rules), each from aligned captures with the calibration
divided out. Here rather than in `as-envelope` or `as-dynamics`' test support, where `../../../RFC-opto.md`
put it: the unit's side and the model's must go through the same code, and a tool runs it.
**Differs from the RFC.**

- **response:** from `sweep`, the transfer by deconvolution (Farina's inverse filter), the linear
  part separated from the harmonics by time; magnitude in dB on third-octave points, 20 Hz to
  20 kHz, smoothed by a sixth of an octave;
- **harmonics:** the second to fifth against level, from `steps` by `as_envelope::measure::tone`
  at each step, down to the calibration's floor + 10 dB, and from `sweep`'s separated harmonics;
- **static curve:** the steady output level of each step against its input level, the last 100 ms
  of each step;
- **gain reduction over time:** on `bursts` and `hold`, the output's level against what the unit
  gives at rest at the same input (from the static curve's lowest steps), so what's left is the
  reduction; a level every half cycle of the 1 kHz tone;
- **times:** from that envelope, the attack's 63% and the release's 37% times, each of the three
  repeats, their mean and spread;
- **memory:** the release time after `hold` 0.1 s and after 2 s, and their ratio.

The figures go into a small text file (TOML), dated and named as the unit's ("Warren's LA-4,
serial if known"), with the session's notes: interface, its settings, the calibration's name, the
ceiling, the unit's knob positions for each capture. **That file is what may be committed**
(Warren, 2026-11-12, `RFC-opto.md` question 5). **The captures are never committed,** and are kept
under Warren's own directory.

### The model through the same bench

`run_model(signal, &mut impl FnMut(&mut [f64], &mut [f64]))`: any unit (an `as-dynamics` unit, an
`as-eq`, a test's model) is fed the same signals offline, and its output reduced **the same way**,
by the same code. So the unit's figures and the model's are always like with like (a sine burst
measured the same way on both, whatever a peak detector does with a sine). A comparison is the two
figure files and their differences, against the acceptance tolerances of the voicing being fitted.

The fitting itself (choosing a voicing's figures to match) is the voicing's step, later. The bench
gives it the measurement on both sides.

### Pictures, as data

The bench returns each picture as **data**: named series of (x, y) points with their axes, units,
scales (log or linear) and labels. It draws nothing (the audio suite doesn't depend on the panel
suite). The front draws them with `lp-paint`, on the panel and **into PNG files** (`lp-paint`'s
`png`), one per picture, written beside the figures:

- the gain reduction over time, unit and model overlaid, one burst at a time;
- the static curve, unit and model, with the curve's ratio marked;
- the response, at rest and under reduction;
- the harmonics against level;
- for a failed check: the capture's start, the sync mark, and what was found.

### A sitting, written down

A sitting is a **measuring list**: an ordered list of captures, each naming its signal, its levels
and times, and what Warren sets on the unit by hand ("ratio 4, input 3"). The list is a text file
the voicing's step writes (the RFCs ask for one sitting of about an hour per unit, said up front).
The bench keeps a sitting in a folder (`sitting-NNN/`, made through `as_keep::Folder`), with a manifest of what was
captured, what passed, and the knob positions typed. A sitting stopped halfway **resumes** where it
stopped, and any capture can be redone, the redo kept beside the first.

## The front

The bench is driven by a tool's front: a command bar (or the terminal, in `STYLE.md`'s way) that
shows the next capture and its knob positions, waits for Warren to set them and confirm, plays,
shows the check, and on REDO offers it again. It writes the PNGs. **`sonorack measure`** is the
first, as the RFCs have it.

So that it's ready before the rest of sonorack, the front can be sonorack's first step, ahead of
the EQ: a workspace with only `measure` in it needs no `as-eq`. That's a change to sonorack's order,
for sonorack's own session, and Warren's to say (Question 1).

## Pinned by tests

No outside audio: the "unit" in every test is code.

**Offline** (no JACK):

- every signal at 44.1, 48 and 96 kHz: its peak at the ceiling within 0.01 dB, its length as said,
  the same bits twice;
- alignment: a signal delayed by 0 to 5,000 samples and by fractions (through a fixed fractional
  delay), and with noise at −60 dBFS added: the latency found to the sample, the polarity found when
  inverted;
- the check: one synthetic capture made to fail each line (clipped, too quiet, inverted, cut short,
  mis-aligned, noisy), each failing that line alone;
- the reduction against units whose figures are known exactly:
  - a gain of −6 dB: response flat at −6 within 0.05 dB, 30 Hz to 15 kHz;
  - an `as-biquad` peak: the response its `gain` gives, within 0.1 dB;
  - a fixed `tanh` stage: the harmonics its series gives, within 0.5 dB, down to −90 dBFS;
  - a plain feed-forward compressor written in the test (known ratio, knee, attack, release): the
    static curve within 0.2 dB, the times within 5%;
  - a two-speed release with a memory, written in the test: the memory ratio within 10%;
- `run_model` and a capture of the same model through the JACK path (below) reduce to the same
  figures within 0.05 dB and 2% for times.

**On a dummy JACK server** (`as_jackd::Dummy`, opt-in by `SUITE_JACK_TEST=1`, `benchtest-<pid>`):

- the bench's outs patched to its ins: the loopback's latency found and the same on every capture;
  the gain 0 dB within 0.001;
- a test client in between running a known unit (the plain compressor above): its figures recovered
  as offline;
- the bench never connects a port: a test reads the crate's source for the calls that would, as
  `as-transport`'s does for transport control;
- an xrun forced (a slow test client): the check fails "xruns".

## Warren's gate

The loopback at the rig first, through his interface, then each unit, by the measuring list. The
session that builds the bench stops when the tests pass and hands him: how to patch the loopback, the
ceiling and why, and what each check's REDO means. Its first real use can be early: a loopback, and
a quick look at either unit's static curve and times, long before a voicing is fitted.

## Questions for Warren

1. **sonorack's `measure` first,** ahead of its EQ, so the bench has a front as soon as it's built?
   Recommended: yes. It needs nothing but the bench, and the sitting at the rig can start while the
   units are built.
2. **The ceiling at −6 dBFS by default,** raised only by typing it? Recommended: yes. The outs go to
   a unit's input, but a mis-patch could send a full-scale sweep to the monitors.

**Settled (Warren, 2026-11-12):** both as recommended. sonorack's `measure` comes first, ahead of its
EQ (for sonorack's own session to write into its order); the ceiling is −6 dBFS by default.

## As built (step 0, 2026-11-12)

What the build settled that the text above left open, or changed, and why. Each was put to the
advisor (Fable 5.2) or to Warren as the step went.

- **Levels under the ceiling,** and `hold`'s quiet tone after (above). A release can't be read on
  silence: there's nothing to read the gain from. The list gives its level (−40 under by default).
- **The mark's search:** every lag up to the 200 ms after it, so the signal proper never falls
  under the template, for a latency up to 200 ms. "The next" peak is the largest more than 1 ms
  from the first, outside the lobe of the mark's own carrier. A capture where nothing correlates
  has no mark (found at once on a silent capture).
- **The check's lines are each alone:** a clipped capture fails "clipped", not "level" too, and a
  noisy capture is caught by its own floor even when the calibration's was quiet.
- **Inputs:** the same signal goes to both outputs, and each list entry names the inputs it keeps
  (`inputs = ["L"]`, both by default), so a one-channel unit isn't failed on its dead side.
- **The response** divides the capture's deconvolution by the played sweep's own, worked out the
  same way, then the calibration's third-octave points: the fades and the band's edges fall out.
  The linear part is cut from a quarter of the way back to the second harmonic to 0.5 s on, each
  harmonic a quarter of the way to its neighbours. `realfft` picks a SIMD path when it runs: the
  same build gives the same bits, not promised between machines, as `as-bands` says.
- **The reduction over time** reads the largest sample in each half cycle of the tone; attack and
  release are the time to move 63% of the way from where the reduction was to where it settled
  (the mean of its last 10 ms), between half cycles on a line.
- **The JACK client** stays open for a sitting, so Warren's patches last between captures. A
  capture goes to JACK's thread in a box through a ring of one and comes back through another;
  it isn't reattached mid-capture (it says JACK went away, and the front opens the bench again).
  What it shares with `as-jack-keeper` and `as-jack-capture` is `as-jack-client` (Warren,
  2026-11-12: a crate of its own, the one copy of opening, locking memory, the notices and
  `reconcile`).
- **A sitting's folder** is numbered when it's begun (`Folder` finished at once, so a sitting
  stopped halfway is still there to resume). Takes are 32-bit float WAVs (`as-wav`), exactly
  what JACK gave; the manifest is replaced whole by `as_keep::save`.
- **Harmonics from the sweep** (`figures::sweep_harmonics`): each at its own frequency, against
  the sweep's level, leaving out inputs whose sixth of an octave reaches into the sweep's 100 ms
  fade-in (a harmonic grows faster than the level, so they read low). The interface's own aren't
  taken off; its calibration's THD says how far down they're believed.
- **Under `steady_sweep`,** the response leaves out the point whose sixth of an octave holds the
  steady tone: the tone isn't the sweep's and reads there as a bump (+13 dB in a test).
- **On JACK's thread:** the signal and record buffers are written through before a capture goes
  (`as_conv::touched`), so no page is first touched there; and each capture carries a number, so
  one that ran out of time and came back late is dropped, never taken for the next.
- **The calibration is a file** (`Calibration::write`, `stem-NNN.toml`, named by date by its
  caller), read back by `from_toml`.
- **Left as they are, said here:** a take is written by the caller's thread (the front's), not a
  writer thread of the bench's own; a round trip of 200 ms or more isn't found and reads "not
  aligned", whose REDO blames the patching; the memory test runs at 48 kHz only, and its unit
  counts how long it held while reducing, which is the test's model, not a figure of the LA-4's.
- **The dummy servers:** `as-jack-keeper`'s, `as-jack-capture`'s and `as-transport`'s tests had
  each its own; all are on `as_jackd::Dummy` now (Warren, 2026-11-12).
- **Not in the crate:** the comparison against a voicing's tolerances (the voicing's step) and
  drawing (the front, with `lp-paint`).
