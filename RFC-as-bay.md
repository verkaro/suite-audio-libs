# RFC: as-bay, the patch bay

From the family's session, 2026-11-11, for jackbay (`../Jackbay.md`, step b) and Dexter's step 9
(`Dexter.md`). Read from Dexter's `main` at `e98d44f`, with nothing run and nothing moved. Dexter
isn't touched: its copies stay its own until its step 9.

**Settled 2026-11-11** (see the end): all four as recommended. The move starts on 2026-11-12, with b1.

## What's there

| Dexter's | Lines | What | Tied to Dexter by |
|---|---|---|---|
| `crates/bay-core` | 5,522 | bay's model, patch syntax, bundles, compile, render, the matrix and its views, staging, the session and its undo, a fake backend and its fixture format | nothing: no dependencies at all |
| `crates/bay-jack` | 202 | the graph read from any `jack::Client` (`snapshot_of`), a plan run through one (`run_on`), `GraphNotices`, and `Jack`, bay's own backend on a client named `bay` | nothing |
| `crates/jack/src/graph.rs` | 40 | the backend over Dexter's engine's own client | Dexter's `client::Handle` |
| `crates/patch` | 854, and 600 of tests | the PATCH tab's session: one bay shared by every screen, each screen with its own view and staged lines; the three verbs; apply and undo; the interface's ports named; and Dexter's rules | `dexter-protocol` (`PatchOp`, `PatchMatrix`, `Refusal`, `State`), `dexter-eventlog` |

`bay-core` and `bay-jack` are bay-rs at `af5c0c7` (vendored 2026-09-29). Since then they've only
been rustfmt'd. Their comments point to bay-rs's `PLAN.md` and `SYNTAX.md` nine times, and neither
file came with them.

`dexter-patch` is two things in one:

- **A sheet,** which any tool with a patch bay needs: the shared session and undo, a screen per
  client with its view (all or Dexter's, a zoom) and its staged lines, a cell staged by TO, CUT
  or ONLY (ONLY's cuts found among every client's ports, not only the ones shown), a cell
  toggled, APPLY and UNDO (an undo tried on a copy, so a refused one stays), a new snapshot when
  the graph's count moves, the interface's ports named after what the interface calls them, each
  staged line in the lists' words, and the matrix that's drawn.
- **Dexter's rules:**
  - a bundle for each track and each mix
  - the guard: the armed inputs during REC, every input while a Focusrite's mixer follows them,
    one input a track, the rig's loop port
  - the loops that would bring Dexter's playing back into a track it records, through other
    programs and the ways back JACK doesn't show
  - `studio`, the fake graph for `--sim`
  - the refusals in Dexter's protocol, and its log

## The proposal

**`as-bay`:** `bay-core`, moved as it is, renamed. Nothing changes but the name and the pointers
to bay-rs's docs (Question 4).

**`as-bay-jack`:** `bay-jack`, moved as it is, renamed, with one addition: `Jack::open_as(name)`,
so jackbay's client is `jackbay` and not `bay`. `Jack::open()` stays, naming it `bay`. Its live
test moves onto `as_jackd::Dummy`.

**`as_bay::sheet`,** written here from `dexter-patch`'s sheet, with its rules left to the tool:

- `Sheet::new(backend, grouping)`, `Sheet::op(screen, Op) -> Result<Matrix, Refused>` with the ops
  Dexter's protocol has (view, toggle, stage, unstage, apply, drop, undo), and `forget(screen)`.
- **The tool's rules** through a trait, `Rules`:
  - `check(plan, snapshot)`, which may refuse an APPLY or an UNDO with its own code
  - `warnings(before, after)`, the lines APPLY asks about first (`anyway`)
  - `shown(client)`, for the view that isn't showing all
  - `reserved()`, ports hidden and never patched
  - `log(line)`

  Dexter's guard and loops are a `Rules`. jackbay's warning that a line reaches a speaker, in
  `Jackbay.md`, is another.
- **Names:** `define(statement)` for a tool's own bundles (Dexter's tracks and mixes), and the
  interface's ports named by `names`, again with each snapshot, as now.
- **`Matrix`** has the fields of Dexter's `PatchMatrix`: rows, columns, the cells as four
  characters each, the staged lines with their words, `can_undo`, the count, and the warnings,
  where Dexter has loops and prints. Dexter maps it field for field. Since jackbay's protocol can
  say the sheet the same way, a panel's drawing of it can later be shared through
  `suite-light-panel`.
- **`Refused`:** `Patch { kind, text }` (bay's own codes), `Rule { kind, text }` (the tool's) and
  `Other(text)`. Dexter maps them onto its `Refusal`: `armed_input` and `rolling` stay its own.

**What stays Dexter's:** `graph.rs` (Question 2), and in `dexter-patch`: `Setup`'s tracks and
mixes, `Guard`, `guarded`, `may_record`, `Way` and the loops, `studio`, `track_bundle`, and the
protocol's types. `dexter-patch` becomes its rules over the sheet at its step 9, and its
protocol and golden test don't change.

**In two commits,** so the move can be checked apart from what's new:

- **b1:** `as-bay` and `as-bay-jack` moved, each with its `VENDORED.md` entry (from Dexter at
  `e98d44f`, and bay-rs at `af5c0c7` before that) and its `CHANGES.md` line.
- **b2:** the sheet, with the tests of `dexter-patch` that are the sheet's (staging, the three
  verbs, ONLY's hidden cuts, a stale line, apply, undo refused and kept, names after each
  snapshot) moved onto bay's fake backend. Dexter's rules' tests stay Dexter's.

## What proves it

- b1: bay-core's tests unchanged but for the crate's name, and `SUITE_JACK_TEST=1` for bay-jack's
  live test on `Dummy`, beside Warren's own server and never touching it.
- b2: the sheet's tests on the fake, plus one written here: a `Rules` that refuses and one that
  warns, so the hook is proved without Dexter.
- Dexter's suites pass untouched, since nothing of Dexter's changes.

## Questions for Warren

Each has a recommendation, which the work goes on by once he gives the go-ahead.

1. **Two crates,** `as-bay` (no I/O) and `as-bay-jack`, as bay has them? Recommended: yes. A tool
   can run the sheet on the fake backend under `--sim` without linking JACK.
2. **`graph.rs` stays Dexter's.** `Dexter.md`'s table lists it for the library, but it's only the
   backend over Dexter's own engine client. `as-bay-jack`'s `snapshot_of` and `run_on` are what's
   shared. Recommended: it stays.
3. **jackbay's sheet in pairs.** Dexter's bay is mono (Warren, 2026-09-27), every port its own row,
   as a multitrack deck's channels are. jackbay mostly patches programs' stereo outs to speakers.
   Recommended: the grouping is the tool's setting. jackbay starts in pairs, with a key to show
   one side alone, as focusrite-ctrl's PATCH does (`p`). This changes what you'll see.
4. **bay-rs's docs.** `PLAN.md` §3 (the design) and `SYNTAX.md` (the patch syntax) are named in the
   code but were never brought over, and bay-rs isn't on this machine. Recommended: bring both
   into `crates/bay/docs/` from bay-rs at `af5c0c7`, if you can put bay-rs beside the family
   (it's yours, so they're not copied from a sibling). Otherwise the pointers say they're bay-rs's,
   and `SYNTAX.md` is written fresh from `syntax.rs` when jackbay's panel first shows the syntax.

## Settled (Warren, 2026-11-11)

All four as recommended:

1. Two crates, `as-bay` (no I/O) and `as-bay-jack`.
2. `graph.rs` stays Dexter's. `Dexter.md`'s table is read with that.
3. The grouping is the tool's setting. Dexter's stays mono. jackbay starts in pairs, with a key
   to show one side alone, as focusrite-ctrl's `p`.
4. bay-rs's `PLAN.md` and `SYNTAX.md` come into `crates/bay/docs/` from `af5c0c7` if bay-rs is
   put beside the family. Until then, the pointers say they're bay-rs's, and `SYNTAX.md` is
   written fresh from `syntax.rs` when jackbay's panel first shows the syntax.

The move starts 2026-11-12: b1, then b2.
