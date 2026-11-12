//! Without JACK: connections reconciled. Moved from `as-jack-keeper`'s tests.

use super::*;

struct Fake(BTreeSet<String>, BTreeSet<(String, String)>);

impl Graph for Fake {
    fn exists(&self, p: &str) -> bool {
        self.0.contains(p)
    }
    fn connected(&self, a: &str, b: &str) -> bool {
        self.1.contains(&(a.into(), b.into()))
    }
    fn connect(&mut self, a: &str, b: &str) -> Result<(), String> {
        self.1.insert((a.into(), b.into()));
        Ok(())
    }
    fn disconnect(&mut self, a: &str, b: &str) {
        self.1.remove(&(a.into(), b.into()));
    }
}

fn link(a: &str, b: &str) -> (String, String) {
    (a.into(), b.into())
}

#[test]
fn reconcile_makes_what_it_can_and_cuts_only_what_was_wanted() {
    let ports = ["brr:out_L", "brr:out_R", "system:playback_1", "mixer:live_01"];
    let mut g = Fake(ports.iter().map(|s| s.to_string()).collect(), Default::default());
    g.1.insert(link("brr:out_L", "mixer:live_01")); // someone else's: kept
    let want: BTreeSet<_> = [
        link("brr:out_L", "system:playback_1"),
        link("brr:out_L", "gone:in"),
        link("brr:out_R", "mixer:live_01"),
    ]
    .into();
    let said = reconcile(&mut g, &want, &[]);
    assert_eq!(said, vec!["connected brr:out_L > system:playback_1", "connected brr:out_R > mixer:live_01"]);
    let cut = [link("brr:out_R", "mixer:live_01")];
    let said = reconcile(&mut g, &[link("brr:out_L", "system:playback_1")].into(), &cut);
    assert_eq!(said, vec!["cut brr:out_R > mixer:live_01"]);
    assert!(g.connected("brr:out_L", "mixer:live_01"), "never someone else's");
}
