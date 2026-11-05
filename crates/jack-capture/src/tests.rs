use super::*;

#[test]
fn links_names() {
    let l = links(
        "cap",
        &["system:capture_1".to_string()],
        &["system:capture_2".to_string(), "x:out".to_string()],
    );
    assert_eq!(l.len(), 3);
    assert!(l.contains(&("system:capture_1".to_string(), "cap:in_L".to_string())));
    assert!(l.contains(&("system:capture_2".to_string(), "cap:in_R".to_string())));
    assert!(l.contains(&("x:out".to_string(), "cap:in_R".to_string())));
}

#[test]
fn away_before_jack() {
    // No server named so: the capture waits, and says Away.
    let mut c = Capture::jack("captest-none", BTreeSet::new(), 0.1);
    let (mut l, mut r) = (vec![], vec![]);
    assert_eq!(c.read(&mut l, &mut r, 100), Chunk::Away);
    c.stop();
}
