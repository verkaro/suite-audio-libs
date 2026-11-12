# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

The audio libraries the Dexter family shares (`as-wav`, `as-ws`, `as-jack-keeper`, …): audio
engineering, and the plumbing each tool used to write for itself. The README describes every
crate. `PLAN.md` is the plan: which tool is on which crate, which tools still keep their own
copies, and how a tool moves in. `Dexter.md` is Dexter's path onto the suite. `CHANGES.md` lists
what changed, for tools moving their pins. `VENDORED.md` records where each crate came from.
The family's rules are `../CLAUDE.md`.

Tools depend on this repository by git address, pinned to a commit. While working, each tool
has a `.cargo/config.toml`, kept out of git, that patches that address to this folder. A change
made here therefore shows up at once in every tool beside it that uses the crate.

## Commands

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
FAMILY_SLOW=1 cargo test                                  # also the slow tests, as CI does
cargo test -p as-wav NAME                                 # one test in one crate
BLESS=1 cargo test -p as-wav reference_files              # rewrites testdata/: on purpose only
SUITE_JACK_TEST=1 cargo test -p as-jack-keeper --test dummy   # also as-transport, as-jack-capture, as-jackd, as-bench; as-bay-jack's is --test live
python3 -I tools/cdp.py                                   # drives a page in Falkon offscreen (as-page-kit, tools' page tests)
```

Building needs JACK2's dev package (`libjack-jackd2-dev`).

## Rules

- **Only shared code belongs here.** A crate is added only if more than one tool will use it. A
  deck, a player or a set format belongs to its tool.
- **A new crate, or one moved in, comes with:**
  - a header comment giving its purpose, where it came from, and the date
  - an entry in the README
  - an entry in `VENDORED.md` ("Written here, not moved" if it is new)
  - a line in `[workspace.dependencies]`
  - tests
- **Third-party dependencies:** check `[workspace.dependencies]` first and reuse what is
  there. Otherwise vendor it and record it in `VENDORED.md`.
- **Fixes:** a fix that a tool needs is made here, never in a tool's copy.
- **One of each thing** (Warren, 2026-11-11): within the suite there is one copy of anything. A
  second found is fixed in the session that finds it, with a `CHANGES.md` line, never invited to
  move "when it wants". A crate's tests use the suite's own harness (`as_jackd::Dummy` for JACK).
- **A session here ends with `../family.sh check suite-audio-libs`** (Warren, 2026-11-11): the
  tools patched to this folder are run before it's pushed.
- **Moving a tool in:** first bring anything the tool's copy has that the suite lacks into the
  suite, with a test. The tool's reference files must still match byte for byte (see
  `PLAN.md`, "Moving a tool in"). Update `PLAN.md`'s "Where each tool stands" when a tool moves (its numbers are `../family.sh pins` and `copies`).
- **`CHANGES.md`:** a change to an interface or a behaviour gets a line there in the same
  commit, newest first, naming the crate and what a tool has to do about it.
- **Pushing:** to GitHub only (soft-serve retired, 2026-11-07), before any tool whose pin moves
  to it, so a fresh clone of the tool builds. Cargo fetches the pinned commit before applying
  the local patch.
- **Watch for breakage:** changing a public interface breaks every tool patched to this folder.
  Run the affected tools' tests too (`../brr`, `../dexter-tape`, `../dexter-ducky`,
  `../dexter-kms` for `as-ws`'s client, and Dexter as it moves in), or note the change for
  their next pin bump.
