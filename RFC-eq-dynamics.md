# RFC: as-eq and as-dynamics, the units in detail

From the family's session, 2026-11-11, at Warren's asking: what `RFC-shaping.md` settled, written
out far enough that a session can write the spec (`docs/spec.md` in each crate, or one for both)
and then build, without coming back for the idea. Where this says "the spec decides", the spec
session chooses and writes why; where it says "Warren", it's his ear or his word. Nothing here is
code, and no figure is final until the spec's tests pin it.

The four units: a parametric EQ (`as-eq`); a FET-style compressor, an optical-style compressor and
a de-esser (`as-dynamics`). Their first home is sonorack (`../Sonorack.md`), in the order
in → EQ → de-esser → optical → FET → out; BRR, ducky, place and irma take them after. Dexter never
does. The manner of each is a hardware unit's, as "style"; none claims to be a model of the unit.

## What they have in common

**One rule above the rest:** the same input, settings and rate give the same output to the bit,
at any block size, on any machine. Everything below serves it.

- **Numbers.** `f64` in and out, one block a channel, in place, as `as-biquad` and Tape's chain
  work. Arithmetic in a fixed order, no fused or vectorised paths the compiler may reorder
  across machines (no `mul_add` unless the spec proves it the same everywhere; the family's
  crates don't use it). No `fast-math`. Denormals are not flushed by the hardware in Rust, so a
  state that decays toward zero is kept out of the denormal range the same way in every unit:
  the spec decides (an alternating tiny offset added to the detector's state is the usual way)
  and one test feeds a −300 dBFS tail and checks the time a block takes doesn't grow.
- **Per sample, no look-ahead.** Every unit's state advances one sample at a time, so a block's
  size can't show. A compressor's detector sees the sample before the gain is applied to it (feed
  forward) or the one just produced (feedback), never a sample ahead.
- **Rate.** Each unit is made for a rate (`new(rate)`), and everything in seconds or hertz is
  designed at that rate from an analog prototype or a time constant. 44.1, 48 and 96 kHz are
  tested; 88.2 and 192 should work untested. The same to the bit is claimed only at one rate.
- **Settings** are a plain struct by value, with `Default`, every field held to its range (never
  refused: a tool shows the held value). `set(&Settings)` is cheap, allocates nothing and may be
  called from the audio thread; it sets targets and the unit ramps to them. A tool keeps its own
  settings file and the ring that carries a change to the thread; the crates know neither.
- **Ramps, so nothing clicks.** A gain (make-up, output, bypass, the de-esser's and compressors'
  computed gain) moves at most a fixed rate in dB a millisecond or over a fixed time (5 ms for a
  knob's gain, the spec decides). The EQ's coefficients change by crossfading the output of the
  old filter into the new over 10 ms (two filter sets, one block of both: cheap at six bands),
  not by interpolating coefficients, which can pass through unstable sets. Bypass of a unit is
  the same crossfade. A test says no sample-to-sample step during a ramp exceeds what the ramp's
  slope allows, and the spectrum during a ramp has no broadband burst above −80 dBFS.
- **Stereo.** A unit processes one or two channels (`process(l)`, `process2(l, r)`). The EQ uses
  the same coefficients on both. A compressor and the de-esser use one detector for both, fed the
  louder channel's detector signal (the maximum of the two rectified values, the spec may choose
  the sum), so the image doesn't shift. A mono source uses one channel.
- **Readouts for a panel,** read off the thread by the tool copying plain fields after a block:
  input and output peak (dBFS, held for the block), gain reduction now (dB), and for the de-esser
  whether it's acting. Nothing in a unit draws or logs.
- **Display without audio:** the EQ's `response(hz) -> dB` for the whole and for each band; a
  compressor's `curve(level_db) -> out_db`, its static transfer, so a panel draws the knee without
  running sound.
- **No shared trait across the crates.** Each crate stands alone, as the suite's others do;
  sonorack wires the chain and owns the order. If the spec finds the chain wants a trait, it's
  sonorack's, not the suite's, until a second tool wants the same one.

**What's out, to stay short of a DAW:** external side-chains, look-ahead, mid/side, oversampling
as a user setting (if the FET's stage needs it, it's fixed and internal), presets in the crate,
automation, any routing. A tool may offer a side-chain high-pass if the spec keeps one (below).

## `as-eq`

**Slots.** Six, in a fixed order, each with a kind and the controls its kind has. Any slot may be
any kind; the spec may fix the ends (slot 1 a high-pass, slot 6 a low-pass) if that reads better
on a panel. Processed in slot order. A slot off is passed over, not run at unity.

| Kind | Controls | Range (held to) | Default |
|---|---|---|---|
| Off | — | — | the default for every slot |
| High-pass | frequency; slope 6, 12, 18 or 24 dB an octave | 20 Hz to 20 kHz, log; slope by sections (first-order, Butterworth second, third as first and second, fourth as two Butterworth) | 80 Hz, 12 |
| Low shelf | frequency, gain, Q (or slope) | 20 Hz to 20 kHz; −18 to +18 dB by 0.1; Q 0.5 to 1.0 | 200 Hz, 0 dB, 0.71 |
| Peak | frequency, gain, Q | 20 Hz to 20 kHz; −18 to +18 dB; Q 0.3 to 10, log | 1 kHz, 0 dB, 1.0 |
| High shelf | frequency, gain, Q | as the low shelf | 6 kHz, 0 dB, 0.71 |
| Low-pass | frequency; slope as the high-pass | as the high-pass | 12 kHz, 12 |

And for the whole: output trim, −12 to +12 dB, and bypass.

**Design.** Every section is `as-biquad`'s, from the analog prototype at the rate. Near Nyquist
the bilinear peak and shelf cramp (a 16 kHz peak at 48 kHz leans and narrows), so **peaks and
shelves whose frequency is above a sixth of the rate use a matched design** (the magnitude
matched to the analog prototype at DC, the centre and Nyquist, as `as-biquad`'s matched low-pass
does; Vicanek's 2016 method is the reference), added to `as-biquad` as `peak_matched` and
`shelf_matched` with a `CHANGES.md` line. Below that frequency the plain bilinear is used, so the
EQ is `as-biquad` plus two designs. The spec decides the crossover between the two designs and
proves that a band moved across it changes by less than 0.1 dB at any frequency.

**What "good" means, pinned by tests:**

- at 44.1, 48 and 96 kHz, every kind at a set of frequencies across its range reads its gain at
  its centre (peak, shelf) or −3 dB at its corner (passes) within 0.1 dB, measured by
  `response(hz)` and by running a sine;
- a 16 kHz, +6 dB, Q 2 peak at 48 kHz reads within 0.5 dB of the analog prototype from 8 to
  20 kHz;
- all slots off, or all at 0 dB, passes the input unchanged within 1e-12 (a null);
- two slots at the same frequency, +6 and −6, null to within 1e-9 (bands are independent);
- the response of the whole equals the product of the bands within 1e-9;
- a ramp test (above) on a gain step and on a frequency step;
- block sizes 1, 64 and 1,024 the same to the bit; a run twice the same; no non-finite sample
  on silence, DC, full-scale noise, or a −300 dBFS tail.

## `as-dynamics`

Three units in one crate, modules `fet`, `opto` and `deess`, over a shared gain computer and shared
test signals. The gain computer works in dB: given a detected level in dBFS, a threshold, a ratio and
a knee width, it returns the gain reduction in dB with a quadratic knee (the usual form); the units
differ in what they detect and how fast.

**Meter.** Each reports gain reduction in dB, as the unit's needle would, and in and out peaks.

### `fet`, the FET style (the 1176's manner)

**Topology: feedback.** The detector listens to the output of the gain cell, one sample late, so
the ratio and the times depend on the programme as a feedback compressor's do (harder material
compresses sooner and releases differently). The spec writes the loop: `y[n] = x[n] · g[n]`,
`g[n]` from the detector's state after seeing `y[n−1]`.

| Control | Range (held to) | Default | Notes |
|---|---|---|---|
| Input | 0 to +40 dB | +20 | drives a fixed threshold, as the unit has no threshold knob; the threshold is the spec's (−24 dBFS at input 0 is a start) |
| Output | −30 to +20 dB | 0 | make-up, ramped |
| Attack | 20 µs to 800 µs | 200 µs | the dial's 1 (slowest) to 7 (fastest), in seconds inside |
| Release | 50 ms to 1.1 s | 400 ms | the dial's 1 (slowest) to 7 (fastest) |
| Ratio | 4, 8, 12, 20, all | 4 | "all" is the all-buttons state |
| Side-chain high-pass | off, 80 Hz | off | one second-order; keeps lows from pumping; the spec may drop it |
| Bypass | | off | crossfaded |

**Detector.** A peak detector on the rectified feedback signal with the attack and release as
one-pole time constants in the linear domain (the unit's side-chain is an amplifier into an RC;
the spec may choose the dB domain if the curve tests prefer it, and says why). Attack and release
are measured by the tests as the time to 63% of the final gain reduction (attack) and to 37% of
it (release), on a 1 kHz burst from −30 to −6 dBFS and back, within 10% of the setting; the
feedback topology shifts them a little with level, which the test allows for by measuring at
one level and stating it.

**Knee and ratios.** A soft knee of about 6 dB at 4:1, narrowing as the ratio rises (the spec's
table). **All-buttons:** ratio about 20:1 with a wider knee and the threshold raised a few dB,
and the attack and release each shortened, so it grabs and lets go in the unit's lurching way;
the figures are the spec's and Warren's ear's.

**The stage after the gain cell.** One soft stage, odd-harmonic mostly, with a little even order
(the unit's transformer and FET): a gentle curve (the spec chooses: a tanh-like, or Tape's
anhysteretic curve on a short loop), measured with `as-envelope`: THD at −18 dBFS below 0.3%,
rising smoothly to a few percent at 0 dBFS; aliasing products below −80 dBFS at the reference,
and if they aren't, the stage runs at twice the rate internally with a fixed filter, and the
determinism rule still holds. The stage is part of the unit, not a setting.

**Static curve test.** A 1 kHz sine stepped from −60 to 0 dBFS in 1 dB steps, each held two
seconds; the steady-state output level against the input, compared with `curve()` within 0.1 dB
at each ratio. Gain reduction never exceeds 40 dB.

### `opto`, the optical style (the LA-4's manner)

**Topology: feedback,** with the detector's "cell" the thing that makes it: a light driven by the
output, a photocell that reads it slowly and remembers.

| Control | Range (held to) | Default | Notes |
|---|---|---|---|
| Peak reduction | 0 to 100 | 50 | the threshold, from none (0) to about −40 dBFS (100), on a law the spec writes (the unit's pot is logarithmic) |
| Gain | 0 to +40 dB | +10 | output, ramped |
| Ratio | the LA-4's switch | 4 | the spec reads the unit's steps (2, 4, 8, 12 and 20 to 1 is the reading to check) and gives them; the LA-2A's single soft ratio is the 2:1 step with the wide knee |
| Meter | gain reduction, output | GR | which the panel shows |
| Bypass | | off | crossfaded |

**Attack** about 10 ms, fixed (a one-pole on the detector).

**Release in two parts, with memory.** A fast part of 40 to 80 ms that recovers about the first
half of the reduction, and a slow part of 0.5 to 5 s for the rest, slower the longer and deeper
the unit has been compressing. The spec writes it as three states: the fast envelope, the slow
envelope, and a memory that integrates gain reduction over the last few seconds (a one-pole of
about 2 s on the reduction in dB); the release coefficient of the slow part is read from the
memory (more memory, slower), and the detector's output is the larger of the two envelopes. The
figures are the spec's; the shape is the point.

**Knee.** Wide and soft, 10 to 20 dB, narrowing with the ratio switch as the FET's does, less.

**Tests.** The static curve as the FET's, within 0.3 dB (the memory makes the steady state slow to
reach; the test holds each step five seconds). The attack within 10% of 10 ms. The memory: after
a −6 dBFS tone held 0.1 s, the time back to 3 dB of reduction, against the same after 2 s held:
the second longer, by at least half again. On `as-envelope`'s speech-like corpus, gain
reduction's rate of change never exceeds a bound the spec sets (it doesn't pump). Gain
reduction never exceeds 40 dB.

### `deess`, the de-esser

**Topology: feed forward,** the detector on the input through a band-pass, so the reduction is
exact and the test can say so.

| Control | Range (held to) | Default | Notes |
|---|---|---|---|
| Frequency | 2 to 12 kHz, log | 6 kHz | the band's centre |
| Width | a third of an octave to two octaves | one octave | the band's width; the band-pass is second-order (or two, the spec decides) |
| Threshold | −60 to 0 dB | −30 | **relative:** the band's level against the whole signal's level, both detected the same way, so a quiet voice de-esses as a loud one does; the spec may add an absolute floor below which nothing acts |
| Range | 0 to 12 dB | 6 | the most reduction, ever |
| Mode | wide, split | split | wide turns the whole signal down; split turns only the band down |
| Listen | off, on | off | the output is the detector's band alone, so the ear finds the ess; never saved as on |
| Bypass | | off | crossfaded |

**Detector.** The band's rectified level and the whole's, each with a fast attack (0.2 to 1 ms, the
spec fixes one) and a release of 20 to 80 ms (fixed); the ratio of the two in dB against the
threshold through the gain computer at a high fixed ratio (10:1 or more), the reduction held to
Range.

**Split.** The signal is split into the band and the rest so that, at no reduction, the two sum to
the input within 1e-9: the spec decides between a complementary pair around the band (a second-
order Linkwitz-Riley high and low pass at the band's edges, phase-matched so they sum flat) and a
subtracted band (the input minus the band-pass's output, which sums exactly but changes phase
outside the band). The exactness test is the deciding one. In wide mode the gain goes on the
whole signal.

**Tests.** A synthetic ess: band noise centred at 6 kHz, an octave wide, at −20 dBFS over a
vowel-like tone (a 220 Hz tone with harmonics to 2 kHz at −12 dBFS, from `as-envelope`'s corpus
or made in the test): in split mode with Range 6 the band comes down by 6 dB within 0.3 dB and the
tone is unmoved within 0.1 dB; in wide mode the whole comes down by 6 within 0.3. A voice with no
ess (the tone alone) is unmoved within 0.01 dB. Split at no reduction nulls within 1e-9. Listen
outputs the band alone. The relative threshold: the ess at −20 over the tone at −12 and the same
pair 20 dB quieter are reduced the same within 0.3 dB. The timing: the reduction reaches 63% within
the attack set, and recovers within the release.

## Order of work, and what each step leaves behind

1. **`as-biquad`:** `peak_matched` and `shelf_matched`, with tests against the analog prototype,
   a `CHANGES.md` line. Nothing to do for the tools on `as-biquad`.
2. **`as-eq`:** the slots, the designs, the crossfade, `response`, the readouts, the tests above;
   README, `VENDORED.md` ("written here"), the workspace line.
3. **`as-dynamics`:** the gain computer and the test signals first (the static-curve test rig,
   the burst rig, the ess), then `deess`, then `opto`, then `fet` with its stage last. Each
   module lands with its tests; the crate's README says which modules are in.
4. **sonorack** takes each as it lands (`../Sonorack.md`); Warren's ears gate each unit there.

## For the spec session to decide (not Warren)

- The ramp times (5 and 10 ms are starting points) and the anti-denormal method.
- The detector's domain (linear or dB) for the FET, and the knee table by ratio.
- The all-buttons figures.
- The FET's stage: which curve, and whether it needs the internal double rate.
- The optical's threshold law from the 0 to 100 pot, the ratio steps as read from the unit, and the
  memory's figures.
- The de-esser's split method, its band-pass order, and whether an absolute floor is kept.
- Whether the EQ's end slots are fixed kinds.
- Whether the side-chain high-pass stays.
- The crates' public names for the structs and functions, in the suite's plain voice.

## For Warren

Nothing until there's sound: each unit's first gate is in sonorack, by ear, and the spec session
will write what to listen for when it hands over. Two things worth a word before the spec, if he
has one: whether the FET's dial figures should read as the unit's 1 to 7 on the panel or in
microseconds and milliseconds (recommended: the unit's dial, with the figure under it); and
whether 12 dB is enough Range for the de-esser on his voices (recommended: yes, and raise it if
his ear says).
