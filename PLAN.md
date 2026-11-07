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

Development is local, with Claude Code on Warren's machine. Work is pushed to his soft-serve
on the LAN between milestones, and to GitHub at milestones. A tool names the suite by its
GitHub address and a pinned commit, so it builds as pushed. Locally, a `.cargo/config.toml`
kept out of git patches that address to the folder beside it:

```toml
[patch."https://github.com/verkaro/suite-audio-libs"]
as-wav = { path = "../suite-audio-libs/crates/wav" }
# one line for each crate the tool uses
```

A change to the suite is then live in the tool at once. At a milestone the suite is pushed
first, then the tool's pin is moved to that commit and the tool pushed.

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
   dexter-scape stays as it is (2026-10-30; not revisited).
3. **A clone builds with the network**, not with nothing else: cargo fetches the pinned suite.
   That's fine for public code.

## Where each tool stands (2026-11-07)

The suite's HEAD is `4131d73`. A pin behind it is fine: a tool moves its pin when it's ready.

| Tool | On the suite | Pin | Its own copies still |
|---|---|---|---|
| BRR | conv, draw, flac, http, ir, jack-keeper, keep, meter, opening, page-kit, resample, seam, tar, tilt, wav, ws | `72cba31` | none |
| dexter-tape | biquad, defects, envelope, flac, http, jack-keeper, keep, loudness, page-kit, provenance, resample, snapshot, wav, ws | `7643374` | none |
| dexter-ducky | bands, biquad, envelope, flac, http, keep, loudness, page-kit, provenance, speech, transport, wav, ws | `ae58a71` | none |
| Dexter | nothing yet | — | `crates/wav`, `crates/ws`, `crates/meter`, `crates/server/src/http.rs`; its page is page-kit's original |
| dexter-place | nothing yet | — | `wav`, `flac`, `ws`, `meter`, `jack` (the keeper), `server/src/http.rs`, `core`'s tilt |
| dexter-foley | nothing yet | — | `wav`, `flac`, `jack` (the keeper and the resampler), `server/src/http.rs` |
| dexter-scape | nothing (stays as it is) | — | `wav`, `ws`, `meter`, `jack`, `server/src/http.rs` |
| dexter-kms | on `suite-light-panel` (`f3ed49b`) | — | `src/ws.rs`, a WebSocket *client* (`as-ws` is the server's half) |

Each tool's `VENDORED.md` says where its copies came from and how they've drifted.

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

For Dexter, in this order: `wav` (the reference files are the gate: the suite's came from
Dexter's), `meter`, `ws`, then `http`. The page's script last, against `as-page-kit`, since
the page is what the others copied and its tests are in the browser. Dexter's own JACK client
is the engine's and isn't the keeper; it stays.

## Next for the suite

- **`as-ws`'s client half**, from dexter-kms's `src/ws.rs` (the handshake, masked frames out,
  fragments in, ping answered). Any second KMS panel needs it, and so would a tool that talks
  to the deck server.
