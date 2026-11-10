use super::*;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

/// A new, empty folder of the test's own.
fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("as-flac-tags-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Two channels of a second: noise on one, a tone on the other.
fn sound() -> Vec<Vec<i32>> {
    let mut x = 9u64;
    let a = (0..48000)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            ((x >> 40) as i64 - (1 << 23)) as i32
        })
        .collect();
    let b = (0..48000).map(|i| ((i as f64 * 0.03).sin() * 7_000_000.0) as i32).collect();
    vec![a, b]
}

/// The encoder's stream (STREAMINFO alone) with `extra` blocks after STREAMINFO.
fn flac(extra: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let plain = crate::encode(&sound(), 48000);
    let mut out = plain[..8 + 34].to_vec();
    if !extra.is_empty() {
        out[4] &= 0x7F;
    }
    for (i, (kind, body)) in extra.iter().enumerate() {
        let last = if i + 1 == extra.len() { 0x80 } else { 0 };
        let len = (body.len() as u32).to_be_bytes();
        out.extend_from_slice(&[last | kind, len[1], len[2], len[3]]);
        out.extend_from_slice(body);
    }
    out.extend_from_slice(&plain[8 + 34..]);
    out
}

fn comments(vendor: &str, fields: &[(&str, &str)]) -> Vec<u8> {
    Tags {
        vendor: vendor.into(),
        fields: fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
    }
    .encode()
}

fn decoded(path: &Path) -> Vec<i32> {
    let mut r = claxon::FlacReader::open(path).unwrap();
    r.samples().map(Result::unwrap).collect()
}

fn edit(k: &str, v: &[&str]) -> (String, Vec<String>) {
    (k.to_string(), v.iter().map(|s| s.to_string()).collect())
}

/// Every file in `dir`, by name.
fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> =
        fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

#[test]
fn a_file_without_comments_has_no_tags() {
    let d = scratch("none");
    let p = d.join("a.flac");
    fs::write(&p, flac(&[])).unwrap();
    assert_eq!(read(&p).unwrap(), Tags::default());
}

#[test]
fn tags_are_read_in_order_with_repeats() {
    let d = scratch("read");
    let p = d.join("a.flac");
    let vc = comments("ref", &[("FXNAME", "Gravel Walk"), ("keywords", "gravel"), ("KEYWORDS", "boots")]);
    fs::write(&p, flac(&[(1, vec![0; 100]), (VORBIS_COMMENT, vc)])).unwrap();
    let t = read(&p).unwrap();
    assert_eq!(t.vendor, "ref");
    assert_eq!(t.get("Keywords"), ["gravel", "boots"]);
    assert_eq!(t.get("FXNAME"), ["Gravel Walk"]);
    assert!(t.get("SERIAL").is_empty());
}

#[test]
fn set_replaces_in_place_in_capitals() {
    let mut t = Tags {
        vendor: String::new(),
        fields: vec![
            ("A".into(), "1".into()),
            ("kw".into(), "x".into()),
            ("B".into(), "2".into()),
            ("KW".into(), "y".into()),
        ],
    };
    t.set("Kw", &["p".into(), "q".into()]);
    let f = |t: &Tags| t.fields.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>();
    assert_eq!(f(&t), ["A=1", "KW=p", "KW=q", "B=2"]);
    t.set("C", &["3".into()]);
    t.set("A", &[]);
    assert_eq!(f(&t), ["KW=p", "KW=q", "B=2", "C=3"]);
}

#[test]
fn a_change_keeps_every_other_block_and_every_audio_byte() {
    let d = scratch("keep");
    let p = d.join("a.flac");
    let app = (2u8, b"TEST and its bytes".to_vec());
    let pad = (1u8, vec![0; 4096]);
    let old = flac(&[app.clone(), (VORBIS_COMMENT, comments("ref", &[("SERIAL", "k3v9qzt")])), pad.clone()]);
    fs::write(&p, &old).unwrap();
    let before = decoded(&p);
    let old_head = Head::read(&mut &old[..]).unwrap();

    let j = d.join("journal.jsonl");
    let long = "a long account ".repeat(500);
    let changes =
        set(&p, &[edit("DESCRIPTION", &[&long]), edit("KEYWORDS", &["gravel", "boots"])], &j).unwrap();
    assert_eq!(changes.len(), 2);

    let new = fs::read(&p).unwrap();
    let head = Head::read(&mut &new[..]).unwrap();
    // The blocks in their order, the comments where they were; STREAMINFO (and its MD5) as it was.
    assert_eq!(head.blocks.iter().map(|b| b.kind).collect::<Vec<_>>(), [STREAMINFO, 2, VORBIS_COMMENT, 1]);
    assert_eq!(head.blocks[0], old_head.blocks[0]);
    assert_eq!(head.blocks[1].body, app.1);
    assert_eq!(head.blocks[3].body, pad.1);
    assert_eq!(new[head.audio_at as usize..], old[old_head.audio_at as usize..]);
    assert_eq!(decoded(&p), before);
    let t = read(&p).unwrap();
    assert_eq!(t.vendor, "ref");
    assert_eq!(t.get("SERIAL"), ["k3v9qzt"]);
    assert_eq!(t.get("DESCRIPTION"), [long.as_str()]);
    assert_eq!(t.get("KEYWORDS"), ["gravel", "boots"]);
    assert_eq!(names(&d), ["a.flac", "journal.jsonl"]);
}

#[test]
fn comments_are_added_after_streaminfo_when_there_were_none() {
    let d = scratch("add");
    let p = d.join("a.flac");
    let old = flac(&[(1, vec![0; 10])]);
    fs::write(&p, &old).unwrap();
    set(&p, &[edit("FXNAME", &["Door Slam"])], &d.join("journal.jsonl")).unwrap();
    let new = fs::read(&p).unwrap();
    let head = Head::read(&mut &new[..]).unwrap();
    assert_eq!(head.blocks.iter().map(|b| b.kind).collect::<Vec<_>>(), [STREAMINFO, VORBIS_COMMENT, 1]);
    assert_eq!(read(&p).unwrap().get("fxname"), ["Door Slam"]);
    // The encoder's own, no other blocks: the last-block flag moves to the comments.
    let p = d.join("b.flac");
    fs::write(&p, flac(&[])).unwrap();
    set(&p, &[edit("FXNAME", &["Door Slam"])], &d.join("journal.jsonl")).unwrap();
    assert_eq!(read(&p).unwrap().get("FXNAME"), ["Door Slam"]);
    assert_eq!(decoded(&p), decoded(&d.join("a.flac")));
}

#[test]
fn each_change_is_journalled() {
    let d = scratch("journal");
    let p = d.join("k3v9qzt.flac");
    fs::write(&p, flac(&[(VORBIS_COMMENT, comments("ref", &[("SERIAL", "k3v9qzt"), ("FXNAME", "Old")]))]))
        .unwrap();
    let j = d.join("journal.jsonl");
    set(&p, &[edit("FXNAME", &["New"]), edit("KEYWORDS", &["a", "b"])], &j).unwrap();
    set(&p, &[edit("KEYWORDS", &[])], &j).unwrap();
    let text = fs::read_to_string(&j).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    // The line as it's written, its names in the order of the alphabet, as serde_json had them.
    let when = &journal::Entry::parse(lines[0]).unwrap().when;
    assert!(when.ends_with('Z'));
    assert_eq!(
        lines[0],
        format!(
            r#"{{"changes":[{{"after":["New"],"before":["Old"],"field":"FXNAME"}},{{"after":["a","b"],"before":[],"field":"KEYWORDS"}}],"file":"k3v9qzt.flac","serial":"k3v9qzt","what":"set","when":"{when}"}}"#
        )
    );
    assert!(lines[1].contains(r#"{"after":[],"before":["a","b"],"field":"KEYWORDS"}"#));
    // And read back.
    let read: Vec<journal::Entry> = journal::read(&j).unwrap().into_iter().flatten().collect();
    assert_eq!(read.len(), 2);
    assert_eq!(read[1].changes[0].before, ["a", "b"]);
    assert_eq!(read[0].how, How::new("set"));
    let how = How { what: "batch".into(), batch: Some("b1".into()), undoes: Some("batch:b0".into()) };
    set_as(&p, &[edit("NOTES", &["x"])], &j, &how).unwrap();
    assert_eq!(journal::read(&j).unwrap()[2].as_ref().unwrap().how, how);
    // A line cut short by a power cut: kept in its place, and the next entry on a line of its own.
    let mut text = fs::read(&j).unwrap();
    text.extend_from_slice(br#"{"changes":[{"fie"#);
    fs::write(&j, &text).unwrap();
    set(&p, &[edit("NOTES", &["y"])], &j).unwrap();
    let lines = journal::read(&j).unwrap();
    assert_eq!(lines.len(), 5);
    assert!(lines[3].is_none());
    assert_eq!(lines[4].as_ref().unwrap().changes[0].after, ["y"]);
    // A line from before `what` was written is a set.
    let old = r#"{"changes":[],"file":"a.flac","serial":null,"when":"2026-10-28T17:58:35Z"}"#;
    assert_eq!(journal::Entry::parse(old).unwrap().how.what, "set");
    let old = r#"{"changes":[{"after":["k3v9qzt"],"before":[],"field":"SERIAL"}],"file":"k3v9qzt.flac","serial":"k3v9qzt","when":"2026-10-28T17:58:35Z"}"#;
    assert_eq!(journal::Entry::parse(old).unwrap().how.what, "import");
}

#[test]
fn nothing_changed_writes_nothing() {
    let d = scratch("same");
    let p = d.join("a.flac");
    fs::write(&p, flac(&[(VORBIS_COMMENT, comments("ref", &[("FXNAME", "Same")]))])).unwrap();
    let ino = fs::metadata(&p).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let j = d.join("journal.jsonl");
    assert!(set(&p, &[edit("fxname", &["Same"]), edit("NOTES", &[])], &j).unwrap().is_empty());
    assert_eq!(fs::metadata(&p).unwrap().modified().unwrap(), ino);
    assert_eq!(names(&d), ["a.flac"]);
}

#[test]
fn interrupted_while_writing_the_old_file_stands() {
    let d = scratch("interrupt");
    let p = d.join("a.flac");
    let old = flac(&[(VORBIS_COMMENT, comments("ref", &[("FXNAME", "Old")]))]);
    fs::write(&p, &old).unwrap();
    let j = d.join("journal.jsonl");
    let r = set_with(&p, &[edit("FXNAME", &["New"])], &j, &How::new("set"), &mut |s| match s {
        Stage::Metadata => Err(io::Error::other("power cut")),
        _ => Ok(()),
    });
    assert_eq!(r.unwrap_err().to_string(), "power cut");
    assert_eq!(fs::read(&p).unwrap(), old);
    assert_eq!(names(&d), ["a.flac"], "no half-written file, no journal");
}

#[test]
fn interrupted_before_the_rename_the_journal_says_more_than_happened() {
    // The entry is appended before the rename, so a change is never made unjournalled. Cut
    // between the two, the journal names a change the file doesn't have; undoing it sets what's
    // there already.
    let d = scratch("journalled");
    let p = d.join("a.flac");
    let old = flac(&[(VORBIS_COMMENT, comments("ref", &[("FXNAME", "Old")]))]);
    fs::write(&p, &old).unwrap();
    let j = d.join("journal.jsonl");
    let r = set_with(&p, &[edit("FXNAME", &["New"])], &j, &How::new("set"), &mut |s| match s {
        Stage::Journalled => Err(io::Error::other("power cut")),
        _ => Ok(()),
    });
    assert!(r.is_err());
    assert_eq!(fs::read(&p).unwrap(), old);
    assert_eq!(names(&d), ["a.flac", "journal.jsonl"]);
}

#[test]
fn a_file_changed_meanwhile_is_left_alone() {
    let d = scratch("meanwhile");
    let p = d.join("a.flac");
    fs::write(&p, flac(&[])).unwrap();
    let theirs = flac(&[(1, vec![0; 7])]);
    let j = d.join("journal.jsonl");
    let r = set_with(&p, &[edit("FXNAME", &["Mine"])], &j, &How::new("set"), &mut |s| {
        if s == Stage::Metadata {
            fs::write(&p, &theirs)?;
        }
        Ok(())
    });
    assert!(r.unwrap_err().to_string().contains("changed while"));
    assert_eq!(fs::read(&p).unwrap(), theirs);
    assert_eq!(names(&d), ["a.flac"]);
}

#[test]
fn a_crash_left_file_is_never_touched() {
    let d = scratch("stale");
    let p = d.join("a.flac");
    fs::write(&p, flac(&[])).unwrap();
    let stale = d.join(".a.flac.tags-1-1");
    fs::write(&stale, b"half").unwrap();
    set(&p, &[edit("FXNAME", &["New"])], &d.join("journal.jsonl")).unwrap();
    assert_eq!(fs::read(&stale).unwrap(), b"half");
    assert_eq!(read(&p).unwrap().get("FXNAME"), ["New"]);
}

#[test]
fn the_files_permissions_are_kept() {
    let d = scratch("mode");
    let p = d.join("a.flac");
    fs::write(&p, flac(&[])).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o640)).unwrap();
    set(&p, &[edit("FXNAME", &["New"])], &d.join("journal.jsonl")).unwrap();
    assert_eq!(fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o640);
}

#[test]
fn what_isnt_a_flac_is_refused() {
    let d = scratch("refused");
    let j = d.join("journal.jsonl");
    for (name, bytes, why) in [
        ("wav.flac", b"RIFF....WAVE".to_vec(), "isn't a FLAC"),
        ("id3.flac", [b"ID3".to_vec(), flac(&[])].concat(), "ID3"),
        ("short.flac", flac(&[])[..20].to_vec(), "ends early"),
        (
            "two.flac",
            flac(&[(VORBIS_COMMENT, comments("", &[])), (VORBIS_COMMENT, comments("", &[]))]),
            "more than one",
        ),
    ] {
        let p = d.join(name);
        fs::write(&p, &bytes).unwrap();
        let e = set(&p, &[edit("FXNAME", &["x"])], &j).unwrap_err().to_string();
        assert!(e.contains(why), "{name}: {e}");
        assert_eq!(fs::read(&p).unwrap(), bytes);
    }
    let p = d.join("ok.flac");
    fs::write(&p, flac(&[])).unwrap();
    assert!(set(&p, &[edit("FX=NAME", &["x"])], &j).is_err());
    assert!(set(&p, &[edit("", &["x"])], &j).is_err());
    assert_eq!(names(&d), ["id3.flac", "ok.flac", "short.flac", "two.flac", "wav.flac"]);
}

#[test]
fn utc_is_civil() {
    assert_eq!(utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
    assert_eq!(utc(UNIX_EPOCH + Duration::from_secs(951_782_400)), "2000-02-29T00:00:00Z");
    assert_eq!(utc(UNIX_EPOCH + Duration::from_secs(1_793_207_045)), "2026-10-28T17:04:05Z");
}

#[test]
fn tags_are_put_on_a_stream_in_memory() {
    let old = flac(&[(2, b"APPL".to_vec()), (VORBIS_COMMENT, comments("ref", &[("ARTIST", "x")]))]);
    let tags = Tags { vendor: "dexter-foley".into(), fields: vec![("SERIAL".into(), "k3v9qzt".into())] };
    let new = with_tags(&old, &tags).unwrap();
    let (a, b) = (Head::read(&mut &old[..]).unwrap(), Head::read(&mut &new[..]).unwrap());
    assert_eq!(b.tags().unwrap(), tags);
    assert_eq!(b.blocks.iter().map(|b| b.kind).collect::<Vec<_>>(), [STREAMINFO, 2, VORBIS_COMMENT]);
    assert_eq!(new[b.audio_at as usize..], old[a.audio_at as usize..]);
    let plain = crate::encode(&sound(), 48000);
    let new = with_tags(&plain, &tags).unwrap();
    assert_eq!(Head::read(&mut &new[..]).unwrap().tags().unwrap(), tags);
}

#[test]
fn a_journal_line_keeps_awkward_text() {
    let c = Change {
        field: "NOTES".into(),
        before: vec![],
        after: vec!["a \"quoted\" \\ line\nnext\ttab é 🎙".into()],
    };
    let e = journal::Entry {
        when: "2026-11-09T00:00:00Z".into(),
        file: "a.flac".into(),
        serial: None,
        how: How { what: "batch".into(), batch: Some("b1".into()), undoes: None },
        changes: vec![c],
    };
    assert_eq!(journal::Entry::parse(&e.line()).unwrap(), e);
    // What another writer might have put: spaces, escapes by number, a number and a boolean.
    let other = r#" { "when" : "w", "file":"f", "n": 12.5e3, "ok": true, "serial": null,
        "changes":[ {"field":"A","before":["\u00e9\ud83c\udf99"],"after":[]} ] } "#;
    let e = journal::Entry::parse(other).unwrap();
    assert_eq!(e.changes[0].before, ["é🎙"]);
    assert!(journal::Entry::parse(r#"{"when":"w","file":"f","changes":[}"#).is_none());
}
