//! Tars written and read back: short and long paths, empty and odd-sized files, a PAX size,
//! a damaged header, and what GNU tar makes.

use super::*;

fn read_all(bytes: &[u8]) -> Vec<(Entry, Vec<u8>)> {
    let mut r = Reader::new(bytes);
    let mut out = vec![];
    while let Some(e) = r.next_entry().unwrap() {
        let mut v = vec![];
        if e.kind == Kind::File {
            r.copy(&e, &mut v).unwrap();
        }
        out.push((e, v));
    }
    out
}

#[test]
fn what_is_written_is_read_back() {
    let long = format!("pack/{}/{}.flac", "a-folder-with-a-long-name".repeat(5), "b".repeat(90));
    let longer = format!("pack/{}.wav", "c".repeat(300));
    let body: Vec<u8> = (0..1300u32).map(|k| (k % 251) as u8).collect();
    let mut t = Writer::new(Vec::new());
    t.dir("pack", 7).unwrap();
    t.file("pack/empty.txt", 0, 7, &mut &b""[..]).unwrap();
    t.file("pack/odd.bin", 1300, 1_700_000_000, &mut &body[..]).unwrap();
    t.file(&long, 3, 7, &mut &b"abc"[..]).unwrap();
    t.file(&longer, 2, 7, &mut &b"de"[..]).unwrap();
    let bytes = t.finish().unwrap();
    assert_eq!(bytes.len() % BLOCK, 0);
    let got = read_all(&bytes);
    let paths: Vec<&str> = got.iter().map(|(e, _)| e.path.as_str()).collect();
    assert_eq!(paths, ["pack/", "pack/empty.txt", "pack/odd.bin", long.as_str(), longer.as_str()]);
    assert_eq!(got[0].0.kind, Kind::Dir);
    assert_eq!((got[2].0.size, got[2].0.mtime, &got[2].1), (1300, 1_700_000_000, &body));
    assert_eq!(got[3].1, b"abc");
    assert_eq!(got[4].1, b"de");
}

#[test]
fn entries_skipped_unread_and_a_damaged_header_refused() {
    let mut t = Writer::new(Vec::new());
    t.file("a", 600, 0, &mut &[1u8; 600][..]).unwrap();
    t.file("b", 1, 0, &mut &[2u8][..]).unwrap();
    let mut bytes = t.finish().unwrap();
    let mut r = Reader::new(&bytes[..]);
    r.next_entry().unwrap().unwrap();
    assert_eq!(r.next_entry().unwrap().unwrap().path, "b", "a's data skipped");
    bytes[10] ^= 0xff;
    assert!(Reader::new(&bytes[..]).next_entry().is_err());
    // Short of its size, a file isn't written.
    let mut t = Writer::new(Vec::new());
    assert!(t.file("c", 10, 0, &mut &b"short"[..]).is_err());
}

#[test]
fn a_pax_size_is_read() {
    let mut t = Writer::new(Vec::new());
    t.entry("big", b'0', 3, 0).unwrap();
    let mut bytes = t.w;
    bytes.extend_from_slice(b"xyz");
    bytes.resize(bytes.len().div_ceil(BLOCK) * BLOCK, 0);
    // A PAX record saying a size, before an ordinary header.
    let pax = record("size", "3");
    let mut with = header(b"././@PaxHeader", &[], b'x', pax.len() as u64, 0).to_vec();
    with.extend_from_slice(pax.as_bytes());
    with.resize(with.len().div_ceil(BLOCK) * BLOCK, 0);
    with.extend_from_slice(&bytes);
    with.extend_from_slice(&[0u8; 2 * BLOCK]);
    let got = read_all(&with);
    assert_eq!((got[0].0.size, got[0].1.as_slice()), (3, &b"xyz"[..]));
    assert_eq!(record("path", "x"), "9 path=x\n");
    assert_eq!(record("path", &"y".repeat(93)), format!("103 path={}\n", "y".repeat(93)));
}

#[test]
fn gnu_tars_are_read_and_ours_are_gnus() {
    // Only where GNU tar is.
    let d = std::env::temp_dir().join(format!("as-tar-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("p").join("x".repeat(120))).unwrap();
    std::fs::write(d.join("p").join("x".repeat(120)).join("f.txt"), b"hello").unwrap();
    let made = std::process::Command::new("tar")
        .arg("-C")
        .arg(&d)
        .arg("-cf")
        .arg(d.join("gnu.tar"))
        .arg("p")
        .status();
    if made.is_err() || !made.unwrap().success() {
        let _ = std::fs::remove_dir_all(&d);
        return;
    }
    let got = read_all(&std::fs::read(d.join("gnu.tar")).unwrap());
    let f = got.iter().find(|(e, _)| e.path.ends_with("f.txt")).unwrap();
    assert_eq!(f.1, b"hello");
    // Ours, listed by GNU tar.
    let mut t = Writer::new(Vec::new());
    let long = format!("q/{}/g.txt", "y".repeat(150));
    t.file(&long, 2, 0, &mut &b"ok"[..]).unwrap();
    std::fs::write(d.join("ours.tar"), t.finish().unwrap()).unwrap();
    let out = std::process::Command::new("tar").arg("-tf").arg(d.join("ours.tar")).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), long);
    let _ = std::fs::remove_dir_all(&d);
}
