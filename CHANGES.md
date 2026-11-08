# Changes

What changed in an interface or a behaviour, newest first: a line for each, naming the crate and
what a tool has to do about it. Read it before moving a pin. Begun 2026-11-07; for before that,
the git log.

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
