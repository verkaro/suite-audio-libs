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

Written here, not moved:

- `crates/keep` (`as-keep`, 2026-10-29), after the way Dexter's export (`renameat2`'s
  `RENAME_NOREPLACE`), place's renders and scape's modules each kept from overwriting.
