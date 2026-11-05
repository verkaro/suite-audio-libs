use super::*;

fn s(h: &str, o: &[(&str, &str)]) -> Section {
    Section { header: h.into(), opcodes: o.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() }
}

#[test]
fn text_exact() {
    let sfz = Sfz {
        comments: vec![
            "00 Piano: a Casio LK-110's tone 00, sampled by dexter-like110 0.1.0, 2026-11-04".into(),
            "auto; 61 keys of 88; reference C4".into(),
        ],
        sections: vec![
            s("control", &[("default_path", "samples/")]),
            s("global", &[("volume", "6.4"), ("ampeg_release", "0.35")]),
            s(
                "region",
                &[
                    ("sample", "060_C4.wav"),
                    ("lokey", "60"),
                    ("hikey", "60"),
                    ("pitch_keycenter", "60"),
                    ("tune", "-2"),
                    ("loop_mode", "no_loop"),
                ],
            ),
            s("empty", &[]),
        ],
    };
    let want = "// 00 Piano: a Casio LK-110's tone 00, sampled by dexter-like110 0.1.0, 2026-11-04\n\
                // auto; 61 keys of 88; reference C4\n\
                <control> default_path=samples/\n\
                <global> volume=6.4 ampeg_release=0.35\n\
                <region> sample=060_C4.wav lokey=60 hikey=60 pitch_keycenter=60 tune=-2 loop_mode=no_loop\n\
                <empty>\n";
    assert_eq!(sfz.to_text().unwrap(), want);
}

#[test]
fn invalid() {
    let one = |sec: Section| Sfz { comments: vec![], sections: vec![sec] }.to_text();
    assert!(matches!(one(s("region", &[("sample", "a\nb")])), Err(Invalid::Value(_))));
    assert!(matches!(one(s("region", &[("sample", "")])), Err(Invalid::Value(_))));
    assert!(matches!(one(s("region", &[("sample", "a//b")])), Err(Invalid::Value(_))));
    assert!(matches!(one(s("region", &[("sample", "a<b")])), Err(Invalid::Value(_))));
    assert!(matches!(one(s("region", &[("sample", "a=b")])), Err(Invalid::Value(_))));
    assert!(matches!(one(s("region", &[("sample", "é")])), Err(Invalid::Value(_))));
    assert!(matches!(one(s("Region", &[])), Err(Invalid::Header(_))));
    assert!(matches!(one(s("", &[])), Err(Invalid::Header(_))));
    assert!(matches!(one(s("region", &[("Sample", "a")])), Err(Invalid::Opcode(_))));
    assert!(matches!(one(s("region", &[("", "a")])), Err(Invalid::Opcode(_))));
    let c = Sfz { comments: vec!["a\nb".into()], sections: vec![] };
    assert!(c.to_text().is_err());
}
