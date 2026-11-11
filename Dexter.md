# Dexter onto the suite: the path

For Warren, written 2026-11-07 after Dexter's adoption was decided. A path to follow in order:
a step isn't started until the one before is done. Each step says what to do, what proves it,
and what's yours to check by hand. Each session's opening prompt is ready to paste.

What was found when this was written (Dexter at `c7ebba3`, the suite at `a176062`):

| Dexter's copy | The suite's | How far apart |
|---|---|---|
| `crates/meter` (`dexter-meter`) | `as-meter` | Comments only. The code is the same. |
| `crates/ws` (`dexter-ws`) | `as-ws` | Comments only. Dexter has a test the suite lacks: `tests/echo.rs`. |
| `crates/wav` (`dexter-wav`) | `as-wav` | The suite's is a superset: channels, `f64`, `bext`, chunks kept. Every Dexter test is in the suite's 22, and the four reference files match byte for byte. Its insides were rewritten for channels, so speed is worth a check. Bless is `BLESS=1`, not `DEXTER_BLESS=1`. Used by 7 of Dexter's crates. |
| `crates/server/src/http.rs` | `as-http` | One real difference. Dexter's `read_request` leaves `/api/upload`'s body unread (`STREAMED`) for the upload to stream, and caps other bodies at 4 KiB. `as-http`'s reads any body up to 64 KiB, and offers `read_head` and `copy_body` for streaming. So the server has to change, not just its imports. |
| the page's script | `as-page-kit` | page-kit was copied from Dexter's page. Not part of this path: it's a decision for after (step 6). |

## Step 0: before any code (yours, then one Dexter session)

1. **Choose the time.** Dexter's `main` should be clean with no PR open. Pick a stretch with no
   sessions booked: the installed `dexter` is what you record with. (Checked 2026-11-07: `main`
   clean at `c7ebba3`, level with GitHub, CI green, no PR open.)
2. **Push the suite.** Done 2026-11-07: the suite is on GitHub (the only remote now), so a
   Dexter pin can name any commit on its `main`.
3. **One Dexter session, docs only, as a PR.** Started from Dexter's folder:
   `cd ~/Documents/Github/dexter && claude --add-dir ../suite-audio-libs`. It does four things:
   - **Rule: the suite.** `docs/dev/PLAN.md` section 6 and `CLAUDE.md` say "a clone from GitHub
     builds with nothing else" (2026-09-29). It becomes "a clone builds with the network: the
     suite is fetched, pinned" (Warren, 2026-11-07). The bay's vendoring stays as it is: bay-rs
     isn't the suite.
   - **Rule: what it may change.** `CLAUDE.md`'s "Never modify anything in a directory outside
     this one" becomes "... outside this one and `../suite-audio-libs`", as `../CLAUDE.md`
     (the family's) has it. Add a "The suite" section like `../dexter-tape/CLAUDE.md`'s: the
     pin, the local patch, `CHANGES.md` read before a pin moves, the suite pushed first.
   - **The handoffs.** Read the four `Dexter.md` files listed in `../HANDOFFS.md`, and say for
     each ask whether the suite now meets it (the FLAC encoder is `as-flac`; foley's
     conversion is `as-resample`; `bext` is in `as-wav`) and whether it changes this path.
     Record that in `docs/dev/TODO.md`. Building any of them isn't part of step 0.
   - **The page protocol's clients.** `docs/page-protocol.md` names who reads it: the page,
     the remote, and `dexter-kms` (`verkaro/dexter-kms`, protocol 60). A protocol change then
     says who has to follow.

   Prompt:
   > Dexter adopts suite-audio-libs (Warren, 2026-11-07). Read ../CLAUDE.md, then
   > ../suite-audio-libs/Dexter.md, step 0. Do item 3's four parts as one PR, docs only, no
   > code. Then take item 4's baseline and put it in the PR's description. Stop there for my
   > go-ahead; step 1 is a separate session.

4. **Take the baseline** in Dexter on `main`, and keep the output in the PR's description:
   ```sh
   cargo test 2>&1 | tail -3
   DEXTER_JACK_TEST=1 cargo test -p dexter-jack -p dexter-engine 2>&1 | tail -3
   DEXTER_BROWSER_TEST=1 cargo test -p dexter --test page 2>&1 | tail -3
   cargo test --release -p dexter-wav bench_convert -- --ignored --nocapture   # step 3's yardstick
   ```
5. **Yours:** read the PR, merge it, and update step 0's line in `../HANDOFFS.md`
   (`./family.sh` pushes nothing: commit and push the family folder by hand).

## How every step after runs

- Open the session from Dexter's folder, with the suite beside it:
  `cd ~/Documents/Github/dexter && claude --add-dir ../suite-audio-libs`
- **The suite first, if needed.** Anything Dexter's copy has that the suite lacks goes into the
  suite as a suite commit with a test, and is pushed.
- **Then Dexter, as one PR:**
  - `[workspace.dependencies]` gets `as-X = { git = "https://github.com/verkaro/suite-audio-libs", rev = "..." }`
    and `.cargo/config.toml` (not in git) the patch line.
  - The users switch from `dexter_X::` to `as_X::`, and Dexter's copy is deleted.
  - A line goes in Dexter's `CLAUDE.md` layout, and the crate's row in this repository's
    `PLAN.md` table moves.
- **What proves it, every time:**
  - `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` all pass,
    with no test edited except its imports.
  - No reference file rewritten: if a bless looks needed, stop. That's a difference to
    understand, not to bless.
  - CI is green on the PR. That's the first proof the pin is fetched from GitHub, since CI has
    no local patch.
- **Then yours:** install it (`cargo build --release -p dexter && install -m 755
  target/release/dexter ~/.local/bin/`, then `dexter --stop`, `dexter`) and do the step's
  check by hand before merging.

## Step 1: meter (the plumbing proved)

The code is the same apart from comments, so anything that goes wrong here is the setup: the
pin, the patch, CI fetching. Used by `crates/server` only.

> Read ../suite-audio-libs/Dexter.md, "How every step after runs" and step 1. Move
> dexter-meter to as-meter as a PR.

**Yours:** the page's ladders and peaks fall as before, against a real session.

## Step 2: ws

Suite first: Dexter's `crates/ws/tests/echo.rs` goes into `as-ws` as its `tests/echo.rs`, with
`dexter_ws` renamed. Then Dexter: used by `crates/server`.

> Read ../suite-audio-libs/Dexter.md, step 2. Move Dexter's ws test into as-ws first, then
> dexter-ws to as-ws as a PR.

**Yours:**
- The page, the remote (the Nexus 7) and `dexter-kms` all connect, and reconnect after
  `dexter --stop` and `dexter`.
- The meters run for a few minutes without a stall.

## Step 3: wav (the big one)

Used by import, jack, export, project, rigcheck and tape. Recording runs through it.

> Read ../suite-audio-libs/Dexter.md, step 3. Move dexter-wav to as-wav as a PR. Compare
> bench_convert against the baseline in the step 0 notes before opening it.

**What proves it, on top of the usual:**
- `crates/project/testdata/rec` still reads. It stands for the projects people have.
- `bench_convert` (now `cargo test --release -p as-wav bench_convert -- --ignored
  --nocapture` in the suite) within noise of step 0's figure. If it's slower, stop and fix it in
  the suite before Dexter moves.
- `DEXTER_JACK_TEST=1` passes.
- `docs/README.md`'s row for `crates/wav/testdata/*.wav` goes: those files are blessed in the
  suite now, with `BLESS=1`.

**Yours**, on a scratch project on the real rig (the Clarett):
- Record a take on several tracks, play it back, and punch in.
- Export it.
- Import a WAV, both from a file and by upload.
- Open an old project and play one of its takes.
- Crash safety: while recording, `pkill -9` the engine, then start `dexter` again. The take is
  there up to the kill, and it plays.

## Step 4: http

The server changes here. Today `/api/upload`'s body is left unread for the upload to stream.
With `as-http`, the server reads the head with `read_head`, and reads the body itself for every
path but the upload.

> Read ../suite-audio-libs/Dexter.md, step 4. Move crates/server/src/http.rs to as-http as a
> PR. Propose how the upload's body stays unread before building it.

Decide when the session asks: the cap on other bodies goes from 4 KiB to 64 KiB with `as-http`.
Take it (recommended: `/api/cmd` bodies are small either way), or ask the suite for a per-call
cap.

**What proves it:** `DEXTER_BLESS` isn't used, and the golden test (`cargo test -p dexter-server
--test golden`) passes as it is.

**Yours:**
- Upload a large WAV for import from the page, across the LAN.
- The page and the remote load.
- A page from another site is still refused (same-site).

## Step 5: done

- Dexter's `CLAUDE.md` names no copies of its own for these four.
- `PLAN.md`'s table row for Dexter reads `meter, ws, wav, http`.
- The installed `dexter` has been used for a real session.

## Step 6: later, its own decision

**The page and `as-page-kit`.** page-kit was copied by hand from Dexter's page, and the others
have moved on with it. Before deciding anything, survey it in a session from this folder: what
page-kit has that Dexter's page lacks, and the reverse. Dexter's browser tests and the Nexus 7's
old Chromium (`the_remote_needs_no_new_css`) are the constraints.

**Not on this path:** Dexter's JACK client (the engine's, not the keeper), and anything of
dexter-mixer's. The bay crates were here too, until steps 7 to 10.

## Steps 7 to 10: JACK, the interface and the bay, as libraries (Warren, 2026-11-07)

Added 2026-11-07, after step 5, in the family's session. JACK control and the patch bay become a
tool of their own, **jackbay** (PLAN.md, Settled 5), and a Focusrite app may follow. Both need
what Dexter has: starting jackd, the rigs, the interface, the bay. That code reaches them only
through the suite, so Dexter makes it into libraries first and uses them itself. There's never
a second copy, and the libraries are proved by Dexter's tests and the real rig before jackbay
leans on them. jackbay starts once step 10 is done.

**Changed (Warren, 2026-11-10):** as step 7's was for focusrite-ctrl, the suite's half of steps 8
to 10 is done ahead of Dexter, for jackbay (`../Jackbay.md`). Each crate is moved in from Dexter and
tested in the suite, with Dexter untouched. Dexter's half of each, moving onto it, stays here at its
own pace, bringing in first anything its copy changed since the move. Until then Dexter's copy is
a second one, and a fix either needs is made in the suite.

They start after step 5 and don't wait for step 6. Each runs as "How every step after runs"
says, with one difference: the crate is *moved into* the suite from Dexter (the suite's
`VENDORED.md` saying so, as for place's and scape's), rather than Dexter's copy being swapped for
one already there. Dexter's engine (`crates/jack`'s `rt.rs`, `engine.rs`, `feeder.rs`,
`client.rs`, `measure`) and its own JACK client stay Dexter's. The crate names below are
proposals: the session settles them.

Found when this was written (Dexter's `main` at `26be596`):

| Dexter's | What ties it to Dexter | Proposed |
|---|---|---|
| `crates/focusrite` | Nothing: no dependencies of Dexter's | `as-focusrite` |
| `crates/jack`'s `jackd.rs` | Starts jackd from a project's settings | `as-jackd` |
| `crates/bay-core`, `crates/bay-jack`, `crates/jack`'s `graph.rs`, `crates/patch` | `patch` uses `dexter-protocol` and `dexter-eventlog`; the bay was vendored from bay-rs (`crates/bay-core/VENDORED.md`) | `as-bay` (the core and JACK's backend), the session perhaps beside it |
| `crates/project`'s `rig.rs` | A rig is half JACK's (the interface, how JACK runs, the setup commands) and half the deck's (the tracks, the monitor, the loopback, the latencies) | the JACK half only, its name to settle |

## Step 7: focusrite (the move proved)

Self-contained, so anything that goes wrong is the move itself. Its tests take a fake card read
from Warren's 18i20 (`testdata/18i20-gen3.txt`), which is a list of controls, not a recording.

**The suite's half is done** (Warren, 2026-11-10, for focusrite-ctrl): `as-focusrite` is
Dexter's `crates/focusrite` at `e98d44f`, as `VENDORED.md` says. What's left is Dexter's half.
Anything changed in Dexter's copy since `e98d44f` (the Clarett 2Pre's work, say) goes into the
suite first, then Dexter moves onto it.

> Read ../suite-audio-libs/Dexter.md, steps 7 to 10, then do step 7: as-focusrite is in the
> suite already; bring in anything Dexter's crates/focusrite has changed since e98d44f, then
> move Dexter onto it, as a PR.

**Yours:** the page's INTERFACE tab and the monitor mixes on the 18i20 and the Clarett 2Pre, as
before.

## Step 8: jackd

Starting, watching, finding and stopping jackd, found by `JACK_DEFAULT_SERVER`, and the
dummy-server harness the tests use. What it's started *with* comes from the caller, not from a
Dexter project, so the session first separates the settings from `dexter.yaml`.

> Read ../suite-audio-libs/Dexter.md, step 8. Propose how jackd.rs stops depending on Dexter's
> project before moving it, then move it into the suite as as-jackd, and Dexter onto it, as a PR.

**The suite's half** is proposed in `RFC-as-jackd.md` (2026-11-11, for jackbay), with
`as-localtime` and `as-interfaces` beside it. Not moved yet: it waits for Warren's answers.

**What proves it, on top of the usual:** `DEXTER_JACK_TEST=1` and the suite's
`SUITE_JACK_TEST=1` both pass, on dummy servers of their own.

**Yours:** start and stop JACK from the page with each rig, and `dexter --stop` leaves no jackd
behind.

## Step 9: the bay

The bay's core and its JACK backend (`bay-core`, `bay-jack`, `graph.rs`) are the library.
`crates/patch`'s session (the names, bundles, undo, a view and staged edits a client) is what
jackbay needs most, but it speaks Dexter's protocol and log: the session proposes where it
divides, and what stays Dexter's. bay-rs is Warren's own: the move is recorded in the suite's
`VENDORED.md` with bay-rs's commit (`af5c0c7`), and from then on the bay is changed in the suite.

> Read ../suite-audio-libs/Dexter.md, step 9. Propose how the bay divides between the suite and
> Dexter before moving anything, then do it as a PR.

**What proves it, on top of the usual:** `BAY_JACK_TEST=1` (as the suite names it) and the
golden test pass as they are; the page protocol doesn't change.

**Yours:** the PATCH tab on the real graph: a connection made, staged, applied and undone.

## Step 10: the rig's JACK half

Last, since it needs a decision first: what a rig is outside Dexter. jackbay needs the
interface, how JACK runs it and the setup commands; the tracks, monitor, loopback and
latencies stay the deck's. Old rig files must still read.

> Read ../suite-audio-libs/Dexter.md, step 10. Propose how a rig divides, with how old rig files
> keep reading, before moving anything; then do it as a PR.

**Yours:** each of your rigs opens, starts JACK, and a new project made from one is as before.

**Done:** Dexter's `CLAUDE.md` names these as the suite's, `PLAN.md`'s row for Dexter lists them,
and jackbay can start.
