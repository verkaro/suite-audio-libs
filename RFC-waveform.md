# RFC: a waveform to place a mark by

From the family's session, 2026-11-11, at Warren's asking (`../NOTES.md`, "waveforms and scrubbed
audio"). Read from the code, with nothing run and nothing moved. It spans both suites: the
picture's numbers here, the view in `suite-light-panel` and `as-page-kit`.

**What it's for (Warren, 2026-11-11):** a visual guide for fine work, not scrubbing. Dropping or
moving a mark across a waveform, setting an IR's trim, seeing an IR. Dexter's `f` mode moves a
mark by ear, a step at a time, and that's trial and error. A picture of where the quiet is would
cut the presses down a great deal. That implies a window: a part of a longer clip, seen close,
with the rest out of view.

So the keys stay as they are. `h` `l` step and play, the digits set the step, and Space plays it
again. The picture puts the first press near the right place, and the ear still judges. Nothing
plays from the picture.

## What's there

| Where | What | Its limits |
|---|---|---|
| irma, `crates/core/src/wave.rs` | `picture(channels, from, to, columns, onset)`: the lowest and highest sample of each column, every channel together, and an IR's decay. Its doc says it's to move into the suite with the panel's part. | Samples in memory. An IR is short, so that's fine. A take isn't. |
| irma's protocol | `view` (`from`, `to`, `columns` up to 4000) answered with `wave` (`lo`, `hi`, the span, `decay`). Zooming is asking for another span. | irma's names, in irma's protocol |
| `lp-paint`, `wave.rs` | `Wave` and `Pen::wave`: `lo` and `hi` a column, straight or in dB to a floor, the times along the bottom, `Marker`s, a curve, and `time_at` from a click back to a time | One span, with no overview and no sense of what's beyond it |
| Tape | Its own waveform, on its panel (`crates/panel/src/draw.rs`) and its page, with a region chosen on it | Its own code. It could move onto the shared part later. |
| `as-page-kit` | Nothing | The page has no waveform |
| Dexter | TRIM and `f` by ear (`dexter/docs/trim.md`). Realtime scrubbing was proposed and not chosen, 2026-09-28. | No picture. Takes run from minutes to hours, 16 to 24 tracks. |

## The proposal

**w1, `as-wave` (here): the picture's numbers.** irma's `picture` moves in, with the decay kept
as an option that only an IR asks for. Added beside it:
- **From a file, by span.** A span read from a WAV or a FLAC a block at a time, through `as-wav`
  and `as-flac`'s `Frames`. FLAC seeking is the settled RFC in `dexter-irma`, which this waits on
  for FLAC only. Never the whole file for a close window.
- **An overview** of a whole file at a coarse resolution (a few thousand columns), for the strip
  that shows where the window is. For a long take it's kept as a new file, a cache (question 2),
  never beside the source and never changing it.
- **The message's shape,** written down once: `view` (`from`, `to`, `columns`) answered with
  `wave` (`lo`, `hi`, `from`, `to`, and `whole` for the overview). Each tool's protocol uses these
  names, as irma's already does. Each tool still documents them in its own `page-protocol.md`.

**w2, the view (`suite-light-panel`, then `as-page-kit`).** `lp-paint`'s `Wave` gains:
- **An overview strip** under the window: the whole file faint, the window's place lit, as `List`
  marks where a long list is.
- **A window that follows the mark.** It's centred on the mark when it opens, and scrolls as
  little as keeps the mark in view as it moves, as `List` keeps the chosen row in view.
- **The window running past the ends.** It's drawn as empty there, so a mark near the start or
  end isn't pinned to the frame's edge.
- **The quiet made plain.** The dB picture with a floor, as `Wave` has. A tool picks dB or
  straight.

The page's half is a `wave` part in `as-page-kit` drawing the same thing from the same message,
held to the panel's by the same names, as the keys and the skins are.

**w3, irma first.** Its trims and its view of an IR move onto w1 and w2, with nothing new to
hear. It already draws `Wave` and asks by span, so the window is tried there before a recorder
depends on it. The command bar went the same way.

**After it:** foley's panel takes the same view, off hold (`../Foley.md`). Dexter takes it after
its step 5 (`../Dexter.md`, ask 9): TRIM and moving a mark draw the window while stopped. Marks
dropped during a take stay a reflex, and nothing is added to the recording path. The peaks for a
span are asked of Dexter's server, off JACK's thread, a page-protocol change that the page, the
remote and dexter-kms follow.

## What proves it

- w1: `picture` gives irma's numbers bit for bit, on irma's tests moved here. A span read from a
  file matches the same span from memory, at any block size. An overview is never written beside
  its source, and a cache file is created exclusively and can be deleted with nothing lost.
- w2: snapshots of the window at a mark, at each end, and past an end, with the overview strip,
  drawn by `lp-panel`'s `parts` example. The page's half is tested in the headless browser against
  the same numbers.
- w3: irma's own tests and snapshots unchanged but for the part they draw through. Its panel is
  tried on a real screen by Warren, which is his gate.

## Questions for Warren

Each has a recommendation, which the work goes on by once he gives the go-ahead.

1. **How it zooms.** Recommended: **the window follows the step.** The digits that set the nudge's
   step also set the window's width, say forty steps across. Fine steps give a close picture and
   coarse steps a wide one, with no new keys. Free keys are scarce: `-` and `=` are mute and solo,
   and `[` `]` are taken. A tool may add a zoom of its own later, as a `differs`.
2. **Where a long file's overview is kept.** Recommended: **a cache, in the tool's own state
   folder,** keyed by the file's path, size and time, made the first time the file is looked at,
   and deletable with nothing lost (as irma's `index.json`). Dexter could write it as it records,
   since its meters already see every block, but that touches the recording path, so not at first.
3. **The page's half too?** Recommended: **yes, in w2.** Dexter's TRIM is on its page and in
   dexter-kms alike, so a panel-only view leaves Dexter half done.
4. **The order.** Recommended: **w1 and w2 in a suite session after s1 and s2, ahead of `as-eq`.**
   It serves irma, foley, Dexter and Tape, and Dexter's everyday marking. Dexter can't take it
   before its step 5 in any case. w3 follows in irma's own session.
