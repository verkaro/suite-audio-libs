use super::*;
use std::fs;
use std::path::PathBuf;

/// A directory of its own for each test, removed when it passes.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let d = std::env::temp_dir().join(format!("as-wav-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        TempDir(d)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn round_trip() {
    let dir = TempDir::new("round_trip");
    let input = [0.0f32, 0.5, -0.5, 0.25, -1.0, 0.999];
    for format in [Format::F32, Format::S24, Format::S16] {
        let path = dir.path(&format!("{format:?}.wav"));
        let mut w = Writer::create(&path, 48000, format).unwrap();
        w.write(&input).unwrap();
        w.write_silence(3).unwrap();
        assert_eq!(w.frames(), 9);
        w.close().unwrap();
        let mut r = Reader::open(&path).unwrap();
        assert_eq!((r.frames(), r.rate()), (9, 48000), "{format}");
        let mut out = [1.0f32; 11];
        r.read(0, &mut out);
        // The dither moves a 16-bit sample by up to one step.
        let tol = if format == Format::S16 { 1.0 / 32768.0 } else { 1e-6 };
        for (i, v) in input.iter().enumerate() {
            assert!((out[i] - v).abs() <= tol, "{format}: sample {i} = {}, want {v}", out[i]);
        }
        assert!(out[6..].iter().all(|&v| v == 0.0), "{format}: past the audio: {:?}", &out[6..]);
    }
}

#[test]
fn reads_from_a_position() {
    let dir = TempDir::new("position");
    let path = dir.path("a.wav");
    let mut w = Writer::create(&path, 48000, Format::F32).unwrap();
    w.write(&[0.1, 0.2, 0.3, 0.4]).unwrap();
    w.close().unwrap();
    let mut r = Reader::open(&path).unwrap();
    let mut out = [9.0f32; 3];
    r.read(2, &mut out);
    assert_eq!(out, [0.3, 0.4, 0.0]);
    r.read(4, &mut out);
    assert_eq!(out, [0.0; 3]);
    out = [9.0; 3];
    r.read(-1, &mut out);
    assert_eq!(out, [0.0; 3]);
}

#[test]
fn readable_without_close() {
    let dir = TempDir::new("without_close");
    let path = dir.path("a.wav");
    let mut w = Writer::create(&path, 48000, Format::F32).unwrap();
    w.write(&[0.0; 1000]).unwrap();
    w.sync(false).unwrap();
    w.write(&[0.0; 500]).unwrap(); // buffered, never synced: lost in the "crash"
    let r = Reader::open(&path).unwrap();
    assert_eq!(r.frames(), 1000, "frames after a crash: want the 1000 synced");
    std::mem::forget(w); // a crash doesn't run Drop
}

#[test]
fn drop_syncs() {
    let dir = TempDir::new("drop");
    let path = dir.path("a.wav");
    let mut w = Writer::create(&path, 48000, Format::S24).unwrap();
    w.write(&[0.5; 7]).unwrap();
    drop(w);
    let r = Reader::open(&path).unwrap();
    assert_eq!(r.frames(), 7);
    // Not padded: only close pads.
    assert_eq!(fs::metadata(&path).unwrap().len(), 44 + 21);
}

#[test]
fn a_file_cut_short_reads_to_its_end() {
    // A crash after the header was synced but before all the data reached the disk.
    let dir = TempDir::new("cut_short");
    let path = dir.path("a.wav");
    let mut w = Writer::create(&path, 48000, Format::S16).unwrap();
    w.write(&[0.25; 100]).unwrap();
    w.close().unwrap();
    let f = OpenOptions::new().write(true).open(&path).unwrap();
    f.set_len(44 + 2 * 60 + 1).unwrap(); // 60 frames and half of one more
    let mut r = Reader::open(&path).unwrap();
    assert_eq!(r.frames(), 60);
    let mut out = [0.0f32; 61];
    r.read(0, &mut out);
    assert!((out[59] - 0.25).abs() <= 1.0 / 32768.0);
    assert_eq!(out[60], 0.0);
}

#[test]
fn never_overwrites() {
    let dir = TempDir::new("overwrite");
    let path = dir.path("a.wav");
    fs::write(&path, "keep").unwrap();
    let e = Writer::create(&path, 48000, Format::F32).err().expect("create overwrote a file");
    assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&path).unwrap(), b"keep");
}

#[test]
fn empty_is_readable() {
    let dir = TempDir::new("empty");
    let path = dir.path("a.wav");
    let w = Writer::create(&path, 48000, Format::S24).unwrap();
    // Before anything is written or synced, a new file reads as empty.
    let r = Reader::open(&path).unwrap();
    assert_eq!(r.frames(), 0);
    w.close().unwrap();
}

/// Every 24-bit sample JACK can deliver from a 24-bit converter (n / 2^23) must be written back
/// as n exactly: the file holds the converter's data.
#[test]
fn s24_is_bit_exact() {
    let dir = TempDir::new("s24_exact");
    let path = dir.path("a.wav");
    let mut w = Writer::create(&path, 48000, Format::S24).unwrap();
    const N: i32 = 1 << 24;
    let mut buf = Vec::with_capacity(1 << 16);
    for i in 0..N {
        buf.push((i - N / 2) as f32 / 8_388_608.0);
        if buf.len() == buf.capacity() {
            w.write(&buf).unwrap();
            buf.clear();
        }
    }
    w.write(&[1.5, -1.5]).unwrap(); // past full scale: clips
    w.close().unwrap();
    let data = fs::read(&path).unwrap();
    let pcm = &data[data.len() - (N as usize + 2) * 3..];
    for (i, b) in pcm.as_chunks::<3>().0.iter().enumerate() {
        let got = i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8;
        let want = match i as i32 {
            N => 8_388_607,
            n if n == N + 1 => -8_388_608,
            n => n - N / 2,
        };
        assert_eq!(got, want, "sample {i}");
    }
}

/// A steady signal smaller than a 16-bit step would round to silence; with the dither it
/// survives as the right average level.
#[test]
fn s16_dither() {
    let dir = TempDir::new("s16_dither");
    let path = dir.path("a.wav");
    let mut w = Writer::create(&path, 48000, Format::S16).unwrap();
    const N: usize = 100_000;
    w.write(&vec![0.3 / 32768.0; N]).unwrap();
    w.close().unwrap();
    let mut r = Reader::open(&path).unwrap();
    let mut out = vec![0.0f32; N];
    r.read(0, &mut out);
    let avg = out.iter().map(|&v| v as f64 * 32768.0).sum::<f64>() / N as f64;
    assert!((0.27..=0.33).contains(&avg), "average {avg:.3} steps, want 0.3");
}

/// The signal in the reference files: edges, full scale and past it, and a ramp.
fn signal() -> Vec<f32> {
    let mut s = vec![
        0.0f32,
        0.5,
        -0.5,
        0.25,
        -1.0,
        0.999,
        1.0,
        1.5,
        -1.5,
        1.0 / 8_388_608.0,
        -1.0 / 8_388_608.0,
        0.3 / 32768.0,
        -0.7 / 32768.0,
    ];
    s.extend((0..1000).map(|i| (i % 200 - 100) as f32 / 97.0));
    s
}

/// The files are the reference files in `testdata/`, byte for byte: headers, conversion, the
/// dither's sequence and the padding. After a change that's meant, rewrite them with
/// `BLESS=1 cargo test -p as-wav reference_files`.
#[test]
fn reference_files() {
    let dir = TempDir::new("reference");
    let bless = std::env::var_os("BLESS").is_some();
    let testdata = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
    for (name, format, rate) in [
        ("s24.wav", Format::S24, 48000),
        ("s16.wav", Format::S16, 44100),
        ("f32.wav", Format::F32, 96000),
        ("s24-odd.wav", Format::S24, 48000),
    ] {
        let mut s = signal();
        if name == "s24-odd.wav" {
            s.truncate(7);
        }
        let path = dir.path(name);
        let mut w = Writer::create(&path, rate, format).unwrap();
        w.write(&s[..5]).unwrap();
        w.write_silence(4).unwrap();
        w.write(&s[5..]).unwrap();
        w.close().unwrap();
        if bless {
            fs::copy(&path, testdata.join(name)).unwrap();
        }
        let (got, want) = (fs::read(&path).unwrap(), fs::read(testdata.join(name)).unwrap());
        assert_eq!(got.len(), want.len(), "{name}: length");
        if let Some(i) = got.iter().zip(&want).position(|(a, b)| a != b) {
            panic!("{name}: byte {i} is {:#04x}, the reference has {:#04x}", got[i], want[i]);
        }
        // And the reference files read back as written.
        let mut r = Reader::open(testdata.join(name)).unwrap();
        assert_eq!((r.frames(), r.rate()), (s.len() as u64 + 4, rate), "{name}");
        let mut out = vec![0.0f32; s.len() + 4];
        r.read(0, &mut out);
        if format == Format::F32 {
            assert_eq!(&out[..5], &s[..5], "{name}");
            assert_eq!(&out[5..9], &[0.0; 4], "{name}");
            assert_eq!(&out[9..], &s[5..], "{name}");
        }
    }
}

#[test]
fn header_layout() {
    let dir = TempDir::new("header");
    let path = dir.path("a.wav");
    let mut w = Writer::create(&path, 48000, Format::S24).unwrap();
    w.write(&[0.0; 3]).unwrap(); // 9 bytes: odd
    w.sync(false).unwrap();
    let b = fs::read(&path).unwrap();
    let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
    // Synced but not closed: the RIFF size counts the pad byte to come, the data size doesn't.
    assert_eq!((u32_at(4), u32_at(40)), (36 + 10, 9));
    assert_eq!(&b[36..40], b"data");
    assert_eq!((u32_at(24), u32_at(28)), (48000, 48000 * 3));
    w.close().unwrap();
    let b = fs::read(&path).unwrap();
    assert_eq!(b.len(), 44 + 10);
}

#[test]
fn reads_other_programs_files() {
    let dir = TempDir::new("others");
    // 32-bit PCM in WAVE_FORMAT_EXTENSIBLE, with a LIST chunk before the data.
    let mut h = Vec::new();
    h.extend_from_slice(b"RIFF\0\0\0\0WAVEfmt ");
    h.extend_from_slice(&40u32.to_le_bytes());
    h.extend_from_slice(&0xFFFEu16.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&44100u32.to_le_bytes());
    h.extend_from_slice(&(44100u32 * 4).to_le_bytes());
    h.extend_from_slice(&4u16.to_le_bytes());
    h.extend_from_slice(&32u16.to_le_bytes());
    h.extend_from_slice(&22u16.to_le_bytes());
    h.extend_from_slice(&32u16.to_le_bytes());
    h.extend_from_slice(&4u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes()); // the GUID starts with the format: PCM
    h.extend_from_slice(&[0; 14]);
    h.extend_from_slice(b"LIST");
    h.extend_from_slice(&3u32.to_le_bytes());
    h.extend_from_slice(b"abc\0"); // odd, so padded
    h.extend_from_slice(b"data");
    h.extend_from_slice(&8u32.to_le_bytes());
    h.extend_from_slice(&(1i32 << 30).to_le_bytes());
    h.extend_from_slice(&i32::MIN.to_le_bytes());
    let path = dir.path("ext.wav");
    fs::write(&path, &h).unwrap();
    let mut r = Reader::open(&path).unwrap();
    assert_eq!((r.frames(), r.rate()), (2, 44100));
    let mut out = [0.0f32; 2];
    r.read(0, &mut out);
    assert_eq!(out, [0.5, -1.0]);

    let refused = |bytes: &[u8]| Reader::open(write(&dir, bytes)).err().unwrap().to_string();
    assert_eq!(refused(b"RIFX\0\0\0\0WAVE"), "bad.wav: not a WAV file");
    assert_eq!(refused(b"RIFF\0\0\0\0WAVE"), "bad.wav: no audio data");
    let mut stereo = h.clone();
    stereo[22] = 2;
    assert_eq!(refused(&stereo), "bad.wav: 2 channels; only mono files can be played");
    // For import, a stereo file opens, and its channels read apart: the 8 bytes of data are one
    // frame of two 32-bit samples.
    let mut r = Reader::open_any(write(&dir, &stereo)).unwrap();
    assert_eq!((r.channels(), r.frames()), (2, 1));
    let mut outs = vec![vec![9.0f32; 2], vec![9.0f32; 2]];
    r.read_channels(0, &mut outs);
    assert_eq!(outs, [vec![0.5, 0.0], vec![-1.0, 0.0]], "left, right; zeros past the end");
    let mut bits8 = h.clone();
    bits8[34] = 8;
    assert_eq!(refused(&bits8), "bad.wav: unsupported sample format (1, 8 bits)");
}

fn write(dir: &TempDir, bytes: &[u8]) -> PathBuf {
    let path = dir.path("bad.wav");
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn formats() {
    assert_eq!(Format::parse("s24"), Some(Format::S24));
    assert_eq!(Format::parse("f32"), Some(Format::F32));
    assert_eq!(Format::parse("S24"), None);
    assert_eq!(Format::default(), Format::S24);
    assert_eq!(Format::F32.to_string(), "32-bit float");
    assert_eq!(Format::S16.bytes(), 2);
}

/// The cost of converting one period of one track (512 samples), without the disk. Run with `cargo test --release -p as-wav -- --ignored --nocapture`.
#[test]
#[ignore]
fn bench_convert() {
    let buf: Vec<f32> = (0..512).map(|i| (i % 2000 - 1000) as f32 / 1000.0).collect();
    let mut out = Vec::with_capacity(512 * 4);
    const N: u32 = 200_000;
    for (name, f) in [
        (
            "f32",
            (|s: &[f32], b: &mut Vec<u8>| b.extend(s.iter().flat_map(|v| v.to_le_bytes())))
                as fn(&[f32], &mut Vec<u8>),
        ),
        ("s24", |s, b| s24(s.iter().map(|&v| v as f64), b)),
        ("s16", |s, b| {
            s16(s, DITHER_SEED, b);
        }),
    ] {
        let start = std::time::Instant::now();
        for _ in 0..N {
            out.clear();
            f(std::hint::black_box(&buf), &mut out);
            std::hint::black_box(&out);
        }
        println!("{name}: {:.0} ns per 512 samples", start.elapsed().as_nanos() as f64 / N as f64);
    }
}

#[test]
fn stereo_round_trip() {
    let dir = TempDir::new("stereo");
    for format in [Format::S24, Format::S16, Format::F32] {
        let path = dir.path(&format!("{format:?}.wav"));
        let mut w = Writer::create_channels(&path, 48000, format, 2).unwrap();
        let frames: Vec<f32> =
            (0..2000).map(|i| ((i / 2) as f32 * 0.01).sin() * if i % 2 == 0 { 0.5 } else { -0.25 }).collect();
        w.write(&frames).unwrap();
        assert_eq!(w.frames(), 1000);
        w.close().unwrap();
        let mut r = Reader::open_any(&path).unwrap();
        assert_eq!((r.channels(), r.frames(), r.rate()), (2, 1000, 48000));
        let mut outs = vec![vec![0.0; 1000], vec![0.0; 1000]];
        r.read_channels(0, &mut outs);
        for k in 0..1000 {
            assert!((outs[0][k] - frames[2 * k]).abs() < 1e-4);
            assert!((outs[1][k] - frames[2 * k + 1]).abs() < 1e-4);
        }
        // Other programs read it too: the header says two channels and the block's size.
        let b = fs::read(&path).unwrap();
        assert_eq!(u16::from_le_bytes([b[22], b[23]]), 2);
        assert_eq!(u16::from_le_bytes([b[32], b[33]]) as u64, 2 * format.bytes());
    }
}

/// A RIFF file of these chunks, in order.
fn riff(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut body = b"WAVE".to_vec();
    for (id, b) in chunks {
        body.extend_from_slice(*id);
        body.extend_from_slice(&(b.len() as u32).to_le_bytes());
        body.extend_from_slice(b);
        if b.len() % 2 == 1 {
            body.push(0);
        }
    }
    [b"RIFF".to_vec(), (body.len() as u32).to_le_bytes().to_vec(), body].concat()
}

fn fmt_chunk(format: u16, channels: u16, rate: u32, bits: u16) -> Vec<u8> {
    let block = channels * bits / 8;
    [
        format.to_le_bytes().to_vec(),
        channels.to_le_bytes().to_vec(),
        rate.to_le_bytes().to_vec(),
        (rate * block as u32).to_le_bytes().to_vec(),
        block.to_le_bytes().to_vec(),
        bits.to_le_bytes().to_vec(),
    ]
    .concat()
}

#[test]
fn every_format_reads_into_f64_exactly() {
    let dir = TempDir::new("f64");
    // 64-bit float, stereo: values f32 can't hold come back as they were.
    let vals = [0.1f64, -0.3, 1.0 / 3.0, 4.0];
    let data: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
    let path = write(&dir, &riff(&[(b"fmt ", fmt_chunk(3, 2, 96_000, 64)), (b"data", data)]));
    let mut r = Reader::open_any(&path).unwrap();
    assert_eq!((r.bits(), r.channels(), r.rate(), r.frames()), (64, 2, 96_000, 2));
    assert_eq!(r.read_channels_f64().unwrap(), vec![vec![0.1, 1.0 / 3.0], vec![-0.3, 4.0]]);
    // 32-bit integer: every bit kept, where f32 would lose the low ones.
    let ints = [i32::MAX, -7, 123_456_789];
    let data: Vec<u8> = ints.iter().flat_map(|v| v.to_le_bytes()).collect();
    let path = dir.path("i32.wav");
    fs::write(&path, riff(&[(b"fmt ", fmt_chunk(1, 1, 48_000, 32)), (b"data", data)])).unwrap();
    let got = Reader::open_any(&path).unwrap().read_channels_f64().unwrap();
    let want: Vec<f64> = ints.iter().map(|&v| v as f64 / 2_147_483_648.0).collect();
    assert_eq!(got, vec![want]);
}

#[test]
fn other_chunks_are_kept_and_written_back() {
    let dir = TempDir::new("chunks");
    let mut bext = Bext::new();
    bext.set_time_reference(48_000 * 3600);
    bext.push_history("A=PCM,F=48000,W=24,M=mono");
    let ixml = b"<BWFXML><PROJECT>ep04</PROJECT></BWFXML>".to_vec(); // odd length: padded
    let data: Vec<u8> = [0i16, 1000, -1000].iter().flat_map(|v| v.to_le_bytes()).collect();
    // iXML after the data, as some recorders put it.
    let path = write(
        &dir,
        &riff(&[
            (b"fmt ", fmt_chunk(1, 1, 48_000, 16)),
            (b"bext", bext.to_bytes()),
            (b"JUNK", vec![0; 6]),
            (b"data", data),
            (b"iXML", ixml.clone()),
        ]),
    );
    let mut r = Reader::open_any(&path).unwrap();
    assert_eq!(r.frames(), 3);
    let chunks = r.chunks().unwrap();
    let ids: Vec<_> = chunks.iter().map(|c| &c.id).collect();
    assert_eq!(ids, [b"bext", b"JUNK", b"iXML"]);
    assert_eq!(chunks[2].body, ixml);
    let mut b = Bext::parse(&chunks[0].body);
    assert_eq!(b.time_reference(), 48_000 * 3600);
    b.push_history("T=dexter-tape/0.1.0");
    assert_eq!(b.history(), ["A=PCM,F=48000,W=24,M=mono", "T=dexter-tape/0.1.0"]);

    // Written back around new samples, read again the same.
    let out = dir.path("out.wav");
    let kept = vec![Chunk { id: *b"bext", body: b.to_bytes() }, chunks[2].clone()];
    let f = OpenOptions::new().write(true).create_new(true).open(&out).unwrap();
    let mut w = Writer::with_chunks(f, out.clone(), 48_000, Format::F32, 1, &kept).unwrap();
    w.write_f64(&r.read_channels_f64().unwrap()[0]).unwrap();
    w.close().unwrap();
    let mut back = Reader::open_any(&out).unwrap();
    assert_eq!(back.chunks().unwrap(), kept);
    assert_eq!(back.read_channels_f64().unwrap(), vec![vec![0.0, 1000.0 / 32768.0, -1000.0 / 32768.0]]);
}

#[test]
fn rf64_is_refused_by_name() {
    let dir = TempDir::new("rf64");
    let mut bytes = riff(&[(b"fmt ", fmt_chunk(1, 1, 48_000, 16)), (b"data", vec![0; 4])]);
    bytes[..4].copy_from_slice(b"RF64");
    let err = Reader::open_any(write(&dir, &bytes)).err().unwrap();
    assert!(err.to_string().contains("RF64"), "{err}");
}

#[test]
fn s24_from_f64_rounds_once() {
    let dir = TempDir::new("s24f64");
    let path = dir.path("a.wav");
    // Just either side of half a step, closer than f32 can tell apart: each to its own nearest.
    let step = 1.0 / 8_388_608.0;
    let (above, below) = (0.75 + 0.5 * step + 1e-12, 0.75 + 0.5 * step - 1e-12);
    let mut w = Writer::create(&path, 48_000, Format::S24).unwrap();
    w.write_f64(&[above, below, -above, 2.0]).unwrap();
    w.close().unwrap();
    let got = Reader::open(&path).unwrap().read_channels_f64().unwrap();
    assert_eq!(got[0], [0.75 + step, 0.75, -0.75 - step, 8_388_607.0 * step]);
}

#[test]
fn audio_past_a_stale_header_is_never_read_as_chunks() {
    // A recording synced at 3 frames, then 1000 more frames written before a crash: the header
    // still says 3 frames. What lies past is audio, not chunks.
    let dir = TempDir::new("stale");
    let path = dir.path("a.wav");
    let mut w = Writer::create(&path, 48_000, Format::S16).unwrap();
    w.write(&[0.1, 0.2, 0.3]).unwrap();
    w.sync(false).unwrap();
    let noise: Vec<f32> = (0..1000).map(|k| ((k * 7919) % 2000) as f32 / 1000.0 - 1.0).collect();
    w.write(&noise).unwrap();
    std::mem::forget(w); // the crash: no sync, no close
    let bytes = fs::read(&path).unwrap();
    let mut flushed = bytes.clone();
    // Whatever made it to disk, the header says 3 frames.
    flushed.extend_from_slice(b"fmt \x10\0\0\0garbage-garbage!");
    fs::write(&path, &flushed).unwrap();
    let r = Reader::open_any(&path).unwrap();
    assert!(
        r.chunks().unwrap().is_empty(),
        "{:?}",
        r.chunks().unwrap().iter().map(|c| c.id).collect::<Vec<_>>()
    );
    assert_eq!(r.channels(), 1);
}

#[test]
fn a_processed_bext_clears_what_described_the_original() {
    let mut body = vec![0u8; Bext::FIXED];
    body[338] = 7; // TimeReference
    body[346] = 2; // version 2
    body[348..412].fill(0xAB); // UMID
    body[412..414].copy_from_slice(&(-2300i16).to_le_bytes()); // LoudnessValue
    let mut b = Bext::parse(&body);
    b.processed();
    let out = b.to_bytes();
    assert_eq!(b.time_reference(), 7);
    assert!(out[348..412].iter().all(|&v| v == 0));
    assert_eq!(i16::from_le_bytes([out[412], out[413]]), 0x7FFF);
}
