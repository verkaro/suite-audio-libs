# RFC: the optical compressor, `as-dynamics`'s `opto`

From sonorack's session, 2026-11-12, at Warren's asking: the optical compressor's shape, written
the way `RFC-eq.md` and `RFC-deess.md` write theirs, for a spec session to build from. **It
replaces the "`opto`, the optical style" section of `RFC-eq-dynamics.md`** (2026-11-11), which
stays as the record. What that RFC says all units share still holds: `f64`, per sample with no
look-ahead, the same to the bit at any block size, settings by value and held to range, ramps, one
detector for two channels, readouts copied off the thread, and `as-dynamics`' shared gain computer.
Nothing here is code, and no figure is final until the spec's tests pin it.

Settled before this (`RFC-shaping.md`, Warren, 2026-11-11): the LA-4's manner, as "style", never
"model", with the LA-2A's single ratio reachable by the ratio switch.

**Warren owns an LA-4,** in good condition and sound (2026-11-12). `RFC-shaping.md` said a model
couldn't be proved because there was no unit on the bench; for this compressor there now is. So
the unit has two voicings on one switch (below, "The LA-4 voicing"): the rack's own, as this RFC
first wrote it, and one with the LA-4's face and its sound, measured from Warren's unit. It stays
"style": it's fitted to one unit, his, and claims no more than that.

## What it's for

Dialogue. An optical compressor is the one engineers reach for to sit a voice level without
hearing it work: it grabs gently, lets go in a way that follows the speech, and holds back longer
after a long loud passage than after a single loud word. In the rack it comes after the de-esser and
before the FET (`Sonorack.md`), so it does the levelling and the FET, if lit, catches what's left.
Warren's gate for it is dialogue.

**What a good one does, and what a bad one does.** Done well, a voice that moves from a murmur to a
raised line comes out nearer one level, the words' fronts keep their shape, and the room tone
between lines doesn't swell up. Done badly it pumps (the level jumps back between words, the room
breathing in and out), it flattens the fronts of words so the voice sounds squashed, or it lets go
so slowly that a quiet line after a shout is buried. Every choice below is made against those three.

## The manner

What makes an optical sound the way it does is the cell: a light driven by the signal, shining on
a photoresistor that reads it slowly and remembers. The rest of the unit is a plain, clean gain
stage. So the cell is where the character lives here, in four behaviours:

- **A soft grab.** The attack is about 10 ms, slow enough that the front of a word passes before
  the cell closes, so consonants keep their bite. And it's **faster the harder it's pushed:** a big
  overshoot lights the cell brighter, and it reacts sooner. A small one is gentler still.
- **A release in two parts.** About half the reduction comes back fast, in 40 to 80 ms, so the gap
  between words isn't held down; the rest comes back slowly, over half a second to a few seconds,
  so the level doesn't jump up between phrases. That slow tail is what stops it pumping.
- **Memory.** The longer and deeper the cell has been held down, the slower that slow part gets. A
  single loud word lets go quickly; a long shouted passage lets go over seconds, as a voice settling
  down would. This is the behaviour the ear knows as "optical".
- **A soft knee.** No point where compression switches on: wide and rounded, 10 to 20 dB, narrowing
  a little at higher ratios.

**Clean.** No saturation and no tube colour. The LA-4 is a solid-state unit; its sound is the cell.
Colour in the rack is the FET's stage, as `RFC-eq.md` settled for the EQ.

**Feedback,** as before: the detector listens to the output, one sample late, `y[n] = x[n] · g[n]`
with `g[n]` from the cell's state after seeing `y[n−1]`. That's how the units are built, and part of
why a feedback compressor is gentle: it can never quite catch up with a peak, so it never clamps.

## The controls

| Control | Range (held to) | Default | Fine step | Coarse step |
|---|---|---|---|---|
| Reduce | 0 to 100 | 40 | 1 | 5 |
| Gain | −10 to +30 dB | 0 dB | 0.5 dB | 3 dB |
| Ratio | 2A, 2, 4, 8, 12, 20 | 4 | one position | — |
| In | in, out | out (dark in the rack) | — | — |

**Reduce** is how much it compresses: the threshold, from none at 0 to about −40 dBFS at 100, on a
log law the spec writes from the unit's pot. It reads as the unit's 0 to 100 on the panel and in
the status line, with the threshold in dBFS beside it (Question 2, settled): `REDUCE 40 (−22 dBFS)`. The
LA-4's own front panel is to be checked by the spec session (whether its compression is set by an
input knob into a fixed threshold, as the LA-4 is remembered, or by a peak-reduction knob, as on the
LA-2A); the control here is the same either way, a single amount, and its name is the panel's.

**Gain** is the make-up, after the cell, ramped. It **defaults to 0 dB** (Question 1, settled): lighting the
unit can only make the voice quieter or leave it, never louder, so nothing jumps out of the
monitors when it's switched in. The earlier +10 default is dropped. The range is the unit's in
spirit: plenty to make up 20 dB of reduction, and a little below unity for a voice that comes up
too hot.

**Ratio** is the LA-4's switch as remembered: 2, 4, 8, 12 and 20 to 1, to be checked by the spec.
Its **2A** position is the LA-2A's manner, as settled: a soft ratio of about 3 to 1 with the widest
knee, the cell's slowest attack, and the most memory. Each position has its own knee from the spec's
table, and only 2A changes the cell's times. Stepped by `h` `l` like any control; a change of ratio
glides through the cell, so it never clicks.

**No mix knob** for parallel compression (Question 3, settled). It's a DAW's habit, and the rack is not a
DAW. A voice that wants it is compressed less.

**No meter switch in the crate.** The unit's switch between gain reduction and output is a choice
of what to show, so it's the panel's.

All controls are described as data exactly as `RFC-eq.md` describes the EQ's, from the same module
the de-esser uses. The detents: Reduce's is 0 (no compression), Gain's is 0 dB, Ratio's is 4.

## The cell, written out

The spec writes it, as three states and a gain computer, from the four behaviours above:

1. **The level:** the output's rectified level, both channels the louder of the two, in dB.
2. **The target:** the gain computer (shared) gives the reduction the level asks for, at the
   threshold from Reduce, with the ratio's knee.
3. **The cell:** moves the reduction toward the target.
   - **Rising** (more reduction): a one-pole whose time is about 10 ms at 6 dB of overshoot,
     faster for more and slower for less, held between 3 and 30 ms. The law is the spec's; it
     depends on the overshoot only.
   - **Falling:** two envelopes. The fast one, 40 to 80 ms, carries about half the reduction; the
     slow one carries the rest, with a time read from the memory. The reduction is the larger of
     the two, so the slow tail shows once the fast part has gone.
4. **The memory:** a slow one-pole, about 2 s, on the reduction in dB. Its value sets the slow
   envelope's time, from 0.5 s with no memory to 5 s at its most. The law is the spec's.

When nothing has reached the threshold (below the knee), the target is exactly 0 dB, the states stay
at exactly 0, and the gain is exactly 1. So a voice below the threshold, and the unit out, pass to
the bit (a test below). The anti-denormal method is `RFC-eq-dynamics.md`'s, applied only where a
state is decaying, so it can't spoil that.

## Readouts, for a panel and for the keys

As the other units: in and out peak, and the gain reduction now in dB, as the needle shows it. And
one more: **how held the cell is,** the memory from 0 to 1, so the panel can show the release slowing
after a long loud passage, and the story's "watches its meter breathe" has something behind it. The
status line says it in words:

`OPTO  REDUCE 40 (−22 dBFS)  4:1  GR −5.5 dB  HELD 0.6  GAIN +4.0`

`curve(level_db) -> out_db` draws the static curve without audio, with the knee, for the ratio set.

## What "good" means, pinned by tests

Kept from `RFC-eq-dynamics.md`:

- **The static curve:** a 1 kHz sine stepped from −60 to 0 dBFS in 1 dB steps, each held five
  seconds; the output against the input matches `curve()` within 0.3 dB at every ratio.
- **The attack:** 63% of the final reduction within 10% of 10 ms, on a 1 kHz burst stepping to
  6 dB over the threshold.
- **The memory:** after a −6 dBFS tone held 0.1 s, the time back to 3 dB of reduction, against the
  same after 2 s held; the second at least half as long again.
- **No pumping:** on `as-envelope`'s speech-like corpus, the rate of change of the reduction stays
  under the spec's bound.
- The reduction never exceeds 40 dB.

New with this shape:

- **Faster when pushed harder:** the attack to 63% at 12 dB of overshoot is shorter than at 3 dB.
- **Two-part release:** after a held tone, half the reduction is back within 80 ms, and the rest
  takes at least 0.5 s.
- **Below the threshold is the input to the bit:** the corpus 30 dB under the threshold, Gain at
  0 dB, passes bit-exact; so does the unit out.
- **2A:** its knee is the widest and its memory the most, against the other positions.
- **Linked:** a loud left and silent right get the same gain on both channels, and a source on
  the right only is compressed as the same source on the left.
- **Held keys:** Reduce and Gain changed every 2 ms for a second, and the ratio stepped through
  every position: no sample-to-sample step beyond the bound, no broadband burst above −80 dBFS, the
  last setting in force at the end.
- **The readouts:** the memory reads 0 after silence and rises with time held.
- **The controls:** as `RFC-eq.md`'s: steps return exactly, format then parse round-trips, out of
  range is held.
- Block sizes 1, 64 and 1,024 the same to the bit; a run twice the same; no non-finite sample on
  silence, DC, full-scale noise or a −300 dBFS tail.

## The LA-4 voicing (Warren, 2026-11-12)

At Warren's asking: a switch that flips the unit to an LA-4 style face and sound, since his own
LA-4 is the one comparison with the real thing there is.

**Voicing**, a switch on the unit: **RACK** or **LA-4**. RACK is the default, and is everything
above. LA-4 changes two things, and only these:

- **The face.** The controls become the LA-4's own, named and marked as on its front panel, so a
  setting on the panel and a knob position on the unit can be set the same and compared. Warren
  reads the front panel off his unit for the spec session: each control's name and its scale
  markings, the ratio switch's positions, the meter's switch (Asked of Warren, 1). The controls are
  described as data as every other control is, with the unit's markings as their format, so the keys
  and the FaderPort work the same.
- **The sound.** The cell's figures (the threshold law from the input or reduction knob, the knee
  and ratio of each switch position, the attack against overshoot, the two releases, the memory)
  and whatever else measuring finds that the ear would hear: the frequency response at rest and while
  compressing, and any distortion, as a fixed stage like the FET's, only if the unit has one worth
  keeping. All of it is **measured from Warren's unit,** not taken from memory or a datasheet.

Everything else stays the rack's: the cell's structure, per sample, feedback, linked, bit for bit at
any block size, the readouts, the tests' kinds. Only the numbers differ, so the LA-4 voicing is a
second table of figures and a second control description, not a second compressor.

**Each voicing keeps its own settings.** Flipping doesn't translate one face's knobs into the
other's: they don't map exactly, and a guess would be a setting nobody chose. Flipping is a
crossfade, as bypass is.

**The unit's hiss is left out** (Question 4, settled): its noise is a recording's to carry, not a
process's, and a rack that adds hiss on every pass can't be bit-exact below the threshold.

**The LA-2A's position.** RACK keeps its 2A position, as settled. The LA-4 voicing has exactly the
switch Warren's unit has, nothing added.

### Measuring Warren's LA-4

Measuring real hardware is Warren's gate, by the family's rules: the session that builds this
writes the method and the tools, and stops; Warren runs it, at the rig.

- **What's played:** `as-envelope`'s synthetic signals only: the stepped sine of the static-curve
  test, the bursts of the attack and release tests, the memory's 0.1 s and 2 s tones, a sine sweep
  at rest and under steady reduction, and a sine at stepped levels for distortion.
- **How:** sonorack plays them from a measuring port and records the return (`sonorack measure`, a
  mode the spec writes), on Warren's own JACK server, through his interface, out to the LA-4 and
  back. Its ports connect nowhere until he patches them in the bay, as always. **A loopback run
  first,** the interface's out to its in with no LA-4, so the interface's own response and latency
  are taken out of every figure.
- **At what settings:** each ratio position at one input setting, and the threshold law at a few
  knob positions on one ratio; the spec writes the list and keeps the session short (an hour at the
  rig is the aim, said up front). Warren sets the knobs by hand; the tool asks for each position in
  the command bar and waits.
- **What's kept:** the captures are Warren's, written new under his own directory, never committed.
  What's committed is **the figures reduced from them,** numbers in a small table, dated and named
  as his unit's (Question 5, settled). The reduction (fitting the cell's figures to the captures) is offline,
  in a tool in `as-envelope` or `as-dynamics`' tests' support, never in the realtime path.
- **The acceptance:** the LA-4 voicing, fed the same synthetic signals, reproduces the figures
  measured from the unit: the static curve within 0.5 dB at each ratio, the attack and the releases
  within 15%, the memory's ratio within 15%, the response at rest within 0.3 dB from 30 Hz to
  15 kHz. A test pins it against the committed table, with no audio.

**A/B by ear,** voicing against the real unit, is done by patching: the same voice through the
rack and through the LA-4 on the interface's insert, both to the monitors, set to the same knobs.
That's in the bay, in jackbay or Dexter's, never inside the rack, which has no hardware insert.

## Order of work

In `as-dynamics`, after the shared gain computer and `deess`, before `fet`:

1. **RACK,** as above, with its tests. sonorack takes it at its step 6 (`../Sonorack.md`).
   Warren's gate: dialogue, by ear, listening for the three faults (pumping, squashed fronts, a
   quiet line buried after a loud one), with the memory readout on the panel.
2. **`sonorack measure`** and the reduction tool. Warren's gate: the loopback, then his LA-4, at the
   rig.
3. **The LA-4 voicing,** fitted to the committed figures, its face from Warren's reading of the
   panel. Warren's gate: A/B against his unit, by ear, on dialogue, at the same knobs.

The FET doesn't wait on 2 and 3.

## For the spec session to decide (not Warren)

- The RACK voicing's ratio steps and the 2A position's figures (the LA-4 voicing's come from the
  measuring).
- The measuring list, its signals' levels and durations, and the fitting method.
- Reduce's law from 0 to 100.
- The attack's law against overshoot, the fast and slow release times, the memory's time and its
  law onto the slow release.
- The knee table by ratio.
- The bound for "no pumping".

## Asked of Warren

1. **The LA-4's front panel, read off his unit:** each control's name and its scale markings, the
   ratio switch's positions, the meter switch's positions, and anything else on the face (a link
   switch, a second channel). A typed list is enough. Needed before the LA-4 voicing's face is built,
   not before RACK.

## Questions for Warren

1. **Gain defaults to 0 dB,** so switching the unit in never makes the voice louder? Recommended:
   yes. The earlier default was +10, which with little compression would jump out of the monitors.
2. **Reduce reads as the unit's 0 to 100, with the threshold in dBFS beside it,** on the panel and
   in the status line? Recommended: yes. The 0 to 100 is how the unit is used; the dBFS tells you
   where the voice will start being compressed.
3. **No mix knob** for parallel compression? Recommended: no knob. It's a DAW's habit; a voice that
   wants less is given less.

**Settled (Warren, 2026-11-12):** 1 to 3 as recommended, and the LA-4 voicing added at his asking.

4. **The LA-4's hiss left out** of its voicing? Recommended: yes. Noise belongs to a recording, and
   a rack that adds it on every pass can't pass a quiet voice untouched.
5. **The measured figures committed** to the suite, as numbers only, named as his unit's, with the
   captures kept his and never committed? Recommended: yes. The figures are what the acceptance
   test needs, and they hold none of his audio.

**Settled (Warren, 2026-11-12):** 4 and 5 as recommended. The hiss is left out; the measured
figures are committed as numbers named as his unit's, and the captures stay his.
