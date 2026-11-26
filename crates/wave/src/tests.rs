use std::fs;
use std::path::PathBuf;

use super::*;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("as-wave-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// irma's test (`core/src/tests.rs`), the onset now asked for.
#[test]
fn a_picture_has_the_lowest_and_highest_of_each_column() {
    let mut ir = vec![0.0f32; 1000];
    ir[100] = 1.0;
    ir[105] = -0.5;
    ir[600] = 0.25;
    let c = vec![ir, vec![0.0; 1000]];
    let p = picture(&c, 0.0, 1000.0, 10, Some(100));
    assert_eq!((p.lo[1], p.hi[1]), (-0.5, 1.0));
    assert_eq!((p.lo[6], p.hi[6]), (0.0, 0.25));
    assert_eq!((p.lo[3], p.hi[3]), (0.0, 0.0));
    // The decay: nothing before the onset, 0 dB at it, falling after, gone when nothing's left.
    assert_eq!(p.decay[0], None);
    assert_eq!(p.decay[1], Some(0.0));
    let after = p.decay[2].unwrap();
    assert!((after - 10.0 * (0.0625f32 / 1.3125).log10()).abs() < 1e-4, "{after}");
    assert_eq!(p.decay[7], None);
    // Zoomed past a frame a column, and run off both ends: still a picture, nothing where nothing is.
    let z = picture(&c, 98.0, 102.0, 16, Some(100));
    assert_eq!(z.hi.iter().filter(|v| **v == 1.0).count(), 4, "frame 100, four columns wide");
    let off = picture(&c, -500.0, 1500.0, 4, Some(100));
    assert_eq!((off.hi[0], off.hi[1], off.hi[3]), (0.0, 1.0, 0.0));
    // With no onset: the same lowest and highest, and no decay.
    let plain = picture(&c, 0.0, 1000.0, 10, None);
    assert_eq!((&plain.lo, &plain.hi), (&p.lo, &p.hi));
    assert!(plain.decay.is_empty());
}

#[test]
fn a_span_that_is_no_span_pictures_nothing_and_stops_nothing() {
    let c = vec![vec![0.5f32; 10]];
    for (from, to) in [(f64::NAN, 5.0), (f64::NEG_INFINITY, f64::INFINITY), (1e300, 2e300), (-9.0, -2.0)] {
        let p = picture(&c, from, to, 4, Some(0));
        assert_eq!((p.lo.len(), p.hi.len(), p.decay.len()), (4, 4, 4), "{from} {to}");
    }
    assert_eq!(picture(&c, 20.0, 30.0, 3, None).hi, [0.0; 3]);
    assert_eq!(picture(&[], 0.0, 10.0, 3, Some(0)).hi, [0.0; 3]);
    assert_eq!(picture(&c, 0.0, 10.0, 0, None).hi, [0.5], "a column at least");
}

const RATE: u32 = 48_000;

/// Two channels that differ at every frame, in 24-bit steps so each format holds them.
fn sound(frames: usize) -> Vec<Vec<i32>> {
    let ch = |a: i64, b: i64| {
        (0..frames as i64).map(|k| ((k * a + b) % 16_000_001 - 8_000_000) as i32).collect::<Vec<_>>()
    };
    vec![ch(7_919_113, 5), ch(104_729_017, 11)]
}

fn wav(path: &Path, format: as_wav::Format, s: &[Vec<i32>]) {
    let mut w = as_wav::Writer::create_channels(path, RATE, format, s.len() as u16).unwrap();
    let inter: Vec<f32> =
        (0..s[0].len()).flat_map(|k| s.iter().map(move |c| c[k] as f32 / 8_388_608.0)).collect();
    w.write(&inter).unwrap();
    w.close().unwrap();
}

/// A WAV whole in memory, by the one conversion the file's picture uses.
fn whole(path: &Path) -> Vec<Vec<f32>> {
    single(&as_wav::Reader::open_any(path).unwrap().read_channels_f64().unwrap())
}

/// Spans with fractional ends, columns narrower than a frame, ends overrun, and no sound at all.
const SPANS: [(f64, f64, usize); 8] = [
    (0.0, 20_000.0, 100),
    (123.4, 9_876.5, 333),
    (500.25, 504.75, 40),
    (-3_000.5, 2_000.5, 64),
    (18_000.0, 26_000.0, 77),
    (-500.0, 21_000.0, 1_000),
    (30_000.0, 40_000.0, 5),
    (-40.0, -10.0, 5),
];

#[test]
fn a_span_from_a_file_is_the_span_from_memory_at_any_block() {
    let d = scratch("span");
    for (name, format) in [("a", as_wav::Format::S24), ("b", as_wav::Format::F32), ("c", as_wav::Format::S16)]
    {
        let path = d.join(format!("{name}.wav"));
        wav(&path, format, &sound(20_000));
        let memory = whole(&path);
        for (from, to, columns) in SPANS {
            let want = picture(&memory, from, to, columns, None);
            for block in [1, 97, 4096, 100_000] {
                let (got, _) = wav_picture(&path, Some((from, to)), columns, block).unwrap();
                assert_eq!((&got.lo, &got.hi), (&want.lo, &want.hi), "{format} {from} {to} by {block}");
                assert_eq!((got.rate, got.frames), (RATE, 20_000));
            }
            let public = file_picture(&path, from, to, columns).unwrap();
            assert_eq!(public.hi, want.hi);
        }
    }
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn only_the_spans_frames_are_read() {
    let d = scratch("read");
    let path = d.join("long.wav");
    wav(&path, as_wav::Format::S24, &sound(200_000));
    let read = |from, to| wav_picture(&path, Some((from, to)), 50, 4096).unwrap().1;
    assert_eq!(read(100_000.0, 100_500.0), 500);
    assert_eq!(read(-1_000.0, 300.0), 300);
    assert_eq!(read(300_000.0, 400_000.0), 0);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn an_overview_of_a_wav_or_a_flac_and_no_span_of_a_flac_yet() {
    let d = scratch("overview");
    let s = sound(50_000);
    let (w, f) = (d.join("a.wav"), d.join("a.flac"));
    wav(&w, as_wav::Format::S24, &s);
    fs::write(&f, as_flac::encode(&s, RATE)).unwrap();
    let from_flac = single(&as_flac::read_f64(&f).unwrap().channels);
    let want = picture(&from_flac, 0.0, 50_000.0, 300, None);
    for block in [1, 4_099, 1 << 16] {
        let got = flac_overview(&f, 300, block).unwrap();
        assert_eq!((&got.lo, &got.hi, got.frames, got.rate), (&want.lo, &want.hi, 50_000, RATE), "{block}");
    }
    assert_eq!(overview(&f, 300).unwrap().hi, want.hi);
    assert_eq!(overview(&w, 300).unwrap().hi, picture(&whole(&w), 0.0, 50_000.0, 300, None).hi);
    let e = file_picture(&f, 0.0, 10.0, 4).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::Unsupported, "{e}");
    let _ = fs::remove_dir_all(&d);
}

fn names(d: &Path) -> Vec<String> {
    let mut v: Vec<String> =
        fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

#[test]
fn the_cache_is_made_once_and_loses_nothing_deleted() {
    let d = scratch("cache");
    let (takes, cache) = (d.join("takes"), d.join("state/waves"));
    fs::create_dir_all(&takes).unwrap();
    let path = takes.join("take.wav");
    wav(&path, as_wav::Format::S24, &sound(10_000));
    let source = fs::read(&path).unwrap();

    let first = cached_overview(&path, &cache, 100).unwrap();
    assert!(!first.found && first.not_kept.is_none(), "{:?}", first.not_kept);
    assert_eq!(first.picture, overview(&path, 100).unwrap());
    let kept = names(&cache);
    assert_eq!(kept.len(), 1, "{kept:?}");
    let again = cached_overview(&path, &cache, 100).unwrap();
    assert!(again.found);
    assert_eq!(again.picture, first.picture);
    // Nothing beside the source, and the source as it was.
    assert_eq!(names(&takes), ["take.wav"]);
    assert_eq!(fs::read(&path).unwrap(), source);

    // Deleted: made again, the same bytes.
    let file = cache.join(&kept[0]);
    let bytes = fs::read(&file).unwrap();
    fs::remove_file(&file).unwrap();
    assert!(!cached_overview(&path, &cache, 100).unwrap().found);
    assert_eq!(fs::read(&file).unwrap(), bytes);

    // Gone bad (cut short): made again.
    fs::write(&file, &bytes[..bytes.len() - 3]).unwrap();
    let mended = cached_overview(&path, &cache, 100).unwrap();
    assert!(!mended.found && mended.not_kept.is_none());
    assert_eq!(fs::read(&file).unwrap(), bytes);

    // Other columns, and the file changed: each its own.
    assert!(!cached_overview(&path, &cache, 101).unwrap().found);
    fs::remove_file(&path).unwrap();
    wav(&path, as_wav::Format::S24, &sound(12_000));
    let changed = cached_overview(&path, &cache, 100).unwrap();
    assert_eq!((changed.found, changed.picture.frames), (false, 12_000));
    assert_eq!(names(&cache).len(), 3, "no temporary left behind: {:?}", names(&cache));
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn an_overview_is_never_kept_beside_its_source() {
    let d = scratch("beside");
    let path = d.join("take.wav");
    wav(&path, as_wav::Format::S24, &sound(1_000));
    for folder in [d.clone(), d.join(".")] {
        let e = cached_overview(&path, &folder, 10).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidInput, "{e}");
    }
    assert_eq!(names(&d), ["take.wav"]);
    // A cache that can't be written is done without.
    fs::write(d.join("not-a-folder"), b"x").unwrap();
    let got = cached_overview(&path, d.join("not-a-folder"), 10).unwrap();
    assert!(!got.found && got.not_kept.is_some());
    assert_eq!(got.picture, overview(&path, 10).unwrap());
    assert_eq!(fs::read(d.join("not-a-folder")).unwrap(), b"x");
    let _ = fs::remove_dir_all(&d);
}

/// irma's `picture` as it was at `4fc58b4`, each column worked from the whole sound: what ours
/// must give, to the bit. It's here as the reference, and nothing else uses it.
fn irmas(c: &[Vec<f32>], from: f64, to: f64, columns: usize, onset: usize) -> Picture {
    let frames = c.first().map_or(0, Vec::len);
    let columns = columns.max(1);
    let (from, to) = (from.min(to), to.max(from));
    let width = (to - from).max(1e-9) / columns as f64;
    // Schroeder's integral, backwards: the energy from each frame on.
    let mut left = vec![0.0f64; frames + 1];
    for k in (0..frames).rev() {
        left[k] = left[k + 1] + c.iter().map(|ch| (ch[k] as f64).powi(2)).sum::<f64>();
    }
    let whole = left.get(onset.min(frames)).copied().unwrap_or(0.0);
    let (mut lo, mut hi, mut decay) = (vec![0.0; columns], vec![0.0; columns], vec![None; columns]);
    for col in 0..columns {
        let a = from + col as f64 * width;
        let b = a + width;
        // The frames whose place falls in the column: at least the one it starts on, so a
        // zoom past one frame a column still draws.
        let first = a.floor().max(0.0) as usize;
        let last = (b.ceil().max(0.0) as usize).max(first + 1).min(frames);
        if a < 0.0 && b <= 0.0 || first >= frames {
            continue;
        }
        let (mut l, mut h) = (f32::MAX, f32::MIN);
        for ch in c {
            for v in &ch[first..last] {
                l = l.min(*v);
                h = h.max(*v);
            }
        }
        if l <= h {
            (lo[col], hi[col]) = (l, h);
        }
        if first >= onset && whole > 0.0 && left[first] > 0.0 {
            decay[col] = Some((10.0 * (left[first] / whole).log10()) as f32);
        }
    }
    Picture { lo, hi, decay }
}

#[test]
fn the_numbers_are_irmas_to_the_bit() {
    let c: Vec<Vec<f32>> =
        sound(20_000).iter().map(|ch| ch.iter().map(|&v| v as f32 / 8_388_608.0).collect()).collect();
    let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    // The spans above, and many more whose ends fall anywhere.
    let more = (0..300u64).map(|k| {
        let from = (k * 7_919 % 30_011) as f64 / 1.3 - 4_000.0;
        let long = [0.7, 3.0, 41.5, 977.3, 24_000.0][(k % 5) as usize];
        (from, from + long, 1 + (k * 131 % 700) as usize)
    });
    for (from, to, columns) in SPANS.into_iter().chain(more) {
        for onset in [0, 137, 19_999, 25_000] {
            let (want, got) =
                (irmas(&c, from, to, columns, onset), picture(&c, from, to, columns, Some(onset)));
            assert_eq!(
                (bits(&got.lo), bits(&got.hi)),
                (bits(&want.lo), bits(&want.hi)),
                "{from} {to} {columns}"
            );
            assert_eq!(got.decay, want.decay, "{from} {to} {columns} from {onset}");
        }
    }
}
