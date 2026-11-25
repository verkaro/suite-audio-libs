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

/// [`reconcile`] or [`reconcile_make_first`].
type Order = fn(&mut dyn Graph, &BTreeSet<(String, String)>, &[(String, String)]) -> Vec<String>;

/// A destination playing one source, moved to another: what each order says it did.
fn moved(order: Order) -> Vec<String> {
    let ports = ["recv:track_3", "recv:track_9", "card:playback_1"];
    let mut g = Fake(ports.iter().map(|s| s.to_string()).collect(), Default::default());
    g.1.insert(link("recv:track_3", "card:playback_1"));
    order(
        &mut g,
        &[link("recv:track_9", "card:playback_1")].into(),
        &[link("recv:track_3", "card:playback_1")],
    )
}

#[test]
fn a_moved_destination_has_its_old_source_cut_before_the_new_is_made() {
    let said = moved(reconcile);
    assert_eq!(said, vec!["cut recv:track_3 > card:playback_1", "connected recv:track_9 > card:playback_1"]);
}

#[test]
fn asked_to_make_first_the_new_source_is_there_before_the_old_is_cut() {
    let said = moved(reconcile_make_first);
    assert_eq!(said, vec!["connected recv:track_9 > card:playback_1", "cut recv:track_3 > card:playback_1"]);
}
