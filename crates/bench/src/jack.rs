//! The bench's JACK client: two outputs and two inputs, playing a signal and recording what
//! comes back, starting both in the same cycle so the round trip is fixed for the capture.
//!
//! Opened on the server `JACK_DEFAULT_SERVER` names, never starting jackd and never opening a
//! card (`as-jack-client`). **Its ports are connected nowhere by it:** Warren patches them, in
//! jackbay or Dexter's bay, and a test reads this file for the calls that would.
//!
//! The signal and the record buffer are made whole before a capture, and go to JACK's thread in
//! a box through a ring; there it copies out, copies in and counts, and when the buffer is full
//! the box comes back through a second ring, to be kept off JACK's thread. Nothing there
//! allocates, locks, blocks or makes a syscall. Between captures the outputs are silent.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use as_jack_client::{Counts, Notices, Notify};
use jack::{AudioIn, AudioOut, Control, Port, ProcessScope};

use crate::check::Capture;
use crate::signal::Signal;

/// The bench's ports.
pub const OUT: [&str; 2] = ["bench_out_L", "bench_out_R"];
pub const IN: [&str; 2] = ["bench_in_L", "bench_in_R"];

#[derive(Default)]
struct Status {
    xruns: AtomicU64,
    rate: AtomicU32,
}

impl Counts for Status {
    fn xrun(&self) {
        self.xruns.fetch_add(1, Ordering::Relaxed);
    }
    fn rate(&self, rate: u32) {
        self.rate.store(rate, Ordering::Relaxed);
    }
}

/// One capture on its way through JACK's thread.
struct Job {
    /// Which capture it is, so one that ran out of time and came back late is never taken for the
    /// next.
    seq: u64,
    out: Vec<f32>,
    rec: [Vec<f32>; 2],
    pos: usize,
}

/// The bench's client, open.
pub struct Bench {
    active: Option<jack::AsyncClient<Notify<Status>, Handler>>,
    notices: Arc<Notices<Status>>,
    jobs: rtrb::Producer<Box<Job>>,
    done: rtrb::Consumer<Box<Job>>,
    rate: u32,
    seq: u64,
}

impl Bench {
    /// Opens the client `name` with its four ports, connected nowhere. `note` hears what's worth
    /// saying (memory that couldn't be locked).
    pub fn open(name: &str, note: impl FnOnce(&str)) -> Result<Bench, String> {
        let client = as_jack_client::open(name)?;
        let reg_out = |n: &str| client.register_port(n, AudioOut::default()).map_err(|e| format!("{n}: {e}"));
        let out = [reg_out(OUT[0])?, reg_out(OUT[1])?];
        let reg_in = |n: &str| client.register_port(n, AudioIn::default()).map_err(|e| format!("{n}: {e}"));
        let ins = [reg_in(IN[0])?, reg_in(IN[1])?];
        let (jobs, take) = rtrb::RingBuffer::new(1);
        let (give, done) = rtrb::RingBuffer::new(1);
        let rate = client.sample_rate();
        let notices = Notices::new(Arc::new(Status::default()));
        notices.status.rate.store(rate, Ordering::Relaxed);
        as_jack_client::lock_memory(note);
        let handler = Handler { out, ins, take, give, job: None };
        let active = client
            .activate_async(Notify(notices.clone()), handler)
            .map_err(|e| format!("can't activate the client: {e}"))?;
        Ok(Bench { active: Some(active), notices, jobs, done, rate, seq: 0 })
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// xruns since it opened.
    pub fn xruns(&self) -> u64 {
        self.notices.status.xruns.load(Ordering::Relaxed)
    }

    /// Plays `s` on both outputs and records both inputs, from the same cycle, for the signal's
    /// length and a second more; waits for it, up to that and `slack`. Returns each input's
    /// capture, with the xruns while it played. `Err` if JACK went away, the rate isn't the
    /// signal's, or it didn't finish.
    pub fn capture(&mut self, s: &Signal, slack: Duration) -> Result<[Capture; 2], String> {
        if self.notices.shut.load(Ordering::Acquire) {
            return Err("JACK went away: open the bench again".into());
        }
        let rate = self.notices.status.rate.load(Ordering::Relaxed);
        if rate != s.rate {
            return Err(format!("JACK runs at {rate} Hz, the signal was made at {} Hz", s.rate));
        }
        let len = s.samples.len() + s.rate as usize;
        self.seq += 1;
        // Every page written now: the first touch of a page mustn't be on JACK's thread.
        let mut out = as_conv::touched::<f32>(s.samples.len());
        for (o, v) in out.iter_mut().zip(&s.samples) {
            *o = *v as f32;
        }
        let job =
            Box::new(Job { seq: self.seq, out, rec: [as_conv::touched(len), as_conv::touched(len)], pos: 0 });
        let before = self.xruns();
        self.jobs.push(job).map_err(|_| "a capture is already playing".to_string())?;
        let until = Instant::now() + Duration::from_secs_f64(len as f64 / s.rate as f64) + slack;
        let job = loop {
            match self.done.pop() {
                // One that ran out of time before, back late: not this one.
                Ok(j) if j.seq != self.seq => continue,
                Ok(j) => break j,
                Err(_) => {}
            }
            if self.notices.shut.load(Ordering::Acquire) {
                return Err("JACK went away during the capture".into());
            }
            if Instant::now() > until {
                return Err("the capture didn't finish: is JACK running?".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let xruns = self.xruns() - before;
        let [l, r] = job.rec;
        Ok([l, r].map(|rec| Capture {
            rate: s.rate,
            samples: rec.iter().map(|v| *v as f64).collect(),
            xruns,
        }))
    }

    /// Closes the client.
    pub fn close(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        if let Some(a) = self.active.take() {
            if self.notices.shut.load(Ordering::Acquire) {
                // The server is gone: closing would wait on it.
                std::mem::forget(a);
            } else if let Ok((c, _, _)) = a.deactivate() {
                drop(c);
            }
        }
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        self.halt();
    }
}

/// The process callback.
struct Handler {
    out: [Port<AudioOut>; 2],
    ins: [Port<AudioIn>; 2],
    take: rtrb::Consumer<Box<Job>>,
    give: rtrb::Producer<Box<Job>>,
    job: Option<Box<Job>>,
}

impl jack::ProcessHandler for Handler {
    fn process(&mut self, _: &jack::Client, ps: &ProcessScope) -> Control {
        if self.job.is_none() {
            self.job = self.take.pop().ok();
        }
        let [ol, or] = &mut self.out;
        let (ol, or) = (ol.as_mut_slice(ps), or.as_mut_slice(ps));
        let (il, ir) = (self.ins[0].as_slice(ps), self.ins[1].as_slice(ps));
        match self.job.as_mut() {
            None => {
                ol.fill(0.0);
                or.fill(0.0);
            }
            Some(j) => {
                let len = j.rec[0].len();
                for i in 0..ol.len() {
                    let p = j.pos + i;
                    let v = j.out.get(p).copied().unwrap_or(0.0);
                    ol[i] = v;
                    or[i] = v;
                    if p < len {
                        j.rec[0][p] = il[i];
                        j.rec[1][p] = ir[i];
                    }
                }
                j.pos += ol.len();
                if j.pos >= len {
                    // Back off JACK's thread; if the ring's full it waits a cycle, in silence.
                    if let Some(j) = self.job.take()
                        && let Err(rtrb::PushError::Full(j)) = self.give.push(j)
                    {
                        self.job = Some(j);
                    }
                }
            }
        }
        Control::Continue
    }
}
