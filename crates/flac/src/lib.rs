//! FLAC written, 24-bit, with no dependencies (moved into the suite from dexter-foley's `foley-flac`, itself
//! place's, `VENDORED.md`):
//! FLAC's fixed predictors, each frame's cheapest of them with its residual
//! Rice-coded at the best parameter for each partition, or the samples as they are when that's
//! smaller; the samples' MD5 in STREAMINFO. Plain and sure rather than small: a sixth bigger
//! than `flac -8` on a noisy recording, half again on a sweep (it also tries LPC). Read back by `claxon` in its tests, and passed by
//! `flac -t`. The format: https://xiph.org/flac/format.html.

/// Samples a frame.
const BLOCK: usize = 4096;
const BITS: u32 = 24;

/// Bits written, most significant first.
struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn new() -> Bits {
        Bits { out: vec![], acc: 0, n: 0 }
    }

    /// The low `bits` of `v` (at most 32).
    fn put(&mut self, v: u64, bits: u32) {
        debug_assert!(bits <= 32);
        if bits == 0 {
            return;
        }
        self.acc = (self.acc << bits) | (v & ((1u64 << bits) - 1));
        self.n += bits;
        while self.n >= 8 {
            self.n -= 8;
            self.out.push((self.acc >> self.n) as u8);
        }
        self.acc &= (1u64 << self.n) - 1;
    }

    /// `q` zeros, then a one.
    fn unary(&mut self, mut q: u64) {
        while q >= 32 {
            self.put(0, 32);
            q -= 32;
        }
        self.put(1, q as u32 + 1);
    }

    /// Zeros to the next byte.
    fn align(&mut self) {
        if self.n > 0 {
            self.put(0, 8 - self.n);
        }
    }
}

fn crc8(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |mut c, b| {
        c ^= b;
        for _ in 0..8 {
            c = if c & 0x80 != 0 { (c << 1) ^ 0x07 } else { c << 1 };
        }
        c
    })
}

fn crc16(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0u16, |mut c, b| {
        c ^= (*b as u16) << 8;
        for _ in 0..8 {
            c = if c & 0x8000 != 0 { (c << 1) ^ 0x8005 } else { c << 1 };
        }
        c
    })
}

/// MD5 (RFC 1321), for STREAMINFO's signature of the samples, and for a file's checksum, a
/// piece at a time.
pub struct Md5 {
    h: [u32; 4],
    /// What's left over that doesn't fill a 64-byte block.
    part: Vec<u8>,
    len: u64,
}

impl Default for Md5 {
    fn default() -> Md5 {
        Md5 { h: [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476], part: Vec::with_capacity(64), len: 0 }
    }
}

impl Md5 {
    pub fn update(&mut self, mut data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);
        if !self.part.is_empty() {
            let n = (64 - self.part.len()).min(data.len());
            self.part.extend_from_slice(&data[..n]);
            data = &data[n..];
            if self.part.len() < 64 {
                return;
            }
            let block = std::mem::take(&mut self.part);
            self.block(&block);
            self.part = block;
            self.part.clear();
        }
        let (blocks, rest) = data.as_chunks::<64>();
        for b in blocks {
            self.block(b);
        }
        self.part.extend_from_slice(rest);
    }

    pub fn finish(mut self) -> [u8; 16] {
        let bits = self.len.wrapping_mul(8);
        let mut tail = vec![0x80u8];
        while (self.part.len() + tail.len()) % 64 != 56 {
            tail.push(0);
        }
        tail.extend_from_slice(&bits.to_le_bytes());
        let len = self.len;
        self.update(&tail);
        self.len = len;
        debug_assert!(self.part.is_empty());
        let mut out = [0u8; 16];
        for (i, v) in self.h.iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
        }
        out
    }

    fn block(&mut self, chunk: &[u8]) {
        const S: [u32; 64] = [
            7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14,
            20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6,
            10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
        ];
        // |sin(i + 1)| · 2³², RFC 1321's table.
        const K: [u32; 64] = [
            0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
            0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
            0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
            0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
            0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
            0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
            0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
            0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
        ];
        let m: Vec<u32> = chunk.chunks(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        let [mut a, mut b, mut c, mut d] = self.h;
        for (i, (s, k)) in S.iter().zip(K).enumerate() {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(*s));
        }
        for (x, y) in self.h.iter_mut().zip([a, b, c, d]) {
            *x = x.wrapping_add(y);
        }
    }
}

/// MD5 of `data` at once.
pub fn md5(data: &[u8]) -> [u8; 16] {
    let mut m = Md5::default();
    m.update(data);
    m.finish()
}

/// What STREAMINFO's MD5 is taken over: each frame's samples, interleaved, three bytes each,
/// little-endian.
pub fn samples_md5(channels: &[Vec<i32>]) -> [u8; 16] {
    let frames = channels.first().map_or(0, Vec::len);
    let mut m = Md5::default();
    let mut raw = Vec::with_capacity(4096 * channels.len() * 3);
    for i in 0..frames {
        for c in channels {
            raw.extend_from_slice(&c[i].to_le_bytes()[..3]);
        }
        if raw.len() >= 4096 * 3 {
            m.update(&raw);
            raw.clear();
        }
    }
    m.update(&raw);
    m.finish()
}

/// FLAC's fixed predictor of `order` (0 to 4): the residual after it, from sample `order` on.
fn residual(x: &[i64], order: usize) -> Vec<i64> {
    (order..x.len())
        .map(|i| match order {
            0 => x[i],
            1 => x[i] - x[i - 1],
            2 => x[i] - 2 * x[i - 1] + x[i - 2],
            3 => x[i] - 3 * x[i - 1] + 3 * x[i - 2] - x[i - 3],
            _ => x[i] - 4 * x[i - 1] + 6 * x[i - 2] - 4 * x[i - 3] + x[i - 4],
        })
        .collect()
}

fn zigzag(r: i64) -> u64 {
    ((r << 1) ^ (r >> 63)) as u64
}

/// The cheapest Rice parameter for `u`, and what it costs, bits.
fn rice(u: &[u64]) -> (u32, u64) {
    let sum: u64 = u.iter().sum();
    let mean = sum / u.len().max(1) as u64;
    let guess = 64 - mean.leading_zeros();
    (guess.saturating_sub(2)..=(guess + 1).min(30))
        .map(|k| (k, u.iter().map(|v| (v >> k) + 1 + k as u64).sum::<u64>()))
        .min_by_key(|(_, bits)| *bits)
        .unwrap_or((0, u64::MAX))
}

/// A residual's cheapest partitioning: (partition order, each partition's parameter, bits).
fn partitions(res: &[i64], n: usize, order: usize) -> (u32, Vec<u32>, u64) {
    let u: Vec<u64> = res.iter().map(|r| zigzag(*r)).collect();
    let mut best: Option<(u32, Vec<u32>, u64)> = None;
    for po in 0..=8u32 {
        let parts = 1usize << po;
        if !n.is_multiple_of(parts) || n / parts <= order {
            break;
        }
        let each = n / parts;
        let mut ks = vec![];
        let mut bits = 0u64;
        let mut at = 0;
        for p in 0..parts {
            let len = if p == 0 { each - order } else { each };
            let (k, b) = rice(&u[at..at + len]);
            ks.push(k);
            bits += b;
            at += len;
        }
        let param_bits = if ks.iter().any(|k| *k > 14) { 5 } else { 4 };
        bits += 2 + 4 + parts as u64 * param_bits;
        if best.as_ref().is_none_or(|b| bits < b.2) {
            best = Some((po, ks, bits));
        }
    }
    best.unwrap_or((0, vec![0], u64::MAX))
}

/// One channel's subframe.
fn subframe(w: &mut Bits, x: &[i64]) {
    let n = x.len();
    if x.iter().all(|v| *v == x[0]) {
        w.put(0, 1);
        w.put(0b000000, 6);
        w.put(0, 1);
        w.put(x[0] as u64, BITS);
        return;
    }
    let verbatim = n as u64 * BITS as u64;
    let best = (0..=4usize.min(n - 1))
        .map(|order| {
            let res = residual(x, order);
            let (po, ks, bits) = partitions(&res, n, order);
            (order, res, po, ks, bits + order as u64 * BITS as u64)
        })
        .min_by_key(|c| c.4);
    match best {
        Some((order, res, po, ks, bits)) if bits < verbatim => {
            w.put(0, 1);
            w.put(0b001000 | order as u64, 6);
            w.put(0, 1);
            for v in &x[..order] {
                w.put(*v as u64, BITS);
            }
            let wide = ks.iter().any(|k| *k > 14);
            w.put(if wide { 1 } else { 0 }, 2);
            w.put(po as u64, 4);
            let parts = 1usize << po;
            let each = n / parts;
            let mut at = 0;
            for (p, k) in ks.iter().enumerate() {
                let len = if p == 0 { each - order } else { each };
                w.put(*k as u64, if wide { 5 } else { 4 });
                for r in &res[at..at + len] {
                    let u = zigzag(*r);
                    w.unary(u >> k);
                    w.put(u, *k);
                }
                at += len;
            }
        }
        _ => {
            w.put(0, 1);
            w.put(0b000001, 6);
            w.put(0, 1);
            for v in x {
                w.put(*v as u64, BITS);
            }
        }
    }
}

/// The frame header's code for a rate, and what follows it.
fn rate_code(rate: u32) -> (u64, Option<(u64, u32)>) {
    match rate {
        88200 => (0b0001, None),
        176400 => (0b0010, None),
        192000 => (0b0011, None),
        8000 => (0b0100, None),
        16000 => (0b0101, None),
        22050 => (0b0110, None),
        24000 => (0b0111, None),
        32000 => (0b1000, None),
        44100 => (0b1001, None),
        48000 => (0b1010, None),
        96000 => (0b1011, None),
        r if r % 1000 == 0 && r / 1000 < 256 => (0b1100, Some((r as u64 / 1000, 8))),
        r if r < 65536 => (0b1101, Some((r as u64, 16))),
        r if r % 10 == 0 && r / 10 < 65536 => (0b1110, Some((r as u64 / 10, 16))),
        _ => (0b0000, None),
    }
}

/// A frame number, as FLAC codes it (UTF-8's way).
fn utf8(w: &mut Bits, v: u64) {
    if v < 0x80 {
        w.put(v, 8);
        return;
    }
    let extra = match v {
        _ if v < 0x800 => 1,
        _ if v < 0x1_0000 => 2,
        _ if v < 0x20_0000 => 3,
        _ if v < 0x400_0000 => 4,
        _ => 5,
    };
    let lead = (0xFF00u64 >> (extra + 1)) & 0xFF;
    w.put(lead | (v >> (6 * extra)), 8);
    for i in (0..extra).rev() {
        w.put(0x80 | ((v >> (6 * i)) & 0x3F), 8);
    }
}

/// `channels` (each from −2²³ to 2²³ − 1, all one length) as a 24-bit FLAC stream at `rate`.
pub fn encode(channels: &[Vec<i32>], rate: u32) -> Vec<u8> {
    let ch = channels.len();
    let frames = channels.first().map_or(0, Vec::len);
    let mut body: Vec<u8> = vec![];
    let (mut min_frame, mut max_frame) = (u32::MAX, 0u32);
    for (number, from) in (0..frames).step_by(BLOCK).enumerate() {
        let n = BLOCK.min(frames - from);
        let mut w = Bits::new();
        w.put(0b11_1111_1111_1110, 14);
        w.put(0, 1);
        w.put(0, 1); // fixed block size
        let size_code = if n == BLOCK {
            0b1100
        } else if n <= 256 {
            0b0110
        } else {
            0b0111
        };
        w.put(size_code, 4);
        let (rc, rate_more) = rate_code(rate);
        w.put(rc, 4);
        w.put(ch as u64 - 1, 4);
        w.put(0b110, 3); // 24 bits
        w.put(0, 1);
        utf8(&mut w, number as u64);
        match size_code {
            0b0110 => w.put(n as u64 - 1, 8),
            0b0111 => w.put(n as u64 - 1, 16),
            _ => {}
        }
        if let Some((v, bits)) = rate_more {
            w.put(v, bits);
        }
        let crc = crc8(&w.out);
        w.put(crc as u64, 8);
        for c in channels {
            let x: Vec<i64> = c[from..from + n].iter().map(|v| *v as i64).collect();
            subframe(&mut w, &x);
        }
        w.align();
        let crc = crc16(&w.out);
        w.put(crc as u64, 16);
        min_frame = min_frame.min(w.out.len() as u32);
        max_frame = max_frame.max(w.out.len() as u32);
        body.extend_from_slice(&w.out);
    }
    let mut w = Bits::new();
    w.out.extend_from_slice(b"fLaC");
    // STREAMINFO, the last metadata block.
    w.put(1, 1);
    w.put(0, 7);
    w.put(34, 24);
    let block = if frames <= BLOCK { frames.max(16) } else { BLOCK };
    w.put(block as u64, 16);
    w.put(block as u64, 16);
    w.put(if max_frame == 0 { 0 } else { min_frame as u64 }, 24);
    w.put(max_frame as u64, 24);
    w.put(rate as u64, 20);
    w.put(ch as u64 - 1, 3);
    w.put(BITS as u64 - 1, 5);
    w.put((frames as u64) >> 32, 4);
    w.put(frames as u64 & 0xFFFF_FFFF, 32);
    for b in samples_md5(channels) {
        w.put(b as u64, 8);
    }
    let mut out = w.out;
    out.extend_from_slice(&body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(bytes: &[u8]) -> (u32, Vec<Vec<i32>>) {
        let mut r = claxon::FlacReader::new(std::io::Cursor::new(bytes)).unwrap();
        let info = r.streaminfo();
        let ch = info.channels as usize;
        let mut out = vec![vec![]; ch];
        for (i, s) in r.samples().enumerate() {
            out[i % ch].push(s.unwrap());
        }
        assert_eq!(info.samples, Some(out[0].len() as u64));
        (info.sample_rate, out)
    }

    #[test]
    fn what_goes_in_comes_out() {
        let mut x = 5u64;
        let mut noise = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            ((x >> 40) as i64 - (1 << 23)) as i32
        };
        let full = (1 << 23) - 1;
        // Noise (verbatim), a tone (predicted), silence (constant), full scale both ways, a
        // short last block, and a lone sample.
        for (len, rate) in
            [(10_000, 48000), (4096 * 3, 44100), (5000, 96000), (1, 48000), (300, 22050), (7000, 12345)]
        {
            let a: Vec<i32> = (0..len).map(|_| noise()).collect();
            let b: Vec<i32> = (0..len)
                .map(|i| match i {
                    _ if i < len / 3 => 0,
                    _ if i == len / 2 => full,
                    _ if i == len / 2 + 1 => -full - 1,
                    _ => ((i as f64 * 0.01).sin() * 8_000_000.0) as i32,
                })
                .collect();
            let bytes = encode(&[a.clone(), b.clone()], rate);
            let (r, got) = decoded(&bytes);
            assert_eq!(r, rate);
            assert_eq!(got, [a, b], "{len} at {rate}");
        }
    }

    #[test]
    fn md5_is_rfc_1321s() {
        let hex = |d: [u8; 16]| d.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(hex(md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(
            hex(md5(b"The quick brown fox jumps over the lazy dog")),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
        assert_eq!(hex(md5(&[0x61; 64])), "014842d480b571495a4a0363793f7367");
        // In pieces that don't fall on its blocks.
        let data: Vec<u8> = (0..1000u32).map(|i| (i * 7) as u8).collect();
        for step in [1, 3, 63, 64, 65, 500] {
            let mut m = Md5::default();
            for piece in data.chunks(step) {
                m.update(piece);
            }
            assert_eq!(m.finish(), md5(&data), "{step}");
        }
    }

    #[test]
    fn a_tone_is_smaller_than_its_samples() {
        let x: Vec<i32> = (0..48000).map(|i| ((i as f64 * 0.05).sin() * 6_000_000.0) as i32).collect();
        let bytes = encode(&[x], 48000);
        assert!(bytes.len() < 48000 * 3 / 2, "{}", bytes.len());
    }
}
