# RFC: the FET compressor, `as-dynamics`'s `fet`

From sonorack's session, 2026-11-12, at Warren's asking: the FET compressor's shape, written the way
`RFC-eq.md`, `RFC-deess.md` and `RFC-opto.md` write theirs, for a spec session to build from. **It
replaces the "`fet`, the FET style" section of `RFC-eq-dynamics.md`** (2026-11-11), which stays as
the record. What that RFC says all units share still holds: `f64`, per sample with no look-ahead,
the same to the bit at any block size, settings by value and held to range, ramps, one detector for
two channels, readouts copied off the thread, and `as-dynamics`' shared gain computer. Nothing here
is code, and no figure is final until the spec's tests pin it.

Settled before this (`RFC-shaping.md`, Warren, 2026-11-11): the 1176's manner, as "style", never
"model"; its saturation stage built last, after the acceptance tests for it exist.

**Warren has a Klark Teknik KT76** at the rig (2026-11-12), a hardware unit in the 1176's manner.
So, as the optical has its LA-4, the FET has two voicings on one switch (below, "The KT76
voicing"): the rack's own, and one with the KT76's face and sound, measured from his unit. Both stay
"style": the KT76 is itself a unit in the 1176's manner, and the voicing is fitted to one of them,
his, and claims no more than that.

## What it's for

Catching and shaping. The FET is the fast one: it can grab a peak in a fraction of a millisecond,
and it has a sound of its own when it does, forward and a little gritty. On a voice it's the last
word on peaks after the optical has levelled it; on a hit (a door, a punch, a footstep) it shapes the
front and the tail, and it's how a hit is made to sound bigger than it was. It's last in the rack
(`Sonorack.md`), so the de-esser and the optical have done their work before it hears anything.
Warren's gates: a hit, and a voice.

**What a good one does, and what a bad one does.** Done well, a peak is held without being heard
held, and a hit gets a harder front or a longer body, as the knobs ask. Done badly it crackles on
a voice (the gain moving within a single low wave), it flattens a hit until it's dead, or it adds
an edge to everything whether it's compressing or not. The first two are what the knobs are for:
the unit does them when asked, and the panel says how far it's pushed. The third is the stage's
figures, below.

## The manner

What makes a FET compressor sound the way it does:

- **Fast.** Attack from about 20 µs to 800 µs, release from about 50 ms to 1.1 s, the 1176's
  ranges as remembered (the spec checks them). Fast enough that, at its quickest, the gain follows
  the waveform of a low note and colours it. That's kept, not hidden: it's what the unit does at
  those settings, and the panel's dial says how fast it's set.
- **Input against a fixed threshold.** There's no threshold knob. Turning Input up pushes more of
  the signal over a fixed threshold, so it compresses more; Output sets the level after. That's how
  the unit is used, and the rack keeps it.
- **Feedback.** The detector listens to the output, one sample late, `y[n] = x[n] · g[n]` with
  `g[n]` from the detector after seeing `y[n−1]` (the 1176's topology as remembered; the spec
  checks it). The ratio and the times then shift a little with the material, as the unit's do.
- **The ratio buttons:** 4, 8, 12 and 20 to 1, and **all buttons in**, the 1176's famous misuse:
  the ratio near 20 to 1, the knee wider, the threshold a few dB higher, the attack and release
  shorter, so it grabs and lets go in a lurch and the sound turns aggressive. Its figures are the
  spec's and Warren's ear's.
- **A colour of its own.** After the gain cell, one soft saturation stage: almost nothing at
  ordinary levels, growing smoothly as it's driven and **as the gain reduction deepens** (a FET
  distorts more the harder it's working). It's part of the unit, always in while the unit is in,
  not a setting.

**No side-chain high-pass.** The earlier RFC left one to the spec; it's dropped here. The unit has
none, and the EQ's high-pass ahead of it in the rack already takes the lows that would make it pump.

## The controls

| Control | Range (held to) | Default | Fine step | Coarse step |
|---|---|---|---|---|
| Input | 0 to +40 dB | 0 dB | 0.5 dB | 3 dB |
| Output | −20 to +20 dB | 0 dB | 0.5 dB | 3 dB |
| Attack | dial 1 to 7 (800 µs to 20 µs) | 3 | 0.1 | 1 |
| Release | dial 1 to 7 (1.1 s to 50 ms) | 5 | 0.1 | 1 |
| Ratio | 4, 8, 12, 20, ALL | 4 | one position | — |
| In | in, out | out (dark in the rack) | — | — |

**The dials read as the unit's 1 to 7, slowest to fastest, with the time beside them**
(Question 2, settled): `ATK 3 (330 µs)`, `REL 5 (180 ms)`. The law from dial to seconds is log, the spec
writes it and checks it against the unit's figures; the dial's numbers are the controls' format, so
typing `3` sets the dial and typing `330us` or `0.33ms` sets the time and shows the dial it lands on.

**Input and Output default to 0 dB** (Question 3, settled), with the threshold at about −18 dBFS at Input 0
(the spec sets it). So switching the unit in with a voice below the threshold changes only its
colour, and above it only makes it quieter, never louder. The earlier +20 default on Input is
dropped for that reason. Once Warren turns Input up, the unit gets louder and compresses more at
once, as the real one does, and Output brings it back.

All controls are described as data, as every unit's are, from the same module. The detents: Input
and Output at 0 dB, Attack at 3, Release at 5, Ratio at 4.

## The stage

One soft curve after the gain cell, odd harmonics mostly with a little even (the earlier RFC's
reading of the unit, which the spec checks), its drive rising with the level into it and with the
gain reduction. The spec chooses the curve (a tanh-like, or Tape's anhysteretic curve on a short
loop) and how the reduction feeds it. What it must meet, measured with `as-envelope`:

- THD below 0.3% at −18 dBFS with no reduction, rising smoothly to a few percent at 0 dBFS;
- at the same output level, more THD with 10 dB of reduction than with none;
- aliasing products below −80 dBFS at the reference. If they aren't, the stage runs at twice the
  rate inside the unit with fixed filters, **minimum phase, adding under 0.5 ms of latency**, since
  the rack is on a monitoring path. Deterministic either way.

Because the stage is always in while the unit is in, the FET doesn't pass a voice below its
threshold to the bit, as the optical does. **The unit out does,** exactly.

## Readouts, for a panel and for the keys

As the other units: in and out peak, and the gain reduction now in dB, as the needle shows. The
status line:

`FET  IN +18  OUT −6  ATK 3 (330 µs)  REL 5 (180 ms)  4:1  GR −6.0 dB`

`curve(level_db) -> out_db` draws the static curve without audio, at the Input and ratio set, with
the knee, and ALL drawn as it behaves.

## What "good" means, pinned by tests

Kept from `RFC-eq-dynamics.md`:

- **The static curve:** a 1 kHz sine stepped from −60 to 0 dBFS in 1 dB steps, each held two
  seconds; the output against the input matches `curve()` within 0.1 dB at each ratio, the stage
  measured out by comparing fundamentals.
- **The times:** on a 1 kHz burst from −30 to −6 dBFS and back, the attack to 63% and the release
  to 37% within 10% of the dial's time, at dials 1, 4 and 7, at a level stated in the test (the
  feedback shifts them with level).
- **The stage:** as above.
- The reduction never exceeds 40 dB.

New with this shape:

- **ALL is its own:** against 20 to 1, a wider knee, a higher threshold and shorter times, each by
  the spec's figures within 10%.
- **Switching in is never louder:** at the defaults, the corpus at any level comes out no louder
  than it went in, by more than the stage's own gain at that level (under 0.1 dB).
- **The unit out is the input to the bit.**
- **The dials:** typing a time lands on the nearest dial position the time allows and shows it;
  the dial's law reads the unit's figures at 1, 4 and 7 within 10%.
- **Linked:** as the optical's.
- **Held keys:** Input, Output and both dials changed every 2 ms for a second, the ratio stepped
  through every position including ALL: no sample-to-sample step beyond the bound, no broadband
  burst above −80 dBFS, the last setting in force at the end.
- **The controls:** as `RFC-eq.md`'s: steps return exactly, format then parse round-trips, out of
  range is held.
- Block sizes 1, 64 and 1,024 the same to the bit; a run twice the same; no non-finite sample on
  silence, DC, full-scale noise or a −300 dBFS tail.

## The KT76 voicing (Warren, 2026-11-12)

The same as the optical's LA-4 voicing (`RFC-opto.md`), and built on what that one builds:
`sonorack measure`, the loopback run first, the reduction tool, the acceptance against a committed
table. Only what differs is written here.

**Voicing**, a switch on the unit: **RACK** (the default, everything above) or **KT76**, which
changes the face and the figures, nothing else:

- **The face:** the KT76's own controls, named and marked as on its front panel, read off Warren's
  unit (Asked of Warren, 1). The dials are the KT76's markings, whatever they read.
- **The figures, measured from Warren's unit:** the threshold against Input, the knee and ratio of
  each button and of all buttons in (if his unit takes it), the attack and release at each dial
  position measured, the frequency response at rest and under reduction, and **the stage:** THD and
  the levels of the second to fifth harmonics against level with no reduction, and against the
  reduction at one level. The stage is where a FET is most itself, so it's measured most.

Each voicing keeps its own settings; flipping is a crossfade. As the optical's, settled 2026-11-12:
the unit's hiss is left out, and what's committed is the figures reduced from the captures, numbers
named as his unit's, with the captures his and never committed.

**What's measured, beyond the optical's list:** the stepped-level sine for the stage at three
reductions (none, 6 and 12 dB); the attack and release bursts at dials 1, 4 and 7 on each, and at
every ratio on one; ALL on its own. The spec keeps the session short and says how long up front;
with both units it's two sittings, or one long one, Warren's choice at the rig.

**The acceptance:** the KT76 voicing, on the same synthetic signals, reproduces the figures: the
static curve within 0.5 dB at each ratio, the times within 15%, the response at rest within 0.3 dB
from 30 Hz to 15 kHz, and **the harmonics within 2 dB of each measured one** at each level and
reduction measured, down to −90 dBFS, below which they're not compared.

**A/B by ear,** voicing against the KT76, is by patching in the bay, as the optical's is.

## Order of work

Last in `as-dynamics`, after the shared gain computer, `deess` and the optical's RACK voicing:

1. **RACK,** with its stage last of all, once `as-envelope`'s acceptance for it passes. sonorack
   takes it at its step 7 (`../Sonorack.md`). Warren's gates: a hit, then a voice, by ear,
   listening for the three faults (crackle on a voice at fast settings that weren't asked for, a hit
   flattened dead, an edge where nothing is compressing).
2. **Measuring the KT76,** with `sonorack measure` as the optical's step 2 built it. Warren's gate:
   at the rig. If the LA-4 hasn't been measured yet, both in the same sitting.
3. **The KT76 voicing,** fitted to the committed figures, its face from Warren's reading. Warren's
   gate: A/B against his unit, on a hit and a voice, at the same knobs.

## For the spec session to decide (not Warren)

- The 1176's ranges, dial law and topology, checked against its published figures.
- The threshold at Input 0, and the knee table by ratio.
- ALL's figures, with Warren's ear.
- The detector's domain (linear or dB), as the earlier RFC left it.
- The stage's curve, how the reduction drives it, and whether it needs the internal double rate.

## Asked of Warren

1. **The KT76's front panel, read off his unit:** each control's name and its markings, the ratio
   buttons, whether all buttons in is possible, the meter's buttons, anything else on the face. A
   typed list is enough, and needed only before the KT76 voicing's face is built.

## Questions for Warren

1. **An 1176 at the rig?** Answered (Warren, 2026-11-12): a Klark Teknik KT76. So the two
   voicings, as the optical's, above.
2. **The dials as the unit's 1 to 7,** with the time beside them? Recommended: yes, as the optical
   reads Reduce as its 0 to 100. The dial is how the unit is used; the time says what it means.
3. **Input and Output at 0 dB by default,** so switching it in is never louder? Recommended: yes,
   as with the optical's Gain.

**Settled (Warren, 2026-11-12):** 2 and 3 as recommended, and the KT76 voicing as written, with
the optical's rules carried over (the hiss left out, the figures committed, the captures his).
