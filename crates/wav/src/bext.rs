//! BWF's `bext` chunk (EBU Tech 3285): its fixed 602 bytes kept as they were, and its
//! CodingHistory, the lines that say what's been done to the sound, one appended for each step.

/// A `bext` chunk's body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bext {
    fixed: [u8; Bext::FIXED],
    /// The CodingHistory as it was, its trailing NULs gone: lines, each ended by CR LF.
    history: Vec<u8>,
}

impl Default for Bext {
    fn default() -> Bext {
        Bext::new()
    }
}

impl Bext {
    /// Description, originator, its reference, date, time, TimeReference, version, UMID,
    /// loudness and what's reserved: 602 bytes before the CodingHistory.
    pub const FIXED: usize = 602;
    const TIME_REFERENCE: usize = 338;

    /// An empty one: every field blank, TimeReference 0, version 0, no history. Nothing in it
    /// says when it was made.
    pub fn new() -> Bext {
        Bext { fixed: [0; Bext::FIXED], history: vec![] }
    }

    /// A chunk's body. One shorter than its fixed part is read as if padded with zeros.
    pub fn parse(body: &[u8]) -> Bext {
        let mut fixed = [0; Bext::FIXED];
        let n = body.len().min(Bext::FIXED);
        fixed[..n].copy_from_slice(&body[..n]);
        let mut history = body.get(Bext::FIXED..).unwrap_or(&[]).to_vec();
        while history.last() == Some(&0) {
            history.pop();
        }
        Bext { fixed, history }
    }

    /// The body, to write back.
    pub fn to_bytes(&self) -> Vec<u8> {
        [&self.fixed[..], &self.history].concat()
    }

    /// Where the sound starts on the session's timeline, in samples since midnight.
    pub fn time_reference(&self) -> u64 {
        let t = Bext::TIME_REFERENCE;
        u64::from_le_bytes(self.fixed[t..t + 8].try_into().unwrap())
    }

    pub fn set_time_reference(&mut self, at: u64) {
        let t = Bext::TIME_REFERENCE;
        self.fixed[t..t + 8].copy_from_slice(&at.to_le_bytes());
    }

    /// The CodingHistory's lines, without their endings.
    pub fn history(&self) -> Vec<String> {
        String::from_utf8_lossy(&self.history)
            .split(['\r', '\n'])
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// Adds a line to the CodingHistory, ended by CR LF; a last line left unended is ended
    /// first.
    pub fn push_history(&mut self, line: &str) {
        if !self.history.is_empty() && !self.history.ends_with(b"\r\n") {
            self.history.extend_from_slice(b"\r\n");
        }
        self.history.extend_from_slice(line.as_bytes());
        self.history.extend_from_slice(b"\r\n");
    }
}
