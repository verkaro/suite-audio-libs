# Changes

What changed in an interface or a behaviour, newest first: a line for each, naming the crate and
what a tool has to do about it. Read it before moving a pin. Begun 2026-11-07; for before that,
the git log.

## 2026-11-10

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
