# Changes

What changed in an interface or a behaviour, newest first: a line for each, naming the crate and
what a tool has to do about it. Read it before moving a pin. Begun 2026-11-07; for before that,
the git log.

## 2026-11-12

- `as-eq`, new (step 3 of `specs/shared.md`; its spec is `crates/eq/docs/spec.md`): `Eq`
  (`new`, `set`, `process`, `process2`, `settings`, `settled`, `readouts`), `Settings` (`get`,
  `set_by_id`, `held`), `Layout::CHANNEL`, `controls`, `response`, `band_response`, `marker`,
  `q_eff`. Nothing for a tool to do: sonorack takes it first, at its step 1.
- `as-biquad`: `Biquad::matched`, `peak_matched`, `low_shelf_matched`, `high_shelf_matched` and
  `inverse`, added (step 2 of `specs/shared.md`, for `as-eq`). `lowpass_matched` unchanged, bit for
  bit. Nothing for a tool to do.
- `as-control`, new (step 1 of `specs/shared.md`; its spec is `crates/control/docs/spec.md`):
  `Control`, `Kind`, `Scale`, `Unit`, `Law`, `Shows`, `Size`; `clamp`, `step`, `grid`, `format`,
  `parse`, `find`, `check`. Nothing for a tool to do: `as-eq` and `as-dynamics` take it first.
- `as-bench`, new (step 0 of `specs/shared.md`; its spec is `crates/bench/docs/spec.md`):
  signals under a ceiling with a sync mark, `align::find`, `check::{calibrate, check}`,
  `figures` (TOML), `picture` (as data), `sitting` (the measuring list, resume, redo),
  `jack::Bench` (two outs, two ins, never connecting a port), and `run_model`. Nothing for a tool
  to do until it takes it: sonorack's `measure` first.
- `as-jack-keeper`, `as-jack-capture`, `as-transport`: their dummy tests run on
  `as_jackd::Dummy`, the servers now `keepertest-<pid>`, `capturetest-<pid>` and
  `transporttest-<pid>`; keeper and capture no longer depend on `libc`. Nothing for a tool to do.
- `as-jack-client`, new: `open(name)`, `lock_memory(note)`, `Notices`/`Notify` over a `Counts`,
  `Graph`, `JackGraph`, `reconcile`, the one copy of what `as-jack-keeper` and
  `as-jack-capture` each had (Warren, 2026-11-12). Their interfaces don't move: nothing for a
  tool to do. `as-jack-capture` now says why a client couldn't open in `as-jack-keeper`'s words
  ("another … is running", "is jackd running?"), and notes once if memory can't be locked.
- `as-serve`: `Tool::left(id)`, called when an admitted client has gone (it left, was closed for
  falling behind, or the server is ending), once, after its last command, with nothing locked.
  A default that does nothing: nothing for a tool to do (jackbay's `SPEC.md` 8.1).

## 2026-11-11

- `as-bay`: **`as_bay::sheet`**, new (`RFC-as-bay.md`, b2): `Sheet::new(backend, grouping)` or
  `with_rules(…, rules)`, `op(screen, Op) -> Result<Matrix, Refused>`, `forget`, `define`,
  `undefine`, `set_names`, `snapshot`, `count`. Nothing to do for a tool until it takes it.
  - **`Rules`**: `check`, `warnings(now, after)`, `shown`, `reserved`, `favourites`, `log`, each
    letting everything by unless a tool says otherwise. A warning that `ask`s refuses APPLY
    with `Refused::Ask(lines)` until `anyway`; one that doesn't is only said. So `Refused` has
    `Ask` beside the RFC's `Patch`, `Rule` and `Other`, and Dexter's loops and prints are one
    `warnings` list.
  - **The speaker mark is the tool's** (jackbay's ask 2): each staged line carries its `links`
    by full port name, and the tool marks it over the `Matrix`. The sheet marks nothing.
  - **`Matrix`** has `PatchMatrix`'s fields, with `warnings` for `loops` and `prints`, rows'
    and columns' `declared` for `track`, and `grouping`. A line's `words` are as said when it
    was staged.
  - **A screen's own grouping** (jackbay's `SPEC.md` 8.3): `Op::View` carries `grouping`, None
    being the sheet's own. Under it, `View::grouping` and `Session::bundles_with(snap,
    grouping)`; None as before. Nothing to do for a tool that writes `View`s with
    `..View::default()`; one written field by field adds `grouping: None`.
  - On the fake backend it needs no JACK: a tool's `--sim` runs it as it is.
- `as-bay` and `as-bay-jack`, new crates, moved in from Dexter at `e98d44f` for jackbay
  (`RFC-as-bay.md`, b1; the sheet, b2, is still to come). Nothing to do for a tool until it takes
  them. Dexter keeps its own copies until its step 9, which has these to follow:
  - **`as_bay`** is `bay_core`, and **`as_bay_jack`** is `bay_jack`, as they were.
  - **`Jack::open_as(name)`**, new: the backend's client under a tool's own name. `Jack::open()`
    is `bay`, as before.
  - **The live test** runs with `SUITE_JACK_TEST=1` on `as_jackd::Dummy`, where Dexter's is
    `BAY_JACK_TEST=1`.
- `as-page-kit`: the common keys gain tier 3's letters (Warren, 2026-11-11, `../STYLE.md`, "Keys"):
  `m` "mark here", `-` "mute", `=` "solo", `o` "the loop on or off", as `lp-keys` has them. **What
  a tool has to do** when its pin moves here: a `keys.json` that uses one of the four for
  something else fails the keys test until it moves the key or says why in `differs`. BRR (mark
  on `t`, mute and solo on `m` and `M`), foley (loop on `l`) and irma (LOOP with no key) have
  moves listed in `../STYLE.md`, "Not yet the same", item 17. irma's `m` already reads "mark here". irma's `o` (the outs) and focusrite-ctrl's `o` (the scenes) move
  off it (Warren, 2026-11-11). Patched to this folder, BRR, irma and focusrite-ctrl fail their keys
  test at once, not only when their pins move; each deals with it in its next session.
- `as-jackd`, `as-interfaces` and `as-localtime`, new crates, moved in from Dexter at `e98d44f`
  for jackbay (`RFC-as-jackd.md`). Nothing to do for a tool until it takes them. Dexter keeps
  its own copies until its step 8, which has these to follow:
  - **`Settings`** is `JackConfig`; `args(server)` is `jackd_args()`, with the server's name
    passed; `on_dummy(captures, playbacks)` takes the counts a project's tracks make.
  - **`start(&Start, probe, log, out)`** for `start_server(&Config, log, out)`. `Start` holds the
    settings, the interface's id and the setup commands. `Start::interface_missing` is the test
    in `without_interface`.
  - **`stop(probe, refuse, force)`** for `stop_server(force)`: Dexter passes its engine's name.
  - **The log** is `$XDG_STATE_HOME/jack/jackd.log`, shared by whoever starts JACK (Warren,
    2026-11-11). `run_start`, `tail` and `noise` as before; `Follow` reads new lines.
  - **`Monitor`** has `graph()` beside `xruns`, `shut_down`, `dsp_load` and `clients`; Dexter's
    `watch` text is built on it and `Follow`.
  - `find`, `running_io`, `running_driver`, `running_alsa_device` and `started` as before.
- `as-jackd`: `Dummy`, the tests' JACK server (`<tool>test-<pid>`, dummy driver, stopped by its
  own pid), and `eventually`. Every tool, and `as-jack-keeper`, `as-transport` and
  `as-jack-capture` here, writes its own today: each can move onto it when it wants.

## 2026-11-10

- `as-focusrite`: `Focusrite::spdif_source` (a Clarett's `S/PDIF Source Capture Enum`, the
  8Pre's, from the driver), `Now::digital_mode` and `spdif_source`, and `Change::Clock`,
  `DigitalMode` and `SpdifSource`, each by its item's name. *Breaks* a `match` on `Change`
  with no `_` arm, and a `Now { … }` written in full.
- `as-meter`: `Ladder::level_db` and `peak_db`, the bar's level and the held peak in dBFS, for
  focusrite-ctrl's ring after a knob's gain. Nothing to do.
- `as-serve`: `Joining::from`, the address a client connected from, for a tool that lets
  only some addresses change things (focusrite-ctrl's `--controller`). Nothing to do.
- `as-focusrite`, for focusrite-ctrl (its step 1). Nothing to do for a tool that doesn't use
  it; Dexter, moving onto it at its step 7, has the items marked *breaks* to follow:
  - **The meters:** `Focusrite::meter` (`Meter`: `db`, `full`), `meter_dests`, `meter_of`,
    `read_meters`. One value for each routing destination in the driver's order (analogue,
    S/PDIF, ADAT outputs, mixer inputs, captures), each the level of its source.
  - **The Clarett 2Pre:** `CLARETT_2PRE`, `testdata/clarett-2pre.txt`, rewritten by the card
    test's `BLESS=1` on a 2Pre as the 18i20's is on an 18i20.
  - **`Focusrite`** gains `standalone`, `firmware`, `validity`, and `Line` gains `label`
    (`Monitor L`). *Breaks* a `Line { … }` written out in full.
  - **`Panel`:** `Now` gains `lines` (`LineNow`), `standalone`, `firmware` and `validity`;
    `Change` gains `LineHw`, `LineVolume`, `LineMute` and `Standalone`. *Breaks* a `match` on
    `Change` with no `_` arm.
  - **`hand`, new:** `Dest`, `route`, `routes`, the destinations by name (`dest_named`:
    `LINE 1`, `MIXER 3`, `CAPTURE 2`), and `Mixing`: a mix's sources added, cut or made the only
    one, the mixer's inputs shared out by need and freed when no mix hears them, gains kept as
    written and `write_all` after a panel scrambled the driver's copy. `MixError` says why not.
  - **On the interface:** `sets_by_hand_and_puts_it_back`, under `SUITE_FOCUSRITE_TEST=1`, on
    any Focusrite, touching only a mix nothing plays.

- `as-focusrite`, a new crate: Dexter's `dexter-focusrite`, moved in for focusrite-ctrl. The
  same interface as Dexter's at `e98d44f`. Its tests on the interface itself are
  `SUITE_FOCUSRITE_TEST=1` (and `SUITE_FOCUSRITE_JACK=1` for `landing`); the fake's file is
  rewritten with `BLESS=1` on an 18i20 3rd gen. Nothing to do for a tool until it takes it.

## 2026-11-09

- `as-serve`, a new crate: a tool's server over `as-http` and `as-ws`. `Server::serve` runs the
  listener, the checks on each connection (`same_site`, `GET` and `HEAD` only), the WebSocket at
  `/ws` and a tick; `Clients::send` and `send_to` reach the clients; a tool implements `Tool`
  (`hello`, `command`, `refusal`, and `tick`, `get`, `note` if it wants them). A client whose
  queue of 256 fills is closed. Nothing to do for a tool that has its own. BRR, Tape and ducky
  can each replace theirs with it, in their own sessions (`../HANDOFFS.md`).
- `as-flac`: `tags`, a FLAC's Vorbis comments read (`tags::read`), set safely (`tags::set`,
  `set_as`: a new file, every other block and every audio byte checked against the old,
  journalled, renamed over) and put on a stream in memory (`tags::with_tags`), from dexter-foley's
  `foley-tags`. No field is refused by name: foley, when it moves, keeps its own check that
  `SERIAL` is never set. The journal's lines are as foley's. Nothing to do until a tool takes it.
- `as-ir`: `numbers`, an IR's numbers after ISO 3382-1 (`numbers::describe`, `span`, `onset`,
  `weight`, `BANDS`), from dexter-place's `core/src/numbers.rs`. It takes `&[Vec<f32>]`. `as-ir`
  now depends on `realfft`. Nothing to do until a tool takes it.
- `as-page-kit`: `kitBar`, the command bar (`STYLE.md`): `ask`, `offer`, `type` (a line, with a
  `check` that keeps it open), `choose`, and `screen` for the screen's levels. `key()` now keeps its
  name and section in `HITS`, and a disabled key lands in `HITS` too, with an empty rect, so the bar
  can dim it; a page's own hit test still can't reach it. `kitSection(name)` says which section the
  keys drawn next fall in, in place of `label()`, which is also used for column heads and counters.
  To take it up: call `kitBar.key` first in the key handler, `kitBar.tap` for `bar:` hits,
  `kitBar.letGo` for other clicks, `kitBar.draw` last, in a footer of its own.
- `as-page-kit`: `KEYS`, the family's common keys as data, and `check_keys(declared, pages)` for a
  tool's tests: a common key meaning something else with no `differs`, and a key a page compares
  against that isn't declared, both fail. `kitKeysOver(list, screens, r)` draws a tool's `?`. To take
  it up: a `keys.json`, a test calling `check_keys`, and a `?` (`PLAN.md`, "The keys and the command
  bar"). Nothing changes for a tool until then.

## 2026-11-07

- `as-ws`: `client`, the client's half (`connect`, `Reader::read`, `Writer::send_text`,
  `send_binary`, `close`), from dexter-kms. Reading waits for ever unless
  `Reader::set_read_timeout` says otherwise. Nothing to do for a server; a panel uses it in
  place of a copy of its own.
- `as-http`: `read_body(r, &mut req, max)`, a body read after `read_head` up to a cap of the
  caller's. `read_request` is `read_head` and `read_body` at `MAX_BODY`, as before. Nothing to do.
- `as-ws`: `tests/echo.rs` from Dexter (an echo server over a real socket). Nothing changed in
  the crate; nothing to do.
- No crate changed. `PLAN.md`, `Dexter.md`, `CLAUDE.md` and this file added.
