# The suite: a plan

Written 2026-10-30 from Warren's answer ("a public pinned suite for audio engineering plus
whatever you see as common denominators"), in BRR as `suite-plan.md`; moved here 2026-11-07,
to the suite's own repository, with where each tool stands and the order of moving in.

## What it is

One public repository (MIT) holding the libraries the Dexter family shares: audio
engineering, and the plumbing every sibling has written for itself. Each tool depends on it by
git, pinned to a commit. A fix goes into the suite, and each tool bumps its pin when it's
ready. Tools never patch the suite from their own repositories, as they never patch each other
now.

Nothing is in the suite that's only one tool's: no deck, no player, no set format.

Its sister is `../suite-light-panel` (`lp-kms`, `lp-keys`, `lp-paint`): a tool's front panel on
a whole screen through KMS, under the same rules.

## Working locally (Warren, 2026-10-30)

Development is local, with Claude Code, on whichever of Warren's computers he's at. GitHub is
the only remote (his soft-serve retired, 2026-11-07), so work is pushed at the end of a session
and `../family.sh pull` brings a computer up to date. A tool names the suite by its
GitHub address and a pinned commit, so it builds as pushed. Locally, a `.cargo/config.toml`
kept out of git patches that address to the folder beside it:

```toml
[patch."https://github.com/verkaro/suite-audio-libs"]
as-wav = { path = "../suite-audio-libs/crates/wav" }
# one line for each crate the tool uses
```

A change to the suite is then live in the tool at once. The suite is pushed first, then the
tool's pin is moved to that commit and the tool pushed. A change to an interface or a behaviour
gets a line in `CHANGES.md`, which a tool reads before moving its pin.

## The first crates, most-copied first

| Crate | From | What |
|---|---|---|
| `wav` | Dexter, three drifted copies | The crash-safe writer (S24, S16 dithered, F32), interleaved writing, the reader, the reference files |
| `flac` | place | The 24-bit encoder with no dependencies; decoding stays `claxon` |
| `ws` | Dexter, via the mixer, three drifted copies | The WebSocket with its own SHA-1 and base64 |
| `http` | each its own | The small HTTP/1.1 with no framework, uploads streamed |
| `meter` | three copies | The LED ladders' ballistics |
| `keep` | each its own | Nothing overwritten: exclusive numbered files, write-and-rename, a render's note |
| `jack-keeper` | scape, via place | Connections made and kept, reattaching to jackd, `--sim`'s clock, the dummy-server test harness |
| `page-kit` | four copies by hand | The skins, LEDs, keys, ladders and the OUT window, as one script the pages load |

All eight are here now. Then the DSP, as BRR and the others need it:

- `resample`: one HQ converter found in ~/Documents/Github/dexter-foley (here);
- `conv`: place's zero-latency convolution (here);
- `tilt` (here);
- `seam`: likeness, the laws, FIND (here);
- `noise`: the floor measured and matched;
- `ir`: an IR's numbers (here);
- `measure`: the sweep and the deconvolution;
- `onsets`: rigcheck's correlation;
- `draw`: seeded variation (here).

And `serve` (here, 2026-11-09, from dexter-irma): the server every tool with a page or a panel
writes, over `http` and `ws`. BRR, Tape and ducky hold their own until they move onto it.

Since then, from Tape and ducky: `loudness`, `biquad`, `snapshot`, `envelope`, `provenance`,
`defects`, `bands`, `speech`, `transport`; and `jack-capture`, `pitch`, `sfz`, `rawmidi`, `tar`,
`opening`. Where each came from is `VENDORED.md`.

## Settled

1. **The name** (2026-10-30): `verkaro/suite-audio-libs`, its crates `as-wav`, `as-ws` and so on.
2. **Moving in.** BRR was built on the suite from its first step, and Tape and ducky after it.
   **Dexter adopts the suite** (Warren, 2026-11-07). Dexter and place move over one crate at a
   time through their own sessions, each move a handoff, so neither is disturbed mid-work.
   **After Dexter: dexter-foley.** dexter-place and dexter-scape are held as they are, with no
   suite move, until it's decided whether they retire (Warren, 2026-11-11, `../CLAUDE.md`; this
   overturns 2026-11-07's place, foley, scape).
3. **A clone builds with the network**, not with nothing else: cargo fetches the pinned suite.
   That's fine for public code.
4. **Rooms get a tool of their own** (Warren, 2026-11-07): recording an IR and convolution are
   broken out of dexter-place into a new tool (its name to come). It's born on the suite, so
   place's `measure` crate and `numbers.rs` come into the suite first (`as-measure`, and the
   numbers into `as-ir`); `as-conv` is here already. What place keeps when they leave is for
   place's next session to settle with Warren.
5. **JACK and the patch bay are a tool of their own,** **jackbay** (Warren, 2026-11-07; named
   so, not `dexter-jack`, which is Dexter's own JACK crate): starting and stopping jackd, the
   rigs, finding the interfaces, and the bay. Dexter keeps its own, as JACK is part of the deck,
   but both run on the same suite crates. **Dexter makes them first and uses them itself**
   (`Dexter.md`, steps 7 to 10: `focusrite`, `jackd.rs`, the bay, the rig's JACK half), so
   there's never a second copy; jackbay starts once they're done. A Focusrite app of its own
   may follow on `as-focusrite`. No other tool controls JACK.
   - **`as-focusrite` came first** (Warren, 2026-11-10): moved in from Dexter for
     focusrite-ctrl, the Focusrite app, without Dexter moving onto it yet. Until Dexter's step
     7, Dexter's `crates/focusrite` is a second copy: a fix either needs goes here, and Dexter's
     copy follows at its step 7.
   - **The rest go first too** (Warren, 2026-11-10): the suite's half of steps 8 to 10
     (`as-jackd`, `as-bay`, the rig's JACK half) is moved in for jackbay ahead of Dexter, the
     same way, each proposed first. jackbay starts on them, not after Dexter's step 10.
     `../Jackbay.md` has the order.
   - jackbay has a panel, and it's the one muntin sends you to when JACK is down, so it runs
     without JACK: it's what starts it.
6. **KMS panels** (Warren, 2026-11-07): place and scape stay on their pages, with no KMS panel.
   BRR, foley, Tape, ducky, jackbay and the rooms tool are candidates for one: a panel a
   tool, run from one frame (see "Panels for the rest of the family" below).

## Where each tool stands

The numbers are never kept here (they were, until 2026-11-11, and were behind within days):
`../family.sh pins` prints each tool's pin on each suite, the suite's head and how far behind;
`../family.sh copies` lists the crates a tool still holds that a suite also has, by name; a
tool's `Cargo.toml` says which crates it's on. A pin behind the head is fine: a tool moves its pin
when it's ready, reading `CHANGES.md` on the way. What `pins` can't say:

- **BRR, Tape, ducky, irma, like110, focusrite-ctrl** were born on the suite and hold no copies.
- **Dexter** is on `meter` (step 1); `ws`, `wav` and `http` are in PRs (#112 to #114, its step 2
  to 4). Its `focusrite`, `jack`, `system` (the interfaces), `localtime`, `bay-core`, `bay-jack`,
  `patch`, `mcu` and `surface` are second copies of suite crates by the "suite's half first"
  decisions (2026-11-10 and 11), until its steps 7 to 11.
- **dexter-place** holds `wav`, `flac`, `ws`, `meter`, `jack` (the keeper), its server's `http.rs`,
  and in `core` `conv.rs`, `opening.rs`, `tilt.rs`, `convert.rs` (a resampler of its own); in
  `library` `ir.rs`, the onset in `numbers.rs`, and its save-and-rename (`as-keep`).
- **dexter-foley** holds `wav`, `flac`, `jack` (the keeper and the resampler), its server's
  `http.rs`, and `library/src/loudness.rs` (now `as-loudness`).
- **dexter-scape** holds `wav`, `ws`, `meter`, `jack`, its server's `http.rs`; in `core` `seam.rs`,
  `tilt.rs`, draw and mix (`as-draw`); `module/src/carry.rs` (a tar of its own, now `as-tar`).
- **dexter-kms** holds none: its WebSocket client, screen loop, snapshot harness, `Pen` and
  palette went into the suites on 2026-11-07 (its `VENDORED.md`).

Each tool's `VENDORED.md` says where its copies came from and how they've drifted. The rows for
place, foley and scape were checked against the suite's crates on 2026-11-07 by module and
import, not yet diffed.

## Moving a tool in

One crate a step, in the tool's own session (`claude --add-dir ../suite-audio-libs` from the
tool's folder), each step its own commit:

1. Diff the tool's copy against the suite's crate. What the copy has that the suite lacks goes
   into the suite first, with a test, as a suite commit.
2. Depend on the `as-` crate (pin plus the local patch), delete the copy, and say so in the
   tool's `VENDORED.md`.
3. The tool's own tests pass unchanged, and reference files match byte for byte (`BLESS=1` is
   never the way through a move).
4. Move this file's table.

For Dexter, in this order: `meter` (the code is the same apart from comments, so it proves the
pin, the patch and CI at no risk), `ws`, `wav` (the reference files are the gate: the suite's
came from Dexter's), then `http` (the server's upload changes). The page's script is a decision
for after, against `as-page-kit`. Dexter's own JACK client is the engine's, not the keeper, and
it stays. The path, step by step, with what proves each step: `Dexter.md`.

## Next for the suite

Found 2026-11-07, most wanted first. Each comes in through the tool that has it, in that tool's
move, never copied from one tool to another.

- **TUNE**, a sample read faster or slower through a windowed sinc: written twice, in scape's
  `core/src/tune.rs` and BRR's `core/src/play.rs`. One crate, and BRR moves onto it.
  (`as-resample` is for fixed ratios of whole-number rates; this isn't.)
- **FLAC tags** (Vorbis comments rewritten without risking the audio), from foley's
  `crates/tags`, into `as-flac`. Place asked for it, and Dexter's DATE_RECORDED on export
  needs it. **Here, 2026-11-09** (`as_flac::tags`), brought by dexter-irma.
- **WALK**, the distance law (quieter, duller, to the side), from scape's `core/src/walk.rs`.
  Place's `core/src/distance.rs` is a copy of it.
- **Measuring a room and its numbers**, from place's `measure` crate and `numbers.rs`, for the
  rooms tool (Settled, 4). **The numbers are here, 2026-11-09** (`as_ir::numbers`), brought by
  dexter-irma. Measuring stays in place for now (Warren, 2026-11-09).
- **JACK control, the rigs, the interfaces and the bay**, out of Dexter, for jackbay
  (Settled, 5): `Dexter.md`'s steps 7 to 10, after step 5, at Dexter's pace.
- **`noise`**, a recording's floor matched, from scape's `core/src/noise.rs`, when a second tool
  wants it.
- **The shaping units,** for sonorack first: `as-bench` (measuring hardware against a model,
  built first so the rig can start early), `as-control`, `as-biquad`'s matched designs, `as-eq`
  and `as-dynamics` (`deess`, `opto`, `fet`). Written here, not moved. Shaped by `RFC-shaping.md`
  and the four unit RFCs; specified in `specs/` (2026-11-12), read from `specs/shared.md`, eight
  steps (0 to 7), one a session, each naming its model. Step 0 waits only on Warren's word. The
  EQ's steps, 1 to 3, go ahead of s1, s2 and the waveform (Warren, 2026-11-12: nothing needs his
  ear until sonorack's panel); 4 to 7 still wait on them. **Steps done:** 0 (`as-bench`,
  2026-11-12), 1 (`as-control`, 2026-11-12).

Staying in their tools: foley's UCS table, place's positions and its opening played by hand,
and Tape's and ducky's engines.

## Panels for the rest of the family

**A panel a tool, and a frame that runs them** (Warren, 2026-11-07, over one client linking
every panel). Settled, 6 names the tools.

- **Each tool's panel** is its own binary, kept in the tool, as dexter-kms is for the deck,
  speaking the tool's page protocol (so each protocol's clients list grows by one).
- **The frame** is `muntin` (named 2026-11-07), its own repository on `suite-light-panel`'s
  crates, and stays running: a menu of the tools, each one's
  state (running or not, JACK's state). It starts a tool's panel and gives it the screen
  (dropping DRM master), and takes the screen back when the panel exits. A key every panel
  shares (in `lp-keys`) leaves the panel for the frame.
- **The shared face:** `lp-paint` drawing what `page-kit.js` draws, the same keys meaning the
  same things, the same skins, and the ws client (`as-ws`'s client half).
- **What a panel is built on** (moved out of dexter-kms, 2026-11-07, so Tape's panel, the
  second, starts on them): `as-ws::client` for its server; `lp-panel`'s `run` for the screen
  loop and `snapshot` for testing it without a screen; `lp_paint::Pen` and `lp_paint::dark`,
  page-kit's dark skin, for drawing.

Starting a tool when it's needed, as a sketch to settle when the frame is built:

- The frame reads what it knows of each tool from one file: its server's binary and port, its
  panel's binary, and whether it needs JACK. It asks the port whether the server is up.
- Choosing a tool that isn't running starts its server, headless, connected to nothing, then
  its panel once the server answers.
- A tool that needs JACK, with JACK down, opens jackbay's panel first. The frame itself
  never starts jackd (the baseline's rule).
- Leaving a panel leaves its tool running, so what's playing keeps playing. Stopping a tool is
  its own choice in the frame, asked first.

**The frame comes early** (Warren, 2026-11-07): it's small beside the tools, and built now it
stays tested as the family grows. Its first version runs the one panel there is, dexter-kms,
and starts the deck's server when it's down; the screen handed over and taken back is the part
to prove first, on a real screen (Warren's gate). Each new panel is added to it as it comes.

## The keys and the command bar (Warren, 2026-11-09)

What's wanted is in `../STYLE.md`: "Keys" (the common keys, and how they change) and "The command
bar". This is how it gets built, and in what order. Items 1 to 4 below are built, and BRR is on them
(2026-11-09, `661ffbf`). Item 5 is built (2026-11-10, `suite-light-panel` `3e35a39`), and no panel
is on it yet. Item 6 isn't.

### What's built, and where

1. **The table of common keys.** `KEYS` in `page-kit.js`, data only: each common key and what it
   means. It's not a key handler. It starts with the keys already settled, and grows as where mute,
   solo, loop, arm and record sit gets decided.
2. **Each tool's declared list.** A tool says what its keys are, once, as data:
   - `keys.json` in the tool, served to its page and included by its panel, so both read one list.
   - An entry has the key, what it does, the screen it's on, and `differs`, the reason, when it
     uses a common key for something else.
   - **`?` and the command bar draw from it,** which is what keeps it honest: a key left out shows
     up as help that's missing.
3. **The test.** `as_page_kit::check_keys`, for a tool's own tests: it fails on a common key used
   for something else with no `differs`, and on a page handling a key it hasn't declared. The second
   is a scan for the page's literal key comparisons, so it's a net with holes in it. A session that
   finds a better way says so.
4. **The bar.** `kitBar` in `page-kit.js`: the two uses (a yes or no question, a choice or command),
   greyed options, levels, Esc up a level, a note on a mark. It's drawn along the footer and takes
   every key while it's open.
   - A level comes from the screen first, so `key()` keeps its label beside its place and id in
     `HITS`, and `kitSection()` says the section it falls in (BRR, 2026-11-09: `label()` is also
     column heads and counters, so it can't be the section).
   - A tool adds levels of its own for what has no place on the screen.
5. **The panel's half,** in `suite-light-panel`: the table held by value and compared to
   `page-kit.js` by a test, as `lp_paint::dark` is, and the bar drawn with `Pen`. **Built
   2026-11-10,** brought by dexter-irma: `lp_keys::common` (the table, `check`, a key named as a
   page names it), `lp_panel::bar::Bar` and `lp_panel::keys::keys_over`. A panel reads its
   `keys.json` itself and hands the suite the entries. In place of the page's scan for key
   comparisons, a panel's test presses every key there is (`lp_keys::common::every`) and fails on
   one that does something and isn't listed.
6. **The terminal's half,** for `--tty`: the same bar on the bottom row, in ASCII.

Every change to the kit gets its line in `CHANGES.md`. A key that changes reaches a tool only when
its pin moves.

### The order

A session changes its own repository and the suites it depends on (`../CLAUDE.md`), so each piece
is built by the first tool that needs it.

| | Tool | Brings | Why then |
|---|---|---|---|
| 1 | BRR | The table, the declared list, the test and the bar (items 1 to 4); the note on a mark | On the kit, has vi's keys already, a time line to prove the note on, and its pace doesn't stop between steps. **Done 2026-11-09** (`../Brr.md`). |
| 2 | ducky | Takes them up, with marks in time from the start | Early, so a change is cheap. |
| 3 | dexter-irma | The panel's half (item 5), with its panel | Ahead of Tape (Warren, 2026-11-09): a new panel, with a list, a search and fields to prove the bar on. **Built 2026-11-10;** irma's panel takes it up next. |
| 4 | Tape | Takes up the panel's half | Has a panel, and its page has few keys of its own. |
| 5 | foley's panel | Takes up the panel's half | When Warren wants it. Needs 70s in `lp-paint` first (`../Foley.md`). |
| 6 | Dexter, then dexter-kms | The terminal's half (item 6) | Last: the recorder in use, one step at a time with Warren's go-ahead, and after its move onto the suite. |
| | place, scape | Nothing yet | Until `../NOTES.md`'s question of whether they stay is settled. |

**Warren's gates:** the bar and the keys on a real screen. The two drafted details in "The command
bar", ignoring a click just after a level opens and how many levels, are settled by trying them.
Both are still drafts, not yet Warren's decision: the first is in use on BRR, and the second is
guidance only, not enforced by the kit.
