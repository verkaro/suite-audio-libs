# Moved in from the siblings

What's below lives here now and is changed here. The siblings it came from keep their own
copies until they move over to the suite, each in its own session, through a handoff.

From `dexter-foley` at `9e1ea2226362e43f786c1d781d510e536fccb0a2` (2026-10-29), with `git
archive`, for BRR's step 1:

- `crates/wav` (`foley-wav`, now `as-wav`), with its tests and `testdata/`: renamed, its first
  comment and the tests' scratch name changed; as it was otherwise. foley had it from place,
  place from scape, scape from Dexter.
- `crates/flac` (`foley-flac`, now `as-flac`), with its tests: renamed, its first comment
  changed. foley had it from place.
- `crates/rate` (`foley-rate`, now `as-resample`, in `crates/resample`), with its tests and
  README: renamed, its first comment and the README's "Taking it" changed.
- `rustfmt.toml` and `.gitignore`, as they were.

From `dexter-scape` at `07ae205311e62ff153564bcca97816dc7649670a` (2026-10-29), with `git
show`, for BRR's step 2:

- `crates/core/src/seam.rs`, now `as-seam` (`crates/seam`): a sound is its channels, `&[Vec<f32>]`,
  where scape had its `Sample`; as it was otherwise. Its tests, from scape's
  `crates/core/src/tests.rs`, on synthetic noise and tones.
- `crates/core/src/tilt.rs`, now `as-tilt` (`crates/tilt`): its glide, `GLIDE`, a constant of its
  own (512 frames, scape's DECLICK). Its tests, on the filter alone, and one more: the same at any
  block size.
- `draw` and `mix`, from `crates/core/src/lib.rs`, now `as-draw` (`crates/draw`): what's drawn is
  a number, where scape had its `What`; the layer's seed is `own_seed`; `spread` as scape's;
  `between` and `pick` (never the same twice running) written here.

From `dexter-place` at `784f1ba6ab09b492e68bde1aca56f5b724189664` (2026-10-28), with `git show`,
for BRR's step 4:

- `crates/core/src/conv.rs`, now `as-conv` (`crates/conv`): `touched` made public, for the
  callers' own buffers; as it was otherwise. Its tests, from place's `crates/core/src/tests.rs`.
- `crates/core/src/opening.rs`, now `as-opening` (`crates/opening`): as it was. Its tests, from
  place's, and one more: between two stops, each route theirs crossfaded.

Written here, not moved:

- `as-flac`'s `read` (2026-10-29, at BRR's code review): FLAC read through `claxon`, whole,
  as floats; a stream cut short keeps its whole frames.
- `crates/ir` (`as-ir`, 2026-10-29): true stereo folded, unit energy, the onset, and the echoes
  without the direct sound, after place's `crates/library/src/ir.rs`, `numbers.rs`'s `onset`
  and its design's "The outside's space".

- `crates/keep` (`as-keep`, 2026-10-29), after the way Dexter's export (`renameat2`'s
  `RENAME_NOREPLACE`), place's renders and scape's modules each kept from overwriting.
