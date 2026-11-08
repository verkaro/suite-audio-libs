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

Since then, from Tape and ducky: `loudness`, `biquad`, `snapshot`, `envelope`, `provenance`,
`defects`, `bands`, `speech`, `transport`; and `jack-capture`, `pitch`, `sfz`, `rawmidi`, `tar`,
`opening`. Where each came from is `VENDORED.md`.

## Settled

1. **The name** (2026-10-30): `verkaro/suite-audio-libs`, its crates `as-wav`, `as-ws` and so on.
2. **Moving in.** BRR was built on the suite from its first step, and Tape and ducky after it.
   **Dexter adopts the suite** (Warren, 2026-11-07). Dexter and place move over one crate at a
   time through their own sessions, each move a handoff, so neither is disturbed mid-work.
   **After Dexter: dexter-place, dexter-foley, then dexter-scape** (Warren, 2026-11-07; scape is
   still being worked on, so it moves too, overturning 2026-10-30's "stays as it is").
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
   - jackbay has a panel, and it's the one muntin sends you to when JACK is down, so it runs
     without JACK: it's what starts it.
6. **KMS panels** (Warren, 2026-11-07): place and scape stay on their pages, with no KMS panel.
   BRR, foley, Tape, ducky, jackbay and the rooms tool are candidates for one: a panel a
   tool, run from one frame (see "Panels for the rest of the family" below).

## Where each tool stands (2026-11-07)

The suite's HEAD is `4131d73`. A pin behind it is fine: a tool moves its pin when it's ready.

| Tool | On the suite | Pin | Its own copies still |
|---|---|---|---|
| BRR | conv, draw, flac, http, ir, jack-keeper, keep, meter, opening, page-kit, resample, seam, tar, tilt, wav, ws | `72cba31` | none |
| dexter-tape | biquad, defects, envelope, flac, http, jack-keeper, keep, loudness, page-kit, provenance, resample, snapshot, wav, ws | `7643374` | none |
| dexter-ducky | bands, biquad, envelope, flac, http, keep, loudness, page-kit, provenance, speech, transport, wav, ws | `ae58a71` | none |
| Dexter | http, meter, wav, ws | `ebe1e3e` | none; its page is page-kit's original |
| dexter-place | nothing yet (after Dexter) | — | `wav`, `flac`, `ws`, `meter`, `jack` (the keeper), `server/src/http.rs`; in `core`: `conv.rs`, `opening.rs`, `tilt.rs`, `convert.rs` (a resampler of its own); in `library`: `ir.rs` and the onset in `numbers.rs`, and its save-and-rename (`as-keep`) |
| dexter-foley | nothing yet (after place) | — | `wav`, `flac`, `jack` (the keeper and the resampler), `server/src/http.rs`, `library/src/loudness.rs` (BS.1770-4, now `as-loudness`) |
| dexter-scape | nothing yet (after foley) | — | `wav`, `ws`, `meter`, `jack`, `server/src/http.rs`; in `core`: `seam.rs`, `tilt.rs`, draw and mix (`as-draw`); `module/src/carry.rs` (a tar of its own, now `as-tar`) |
| dexter-kms | on `suite-light-panel` (`f3ed49b`) | — | `src/ws.rs`, a WebSocket *client* (`as-ws` is the server's half) |

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

- **`as-ws`'s client half**, from dexter-kms's `src/ws.rs` (the handshake, masked frames out,
  fragments in, ping answered). Every KMS panel after the deck's needs it (Settled, 6), and so
  would a tool that talks to the deck server.
- **TUNE**, a sample read faster or slower through a windowed sinc: written twice, in scape's
  `core/src/tune.rs` and BRR's `core/src/play.rs`. One crate, and BRR moves onto it.
  (`as-resample` is for fixed ratios of whole-number rates; this isn't.)
- **FLAC tags** (Vorbis comments rewritten without risking the audio), from foley's
  `crates/tags`, into `as-flac`. Place asked for it, and Dexter's DATE_RECORDED on export
  needs it.
- **WALK**, the distance law (quieter, duller, to the side), from scape's `core/src/walk.rs`.
  Place's `core/src/distance.rs` is a copy of it.
- **Measuring a room and its numbers**, from place's `measure` crate and `numbers.rs`, for the
  rooms tool (Settled, 4).
- **JACK control, the rigs, the interfaces and the bay**, out of Dexter, for jackbay
  (Settled, 5): `Dexter.md`'s steps 7 to 10, after step 5, at Dexter's pace.
- **`noise`**, a recording's floor matched, from scape's `core/src/noise.rs`, when a second tool
  wants it.

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
