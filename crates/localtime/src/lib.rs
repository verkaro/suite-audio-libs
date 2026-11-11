//! Local wall-clock time, in the formats Dexter writes in its files: `2006-01-02 15:04:05` in
//! `segments.txt`, `info.txt`, `dexter.log` and jackd's log, and `15:04:05` on the terminal and in
//! `markers.txt`. The standard library has no time zones, so this uses the C library's, which
//! follow `TZ` and `/etc/localtime`.
//!
//! Moved in from Dexter's `dexter-localtime` at `e98d44f` (2026-11-11), for `as-jackd`'s log
//! marker (`RFC-as-jackd.md`). As it was.

use std::time::{SystemTime, UNIX_EPOCH};

/// A moment on the local clock, to the second.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Local {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
}

impl Local {
    /// The local time at t.
    pub fn at(t: SystemTime) -> Local {
        let secs = match t.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs() as libc::time_t,
            Err(e) => -(e.duration().as_secs_f64().ceil() as libc::time_t),
        };
        // SAFETY: localtime_r only writes the tm it's given.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&secs, &mut tm) };
        Local {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u32,
            day: tm.tm_mday as u32,
            hour: tm.tm_hour as u32,
            min: tm.tm_min as u32,
            sec: tm.tm_sec as u32,
        }
    }

    pub fn now() -> Local {
        Local::at(SystemTime::now())
    }

    /// `2006-01-02 15:04:05`.
    pub fn date_time(&self) -> String {
        format!("{:04}-{:02}-{:02} {}", self.year, self.month, self.day, self.time())
    }

    /// `15:04:05`.
    pub fn time(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.min, self.sec)
    }

    /// Reads `2006-01-02 15:04:05`, in local time.
    pub fn parse_date_time(s: &str) -> Option<Local> {
        let (date, time) = s.split_once(' ')?;
        let d: Vec<&str> = date.split('-').collect();
        let t: Vec<&str> = time.split(':').collect();
        let widths_ok = d.len() == 3
            && t.len() == 3
            && d[0].len() == 4
            && [d[1], d[2], t[0], t[1], t[2]].iter().all(|f| f.len() == 2);
        if !widths_ok || !s.bytes().all(|b| b.is_ascii_digit() || b == b'-' || b == b':' || b == b' ') {
            return None;
        }
        let n = |f: &str| f.parse::<u32>().ok();
        let l = Local {
            year: d[0].parse().ok()?,
            month: n(d[1])?,
            day: n(d[2])?,
            hour: n(t[0])?,
            min: n(t[1])?,
            sec: n(t[2])?,
        };
        let days = match l.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if l.year % 4 == 0 && (l.year % 100 != 0 || l.year % 400 == 0) => 29,
            2 => 28,
            _ => return None,
        };
        (l.day >= 1 && l.day <= days && l.hour < 24 && l.min < 60 && l.sec < 60).then_some(l)
    }

    /// The moment this local time names. In the hour repeated when clocks go back, the C library
    /// picks one of the two.
    pub fn to_system_time(&self) -> SystemTime {
        // SAFETY: mktime only reads and normalises the tm it's given.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        tm.tm_year = self.year - 1900;
        tm.tm_mon = self.month as i32 - 1;
        tm.tm_mday = self.day as i32;
        tm.tm_hour = self.hour as i32;
        tm.tm_min = self.min as i32;
        tm.tm_sec = self.sec as i32;
        tm.tm_isdst = -1;
        let secs = unsafe { libc::mktime(&mut tm) };
        if secs >= 0 {
            UNIX_EPOCH + std::time::Duration::from_secs(secs as u64)
        } else {
            UNIX_EPOCH - std::time::Duration::from_secs(secs.unsigned_abs())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn formats() {
        let l = Local { year: 2026, month: 9, day: 5, hour: 7, min: 3, sec: 9 };
        assert_eq!(l.date_time(), "2026-09-05 07:03:09");
        assert_eq!(l.time(), "07:03:09");
    }

    #[test]
    fn back_to_a_moment() {
        let now = SystemTime::now();
        let l = Local::at(now);
        let t = l.to_system_time();
        assert!(now.duration_since(t).unwrap() < Duration::from_secs(1));
        assert_eq!(Local::at(t), l);
    }

    #[test]
    fn round_trip() {
        let now = SystemTime::now();
        let l = Local::at(now);
        assert_eq!(Local::parse_date_time(&l.date_time()), Some(l));
        assert_eq!(Local::at(UNIX_EPOCH + Duration::from_secs(86_400 * 400)).year, 1971);
    }

    #[test]
    fn parse_refuses() {
        for bad in [
            "",
            "2026-09-25",
            "2026-9-25 10:00:00",
            "2026-02-30 10:00:00",
            "2026-09-25 24:00:00",
            "2026-09-25T10:00:00",
            "2026-09-25 10:00:0x",
        ] {
            assert_eq!(Local::parse_date_time(bad), None, "{bad:?}");
        }
        assert!(Local::parse_date_time("2024-02-29 23:59:59").is_some());
    }
}
