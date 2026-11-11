# RFC: as-jackd, jackd started, watched, found and stopped

From the family's session, 2026-11-11, for jackbay (`../Jackbay.md`, step a) and Dexter's step 8
(`Dexter.md`). Read from Dexter's `main` at `e98d44f`, with nothing run and nothing moved. Dexter
isn't touched: its copy stays its own until its step 8, and this waits for Warren's go-ahead.

## What's there

`dexter/crates/jack/src/jackd.rs`, 558 lines with its tests, does six things:

| | What | Tied to Dexter by |
|---|---|---|
| Start | setup commands, the realtime warning, jackd in its own session (`setsid`), its output appended to a log after a dated `===` marker, waiting up to 10 s for it to answer | `Config` (`jack`, `interface`, `setup`), `dexter_system::resolve` for the interface, `Server::open("dexter-start")` |
| Find | the jackd serving `JACK_DEFAULT_SERVER` from `/proc`, its command line read for the server name, driver, ALSA device, `-I`/`-O`; zombies seen; a start's ticks | nothing |
| Watch | `Monitor`: a client with no ports counting xruns and noticing shutdown, the DSP load, the clients; `watch` prints the log's new lines, xruns and clients coming and going | `client::Server`, `Notify`, `rt::Control`; the hint "dexter-engine --jack-stop", hiding `dexter-` clients |
| Log | `log_path` (`$XDG_STATE_HOME/dexter/jackd.log`), `run_start` (this run's marker), `noise`, `tail` | `dexter_localtime`, Dexter's folder |
| Stop | SIGTERM to the pid found, waiting 5 s; refused while the engine's client is attached | `engine::CLIENT_NAME` |
| Dummy | `without_interface`: the project's settings on the dummy driver when the interface isn't there | `Config::on_dummy`, which counts the tracks |

The settings themselves are `dexter_project::JackConfig` and `jackd_args` (`config.rs`), and the
interfaces are `dexter_system`'s `interfaces.rs` (200 lines). The tests' dummy server is written
again in every tool: three times in the suite (`as-jack-keeper`, `as-transport`,
`as-jack-capture`), at least three in Dexter, and once each in BRR, Tape, irma and ducky.

## The proposal

**`as-jackd`**, moved from Dexter, the caller saying everything that was Dexter's:

- **`Settings`:** `JackConfig`'s fields as they are, no serde, with `args(server)`, Dexter's
  `jackd_args`, and `on_dummy(captures, playbacks)` for the dummy driver. Counting the tracks
  stays Dexter's. Dexter can re-export it as `JackConfig` at its step 8, so nothing else there
  changes. Reading it from YAML stays Dexter's until step 10 settles the rig.
- **The server's name is passed, not read.** Every function takes the name; `server_name()` reads
  `JACK_DEFAULT_SERVER` (else `default`) for a caller that wants it. A test can then never reach
  `default` by forgetting the variable, and two tests in one process don't race on it.
- **`start(&Start, log, out)`:** `Start` is the settings, the interface's id (or none) and the
  setup commands. The probe's client name is the caller's. Unchanged: the warning, the setup
  commands' failures only warned, `setsid`, the marker, the 10 s, SIGTERM to the jackd it started
  if it never answered, the thread that reaps it.
- **Finding, unchanged:** `find`, `running_io`, `running_driver`, `running_alsa_device`,
  `started`, and the private `zombie`, `gone`, `server_name`, `driver`, `alsa_device`.
- **`Monitor`,** written over a small notification handler of its own (atomics only: the
  xruns, the shutdown, a graph count), not Dexter's engine client. `Follow` reads the log's new
  lines without the noise, a partial line waiting for the next round. Dexter's `watch`, its text
  and its hint, stays Dexter's and is built on the two at its step 8. jackbay's server wants
  events, not text.
- **`stop(server, refuse)`:** refused while any client named in `refuse` is attached, unless
  forced. Dexter passes its engine's name. jackbay passes the same (Question 4).
- **`Dummy`:** the tests' server, `jackd -n <tool>test-<pid> -r -d dummy -C 16 -P 2`, waited for,
  stopped with SIGTERM by its own pid on drop, never the user's. Each tool moves onto it at its
  own pace, when it wants. A `CHANGES.md` line offers it.
- **The log:** `log_path()`, `run_start`, `tail`, `noise`, the marker's format kept, so a log
  Dexter wrote reads the same (Question 3).

**What stays Dexter's:** the YAML, the rig and `Config` (step 10), counting a project's tracks for
the dummy driver, its engine's client, its `watch` text, the `LAT` text, and where its other logs
go.

**Two more moved with it,** each small, needed by `as-jackd`, and wanted elsewhere:

- **`as-localtime`:** `dexter-localtime` (149 lines, libc only), for the marker. The suite already
  calls `localtime_r` for itself in `as-focusrite` and `as-transport`, which can move onto it.
- **`as-interfaces`:** `dexter_system`'s `interfaces.rs`, finding the interfaces by what they are
  (`/dev/snd/by-id`, their rates and channels), read under a root the tests fake, and
  `realtime_allowed` with it (Question 1).

Each gets its header, README entry, `VENDORED.md` entry (from Dexter at `e98d44f`), a line in
`[workspace.dependencies]`, its tests moved with it, and its `CHANGES.md` line. Dexter's
`Dexter.md` step 8 then says the suite's half is done.

## What proves it

- The unit tests that move with it, unchanged but for imports and the name passed in.
- `SUITE_JACK_TEST=1`, on `Dummy` (`asjackdtest-<pid>`): started from `Settings` on the dummy
  driver; found, with its driver and no ALSA device; `Monitor` reads its rate and period and sees
  a client join and leave; `stop` refused with a named client attached and done without one;
  gone after, and nothing left running. A start that fails (a driver that isn't there) returns the
  log's tail, not a hang.
- `as-interfaces` on a faked `/dev/snd` and `/proc`, as Dexter's tests have them.
- Dexter's own suites pass untouched, since nothing of Dexter's changes.

## Questions for Warren

Each has a recommendation, which the work goes on by once he gives the go-ahead.

1. **The interfaces in a crate of their own,** `as-interfaces`, with no JACK in it? Recommended:
   yes. focusrite-ctrl never links JACK, and could then name its card by the same id a rig keeps.
2. **The settings' name.** `Settings` in the suite, Dexter re-exporting it as `JackConfig`?
   Recommended: yes. The fields and `dexter.yaml` don't change.
3. **One jackd log for both,** at `$XDG_STATE_HOME/jack/jackd.log` (`as_jackd::log_path`), so
   whoever started JACK, the other finds this run's lines? Recommended: yes. jackbay uses it from
   the start. Dexter moves to it at its step 8, its old log left where it is, and its own logs
   (`engine.log`, `deck.log`, `surface.log`) stay in `dexter/`.
4. **"The deck has JACK"** (Jackbay.md, Settled 2) means the engine's client, `dexter`, is in the
   graph? Recommended: yes. A deck server with no engine (`--attach`) holds no audio, so jackbay
   may start and stop JACK then, asked as always. Dexter's own stop refuses on the same name, so
   the two agree.
