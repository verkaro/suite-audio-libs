# RFC: as-mcu and as-surface, the FaderPort as a library

From the family's session, 2026-11-11, at Warren's asking: Dexter's surface controller prepared
for the suite, so a FaderPort can play the other tools (BRR first), a FaderPort app of its own
can be built (`../Sonofader2.md`, the repository `verkaro/sonofader2`), and Dexter can later
replace its built-in crates with the shared ones. Read from Dexter's `main` at `e98d44f`, with
nothing run and nothing moved. Dexter isn't touched: its copies stay its own until its step 11.

**Settled 2026-11-11** (see the end): all six as recommended.

## What's there

Two crates, both from `verkaro/dexter-surface` (archived; brought into Dexter 2026-10-18 at
`87ea53a`, `crates/surface/VENDORED.md`), and the command line in Dexter's binary. The docs are
`dexter/docs/faderport.md` (the program), `docs/surface.md` (what the engine offers it) and
`docs/faderport-capture.md` (how a surface's messages are recorded).

| Dexter's | Lines | What | Tied to Dexter by |
|---|---|---|---|
| `crates/mcu` (`dexter-mcu`) | 1,253 | `midi`: a parser (running status, SysEx read past) and hex; `mcu`: Mackie Control's bytes to `Event` and `Out` (LEDs, the motor, the ring) and back; `table`: `Control`, `Device` (`STANDARD`, `FADERPORT_V2`, `KNOWN`), only where a device differs; `capture`: a capture file read, for the tests; `port`: `/proc/asound/cards` and `/dev/snd/midiC*D*` under a root, `Want` (a card's name, `hw:N,M`, a path), `find`, `open` with `Refused` (busy, no access); `surface`: the port kept open on a thread, `Opener` (the real one or the tests'), `Status`, `Note`, `STALE` (the first 100 ms dropped), the message log | nothing: no dependencies at all. `testdata/faderport-v2/` is the FaderPort V2's capture, read by the tests |
| `crates/surface/src/desk.rs` | 293 | the seam: `Desk` (state, command, name, refusals, focus, open, picked), `DeskState`, `Strip`, `Has`, `Kind` (Track, Input, Out), `Mix`, `Cmd`, `Refusal`, `Level` (hundredths of a dB), `Taper` | in name only: `Taper::Dexter` is the one taper, and `Refusal`'s codes are Dexter's strings |
| `crates/surface/src/binding/` | 584, and 506 of tests | the rules, pure: `Input` (what's done, on a surface or a screen's keys), `Controls` (the strip, the link, the held balance), `act_all`; `binding::surface`, the FaderPort's events to `Input` and the desk's state to its LEDs and motor, with the timings (touch, momentary wind, the knob's growing steps, blink) | `dexter-mcu` only. The comments name Dexter's page for the timings they copy |
| `crates/surface/src/run.rs` | 242, and 140 of tests | the loop: `Options`, `present`, `monitor` (the surface alone), `run` (the device's thread, the binding, the desk, the log's lines) | `dexter-mcu`; the test runs it on the adapter and a simulated engine |
| `crates/surface/src/testbed.rs` | 181, and 155 of tests | two silent strips and a transport, refusing where Dexter would: the binding's fake desk, test-only | nothing |
| `crates/surface/src/process.rs` | 110 | one surface for an engine's socket: an flock on `<socket>.surface` with the pid in it; `take`, `running`, `stop` | `libc`; the path is Dexter's socket's |
| `crates/surface/src/dexter.rs` | 486, and 197 of tests | **the adapter:** Dexter's engine as the desk, on `dexter_protocol::Console`: tracks, the interface's inputs and the mixes' outputs as strips, one mix at a time, `surface_track`, the mixes asked again as their count moves | `dexter-protocol`; the tests on `dexter-sim` and `dexter-project` |
| `crates/dexter/src/surface.rs` | — | `dexter --surface`: the command line, the lock, the session starting it beside the engine, `--monitor`, the page's ATTACH and RESET | everything Dexter's |
| `contrib/capture-surface.sh` | — | the capture script Warren runs at the unit | nothing |

So the line is already drawn where `docs/faderport.md` says it is: only the adapter and the
command line know Dexter. The rest was written to stand alone, and the move is mostly a move.

**Overlap with the suite:** `as-rawmidi` (2026-11-04, for dexter-like110) finds raw MIDI ports
under a root "after the way dexter-surface finds its ports", opens one for writing, and writes a
keyboard's messages. `dexter-mcu`'s `port` does the finding too, and also parses `Want`, finds by
a card's name, and opens a port for reading and writing with a `Refused` that tells busy from no
access. Two copies of the same `/proc/asound/cards` reader would be in the suite.

**What's Dexter-shaped in what moves, and would stay so:** the fader's travel is Dexter's MIX
page's law, −80 dB to +6 with 0 dB at four fifths (`Taper::Dexter`, the only variant); the
refusal codes the binding blinks on are whatever a desk returns, so nothing to change there;
`Kind::Input` and `Kind::Out` are a Focusrite's mixer and a mix's masters, which another desk
simply doesn't list; `Mix` is one desk's having more than one mix, `None` for the others. The
testbed already proves a desk that isn't Dexter's plays.

## The proposal

**`as-mcu`:** `dexter-mcu`, moved as it is, renamed, with its capture testdata. Its `port` module
goes onto `as-rawmidi` (Question 1): `as-rawmidi` gains `Want`, `find(root, &[Want])`, a `Card`
with its long name, and an open for reading and writing with `Refused` (busy, no access, other);
`as-mcu` keeps `surface` (the port kept open) over them. Additive for dexter-like110: its `ports`,
`find(root, card_id, device)` and `Out` don't change.

**`as-surface`:** `desk`, `binding`, `run` and `testbed`, moved, with three small changes made
in the move and pinned by the tests Dexter has:

- **The taper is data** (Question 2): `Taper` holds its points, with `Taper::DEXTER` the constant
  Dexter's `FADER_LAW` is; `dexters_taper` pins the same numbers. A desk says its own in `Has`,
  as it already does per strip. BRR's law and the testbed's are then theirs to give.
- **The testbed is public,** `as_surface::testbed` (Question 3): a desk with strips and a
  transport and no sound, refusing as a deck would. sonofader2's first desk, and every tool's
  way to try the binding without a device. Its tests move with it.
- **The lock moves,** as `as_surface::lock` (Question 4): `take(path)`, `running(path)`,
  `stop(path)`, with the path the tool's. Dexter passes `<socket>.surface` as now; sonofader2
  one of its own, so two of it never hold the port.

Comments that name Dexter's page for a timing say so still ("as Dexter's page has it"): that's
where the rule came from, and the family's keys are kept alike on purpose (`../STYLE.md`).

**`run`'s end-to-end test** is written on the testbed in the suite (the fake FaderPort on a pair
of sockets, as now), since the adapter and `dexter-sim` stay Dexter's. Dexter's own copy of that
test stays Dexter's at its step 11, on `as_surface::run`.

**What stays Dexter's:** `dexter.rs` (the adapter: `crates/surface` becomes that alone, on
`as-surface`), `dexter --surface` and the session's starting of it, the page's ATTACH and RESET,
the program's tests (a pty in a scratch `/dev/snd`, a simulated engine), and the docs, which get
a pointer to the suite. `contrib/capture-surface.sh` stays where it is until sonofader2 has a
capture command of its own (`../Sonofader2.md`); the suite holds libraries, not scripts.

**Dexter's open asks on the surface** (`dexter/docs/dev/TODO.md`: the footswitch, Ø on
SHIFT + PAN, a key between MIX and DECK, REW and FF following the roll/stop setting) are each a
binding change and an adapter change. After s2 the binding's half is made in the suite, with a
`CHANGES.md` line, and Dexter takes it at step 11 or a pin bump (Question 5). None is built yet,
so nothing is lost by waiting.

**In two commits,** as `as-bay` goes:

- **s1:** `as-rawmidi` extended, and `as-mcu` moved onto it, each with its `VENDORED.md` entry
  (from Dexter at `e98d44f`, and dexter-surface at `87ea53a` before that), README entry and
  `CHANGES.md` line.
- **s2:** `as-surface`: the seam, the binding, the loop, the testbed and the lock, with the
  three changes above, the binding's and the testbed's tests as they are, and `run`'s test on the
  testbed.

Then, in their own sessions: **s3**, sonofader2 born on them (`../Sonofader2.md`); **Dexter's
step 11** (`Dexter.md`), when its gates reopen: `crates/mcu` gone, `crates/surface` the adapter
on `as-surface`, its reference files and page protocol untouched; **BRR**, by a handoff written
once sonofader2 has played a second desk and shown what the seam lacks for a tracker (the
transport's loop and sections; lanes as strips). Until step 11, Dexter's crates are a second copy:
a fix either needs goes into the suite, and Dexter's copy follows (as `as-focusrite`).

## What proves it

- s1: `as-mcu`'s tests unchanged but for the crate's name, the capture's every control among
  them; `as-rawmidi`'s tests unchanged, plus `Want` and `Refused` brought over from `port`'s.
- s2: the binding's tests unchanged; the testbed's unchanged; `dexters_taper` pins the law as
  data; the loop end to end on the testbed; the lock's test as Dexter has it.
- Dexter's suites pass untouched, since nothing of Dexter's changes.
- No test opens a real MIDI device, as now.

## Questions for Warren

Each has a recommendation, which the work goes on by once he gives the go-ahead.

1. **`port` onto `as-rawmidi`,** or `as-mcu` with a port finder of its own? Recommended: onto
   `as-rawmidi`, extended additively. One reader of `/proc/asound/cards` in the suite, and
   dexter-like110 has nothing to do.
2. **The taper as data.** Recommended: yes, now: a one-line change in the move, pinned by the
   test that exists, and the first thing a second desk needs.
3. **The testbed public.** Recommended: yes: sonofader2 starts on it, and a tool's session can
   try the surface's rules on it without a tool.
4. **The lock moves.** Recommended: yes: two programs on one FaderPort is the problem it solves,
   and that's any tool's.
5. **Dexter's open surface asks go into the suite's binding** after s2, Dexter following at step
   11. Recommended: yes. The alternative is building them twice.
6. **The names:** `as-mcu` and `as-surface`, or `as-faderport` for the lot? Recommended: the two.
   The code is Mackie Control with one device's table so far; a second surface is a table.

## Settled (Warren, 2026-11-11)

All six as recommended:

1. `port` goes onto `as-rawmidi`, extended additively; `as-mcu` keeps `surface` over it.
2. The taper is data, `Taper::DEXTER` the constant, `dexters_taper` pinning it.
3. The testbed is public, `as_surface::testbed`.
4. The lock moves, `as_surface::lock`, the path the tool's.
5. Dexter's open surface asks are built in the suite's binding after s2; Dexter follows at step 11.
6. The names are `as-mcu` and `as-surface`.

s1 and s2 can start, after as-bay's b1 and b2.
