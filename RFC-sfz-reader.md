# RFC: an SFZ read, and played

From dexter-like110's session, 2026-11-12, at Warren's asking. sfizz is erratic to install on
Debian, and like110's step 9 has Warren listen to the first bank as a sampler plays it: the decays,
the loops, the release after a held note, the level from key to key. So the family reads an SFZ
and plays it, written fresh. A rough plan, with notes; nothing built yet.

## What it is, and what it isn't

**An SFZ reader for the files the family writes, and a little more.** like110's banks use a dozen
opcodes. The reader takes those, the few a sampler applies whether a file says them or not
(velocity, mostly), and the headers they sit under. It isn't sfizz: no velocity layers, round
robins, filters, CCs, curves, `#define` or `#include`. A file that uses one is refused, with its
line and the opcode named, never half played.

**A voice that plays a region as a sampler would:** the key's pitch from the sample, round its
loop while held, through its release when let go, at its volume. Enough for an audition, and
enough for BRR or scape to build on, but no instrument host.

**What it can't prove.** Our player shows a bank is what we meant to write. It can't show that
sfizz or another sampler reads it the same: a misreading shared by our writer and our reader is
invisible here. Two things narrow that: the reader follows the SFZ spec's defaults, not
like110's habits, and its tests are hand-written files with the answer worked from the spec. A
bank is still played once in an outside sampler, wherever that's easy, before the library is
called done (Warren's gate, not ours).

## Where it goes

- **`as-sfz` gains the reader** (the text to `Sfz`, then to typed regions). One crate for the
  format, written and read, so a round trip can be pinned.
- **The voice in a new crate, `as-sampler`** (Question 1): regions, voices, the loop, the
  envelope, on `as-resample`'s `read` for the pitch. No JACK in it: it fills blocks it's given, as
  Tape's chain does, so it's tested without a server and any tool can host it.
- **like110 hosts it first**: an audition on its page (a key held, or several) and over MIDI
  through `as-rawmidi`, to JACK, connected nowhere until Warren picks. A tool of its own can come
  later, as a shell round the same crates, if another tool wants one.

## The reader

### The text

- Headers `<control>`, `<global>`, `<group>`, `<region>` (`<master>` too, since sfizz's files
  have it). Any other header is refused.
- Opcodes `name=value`, separated by whitespace or newlines. A value runs to the end of the line
  or to the next `name=`: `sample=` may hold spaces (`sample=02 Piano/060 C4.wav`), so its value
  runs to the next opcode, trimmed. Other values end at whitespace.
- Comments `//` to the line's end; `/* */` too (other writers use them). Blank lines anywhere.
- `\` in a path read as `/`. Paths relative to the file's folder, after `default_path`.
- `#define`, `#include`, `$` variables: refused, named.
- Text is UTF-8 read; our own writer stays ASCII.

`parse(text) -> Result<Sfz, Unread>` gives back the same `Sfz` the writer takes, comments
dropped (Question 3), so `parse(to_text(x))` is `x` without them, pinned on like110's own example
(spec 14.5) and on a bank from the simulated keyboard.

### The regions

`Sfz` to `Instrument`: each region with the opcodes of the headers above it, the nearest winning
(`region` over `group` over `master` over `global`), `control`'s `default_path` before each
`sample`. Typed, with the spec's defaults where a file says nothing:

| Opcode | Type, range | Default | like110 writes it |
|---|---|---|---|
| `sample` | path | (required) | region |
| `lokey`, `hikey` | 0..=127, or a note name (`c4`, `c#4`; C4 is 60) | 0, 127 | region |
| `key` | sets `lokey`, `hikey`, `pitch_keycenter` | | no |
| `pitch_keycenter` | 0..=127 or a name | 60 | region |
| `pitch_keytrack` | -1200..=1200 cents a key | 100 | global, kits (0) |
| `tune` | -100..=100 cents | 0 | region |
| `transpose` | -127..=127 keys | 0 | no |
| `volume` | -144..=6 dB | 0 | global |
| `amp_veltrack` | -100..=100 % | 100 | no (see note) |
| `lovel`, `hivel` | 1..=127 | 1, 127 | no |
| `offset`, `end` | frames | 0, the last | no |
| `loop_mode` | `no_loop`, `one_shot`, `loop_continuous`, `loop_sustain` | `no_loop`, or the file's own loop if it has one | region, or global for kits |
| `loop_start`, `loop_end` | frames, `loop_end` included | the file's loop, or none | region |
| `ampeg_attack`, `ampeg_release` | seconds, 0..=100 | 0, 0.001 | global and region |
| `ampeg_sustain` | 0..=100 % | 100 | no |

Any other opcode: refused, named, unless a list of ones to pass over is given (Question 2).
A region whose loop runs past its sample, or `loop_end` before `loop_start`, is refused, as is a
`sample` that doesn't open. Reading the WAVs is `as-wav`'s; their rate needn't be JACK's (below).

**A note on velocity.** No like110 file says `amp_veltrack`, so by the spec a sampler scales a
note's level by its velocity: at 100 % the common curve takes the gain as (velocity / 127)², so a
key played at 64 is about 6 dB down (sfizz's curve checked before step 1). The LK-110 has no velocity, and its samples are taken at one. Whether
like110 should write `amp_veltrack=0` changes what Warren hears in any sampler, so it's his
(Question 4). The player follows the spec either way.

## The voice

- **Which region:** the first whose keys and velocities hold the note, as sfizz picks when
  regions overlap (it plays all of them; we play all of them too, which for like110 is one).
- **The pitch:** speed = 2^((note − pitch_keycenter) × pitch_keytrack / 1200 + (tune + transpose
  × 100) / 1200) × the sample's rate / JACK's rate, read through `as_resample::read::Weights`.
  Its `MOST` is 4, two octaves: a region stretched further is refused at load, not read badly.
  like110's regions are one key each, so the speed is the tune and the rate alone.
- **The loop:** `loop_sustain` goes round while the note is held, and on from where it is through
  the end of the sample once it's let go; `loop_continuous` goes round through the release too;
  `no_loop` plays to the end; `one_shot` ignores the note's end and plays to the sample's. Across
  the seam the sinc reads frames both sides, so the voice reads a stream with the loop unrolled
  (the loop's frames on from `loop_end`, `TAPS` of them), made at load, never on JACK's thread.
- **The envelope:** attack, sustain, release as `ampeg_` says; the release from wherever the
  level is. Its shape is Question 5. Every other change of level is ramped (5 ms), so a note
  stolen or stopped never clicks.
- **Level:** `volume` in dB, then velocity by `amp_veltrack`.
- **Voices:** a fixed number, made at load (Question 6); a note past them takes the oldest,
  ramped out. Notes arrive on a lock-free SPSC ring. Nothing on JACK's thread allocates, locks or
  blocks; samples are loaded, converted to `f32` and unrolled before the instrument is handed over.
- **Repeatable:** the same instrument, notes and their frames render the same, bit for bit, at any
  block size. Tests pin it.

## Tests

Synthetic sounds only (`as-envelope`'s, and like110's simulated keyboard):

- **Reading:** each row of the table from a hand-written file, its default and its range;
  inheritance through every header; note names; `sample=` with spaces; `/* */`; every refusal,
  with the line it names. like110's example (spec 14.5) read back to the same regions.
- **Pitch:** a sine at 440 Hz in a region centred on 69, played at 72 and measured with
  `as-pitch`: 523.25 Hz within a cent. With `tune=-50`, a quarter tone flat. With
  `pitch_keytrack=0` (a kit), the same pitch on every key. A 44.1 kHz sample at 48 kHz, in tune.
- **The loop:** a looped sine held for 10 s, the seam's sameness (`as_seam::sameness`) as the
  sample's own; `loop_sustain` let go plays the tail; `one_shot` ignores the note's end.
- **The release:** the level after `ampeg_release` seconds is what Question 5's shape says, within
  0.5 dB, from a note let go during attack and during sustain.
- **Velocity:** with `amp_veltrack` at 100 and 0, the level at 127, 64 and 1 as the spec's curve
  says.
- **Bit for bit** at blocks of 1, 64, 512 and 1000 frames, and with voices stolen.
- **No allocation** on the render path: a counting allocator in the test (new to the suite).

## Questions for Warren

1. **The voice in its own crate, `as-sampler`**, or in like110 until a second tool wants it?
   *Recommended: `as-sampler`.* BRR plays samples tuned already, and scape is one; the voice is
   what they'd share, and the suite's rule is a crate as soon as two will.
2. **An opcode we don't know: refuse, or pass over and say so?** *Recommended: refuse* for like110's
   own banks (a file we wrote with an opcode we can't play is a bug), with a flag to pass over and
   list them for a file from elsewhere.
3. **Comments read back,** so a bank's first lines (its tone, its date) show on the page?
   *Recommended: yes,* kept in `Sfz.comments` as the writer has them.
4. **Should like110 write `amp_veltrack=0`?** Its keys have one velocity; without it a sampler
   plays them softer the softer the key's struck, which the LK-110 never did. *Recommended: yes,*
   in `<global>`, written into banks already taken by `--sfz`. It changes what Warren hears in any
   sampler, so it's his.
5. **The release's shape.** Samplers differ: some fall linearly, some exponentially (a straight
   line in dB). *Recommended: as sfizz does,* read from its source before step 2 and written down
   here, since it's what Warren would otherwise have heard. A sustained key's release is its
   sample's own tail and this envelope over it, so it may shape twice (`docs/design.md` in like110,
   "The release after a loop"): this is where Warren hears whether it does.
6. **How many voices?** *Recommended: 32,* plenty for an audition and a few held chords.

## Steps

1. `as-sfz`'s reader and `Instrument`, with its tests and a `CHANGES.md` line. No sound.
2. `as-sampler`: the voice, the loop, the envelope, the voices, rendered offline in tests.
3. like110's audition: the page's keys and MIDI in, through JACK, connected nowhere; the page
   protocol moved on, its document and golden test with it.
4. **Warren listens** to a bank from the simulated keyboard first, then his first real bank:
   step 9's gate in like110.
