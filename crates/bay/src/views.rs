//! The line-mode views (`SYNTAX.md`, "Printed form"): `ls`, `ls X`, `ports X` and
//! `grid`. Everything they print is patch syntax: statements, `alias` lines and `#`
//! comments, so any view reads back in.
//!
//! Width: statement and `alias` lines are never cut. Comment lines are cut to the width
//! with `…`, and the grid leaves out the columns that don't fit and says so.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write;

use crate::bundle::{Bundles, Cell};
use crate::matrix::{Axis, Matrix, Pattern};
use crate::model::{ClientId, Direction, Link, PortId, PortType, Snapshot};
use crate::render;
use crate::syntax::{Name, Ref, Statement};

/// The names a view prints clients by: raw client name → alias. The session decides them
/// (step 3); views emit each alias's `alias` line before its first use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Names {
    aliases: HashMap<String, String>,
    /// Clients whose alias was generated, not defined in the session.
    generated: HashSet<String>,
}

impl Names {
    pub fn alias(&mut self, client: &str, alias: &str) {
        self.aliases.insert(client.to_owned(), alias.to_owned());
    }

    /// A generated alias: it resolves only where its `alias` line is printed.
    pub fn generated(&mut self, client: &str, alias: &str) {
        self.alias(client, alias);
        self.generated.insert(client.to_owned());
    }

    /// The name a typed statement can use: a session alias, else the raw name.
    pub fn typable<'a>(&'a self, client: &'a str) -> &'a str {
        if self.generated.contains(client) { client } else { self.get(client) }
    }

    pub fn get<'a>(&'a self, client: &'a str) -> &'a str {
        self.aliases.get(client).map_or(client, String::as_str)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    /// Cut comment lines (and the grid) to this many characters.
    pub width: Option<usize>,
}

/// Collects one output. `alias` lines appear once per output, before their first use.
struct Out<'a> {
    names: &'a Names,
    layout: Layout,
    emitted: HashSet<String>,
    text: String,
}

impl<'a> Out<'a> {
    fn new(names: &'a Names, layout: Layout) -> Out<'a> {
        Out { names, layout, emitted: HashSet::new(), text: String::new() }
    }

    /// A client's printed name, emitting its `alias` line the first time.
    fn client(&mut self, raw: &str) -> String {
        let name = self.names.get(raw);
        if name != raw && self.emitted.insert(raw.to_owned()) {
            let alias = Statement::Alias { name: name.to_owned(), target: raw.to_owned() };
            let _ = writeln!(self.text, "{alias}");
        }
        name.to_owned()
    }

    /// A statement from [`render`], whose refs are raw client names.
    fn stmt(&mut self, mut s: Statement) {
        match &mut s {
            Statement::Connect { from, to } | Statement::Disconnect { from, to } => {
                from.name = self.client(&from.name);
                to.name = self.client(&to.name);
            }
            Statement::Exact { client } => *client = self.client(client),
            _ => {}
        }
        let _ = writeln!(self.text, "{s}");
    }

    fn comment(&mut self, text: &str) {
        let line = if text.is_empty() { "#".to_owned() } else { format!("# {text}") };
        let _ = writeln!(self.text, "{}", cut(&line, self.layout.width));
    }
}

/// Cuts a line to `width` characters, ending in `…` when it was longer.
fn cut(line: &str, width: Option<usize>) -> String {
    match width {
        Some(w) if line.chars().count() > w => {
            let mut s: String = line.chars().take(w.saturating_sub(1)).collect();
            s.push('…');
            s
        }
        _ => line.to_owned(),
    }
}

/// Statements (a plan, a diff), with the `alias` lines they need.
pub fn statements(stmts: Vec<Statement>, names: &Names) -> String {
    let mut out = Out::new(names, Layout::default());
    for s in stmts {
        out.stmt(s);
    }
    out.text
}

/// What running a plan did: the applied part as statements, a `# failed:` comment per
/// failure, or `# no change`.
pub fn report(snap: &Snapshot, report: &crate::backend::Report, names: &Names) -> String {
    if report.applied.is_empty() && report.failures.is_empty() {
        return "# no change\n".to_owned();
    }
    let mut text = statements(render::plan(snap, &report.applied), names);
    for f in &report.failures {
        let (from, to) = (Ref::new(snap.port(f.link.from).name()), Ref::new(snap.port(f.link.to).name()));
        let stmt =
            if f.connect { Statement::Connect { from, to } } else { Statement::Disconnect { from, to } };
        let _ = writeln!(text, "# failed: {stmt}: {}", f.reason.replace('\n', " "));
    }
    text
}

/// `ls`: one block per client with outputs, headed by a summary comment.
pub fn ls(snap: &Snapshot, names: &Names, layout: Layout) -> String {
    let mut out = Out::new(names, layout);
    for (c, client) in snap.clients() {
        if !client.ports().iter().any(|&p| snap.port(p).dir() == Direction::Out) {
            continue;
        }
        let name = out.client(client.name());
        out.comment(&format!("{}: {}", Name(&name), summary(snap, c, Direction::Out)));
        for s in render::connects(snap, &links_where(snap, |l| snap.port(l.from).client() == c)) {
            out.stmt(s);
        }
    }
    out.text
}

/// Which links a snapshot holds, and which source clients it makes `exact`. The default
/// is the whole graph, every source client exact (`bay save`). Dexter's project routing
/// passes `dexter` for both: only links with an end on Dexter's ports, and `exact` for
/// Dexter alone, so the interface is only ever added to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SnapshotFilter {
    /// Links with an end matching one of these; none means every link.
    pub links: Vec<Pattern>,
    /// Source clients (by any port) matching one of these get an `exact` line; None means
    /// every source client in the graph.
    pub exact: Option<Vec<Pattern>>,
}

/// A snapshot: for each source client, a summary comment, its `exact` line, and its links
/// one to a line by full port names (`system:capture_1 > dexter:in_01`). Full names mean
/// the same links whatever the port counts when it's restored: a whole-client ref like
/// `system > dexter` wouldn't. `header` is its first comment line (a name and a time, from
/// the front end: core has no clock). Restore it with `compile_tolerant`, which skips
/// what isn't there and then withholds `exact` from a client it couldn't fully read.
pub fn snapshot(snap: &Snapshot, header: &str, filter: &SnapshotFilter) -> String {
    let names = Names::default();
    let mut out = Out::new(&names, Layout::default());
    out.text = format!("# {}\n", header.replace('\n', " "));
    let matches = |pats: &[Pattern], p: PortId| pats.iter().any(|m| m.matches(snap.port(p).name()));
    for (c, client) in snap.clients() {
        let outs: Vec<PortId> =
            client.ports().iter().copied().filter(|&p| snap.port(p).dir() == Direction::Out).collect();
        if outs.is_empty() {
            continue;
        }
        let links: Vec<Link> = outs
            .iter()
            .flat_map(|&f| snap.links_from(f).map(move |t| Link { from: f, to: t }))
            .filter(|l| {
                filter.links.is_empty() || matches(&filter.links, l.from) || matches(&filter.links, l.to)
            })
            .collect();
        let exact = match &filter.exact {
            None => true,
            Some(pats) => client.ports().iter().any(|&p| matches(pats, p)),
        };
        if links.is_empty() && !exact {
            continue;
        }
        out.comment(&format!("{}: {}", Name(client.name()), summary(snap, c, Direction::Out)));
        if exact {
            out.stmt(Statement::Exact { client: client.name().to_owned() });
        }
        for l in links {
            let (from, to) = (Ref::new(snap.port(l.from).name()), Ref::new(snap.port(l.to).name()));
            out.stmt(Statement::Connect { from, to });
        }
    }
    out.text
}

/// `ls X`: what one client feeds and what feeds it.
pub fn ls_client(snap: &Snapshot, c: ClientId, names: &Names, layout: Layout) -> String {
    let mut out = Out::new(names, layout);
    let name = out.client(snap.client(c).name());
    let dirs: Vec<String> = [Direction::Out, Direction::In]
        .into_iter()
        .filter(|&d| snap.client(c).ports().iter().any(|&p| snap.port(p).dir() == d))
        .map(|d| summary(snap, c, d))
        .collect();
    out.comment(&format!("{}: {}", Name(&name), dirs.join("; ")));
    for (title, dir) in [("feeds", Direction::Out), ("fed by", Direction::In)] {
        let links = links_where(snap, |l| {
            let end = if dir == Direction::Out { l.from } else { l.to };
            snap.port(end).client() == c
        });
        out.comment(if links.is_empty() { format!("{title} nothing") } else { title.to_owned() }.as_str());
        for s in render::connects(snap, &links) {
            out.stmt(s);
        }
    }
    out.text
}

/// `ports X`: every port of one client with its ordinal and link count, grouped by type
/// and direction. The first port of each bundle of two or more carries its label.
pub fn ports(snap: &Snapshot, bundles: &Bundles, c: ClientId, names: &Names, layout: Layout) -> String {
    let mut out = Out::new(names, layout);
    let name = out.client(snap.client(c).name());
    for (ty, dir) in SECTIONS {
        let ports = snap.ports_of(c, ty, dir);
        if ports.is_empty() {
            continue;
        }
        let kind =
            if ty == PortType::Midi { format!("midi {}", dir.as_str()) } else { dir.as_str().to_owned() };
        out.comment(&format!("{} {kind}", Name(&name)));
        let wide = ports.iter().map(|&p| snap.port(p).short_name().chars().count()).max().unwrap_or(0);
        for &p in &ports {
            let port = snap.port(p);
            let (b, pos) = bundles.of_port(p);
            let bundle = bundles.get(b);
            let label = if pos == 0 && bundle.ports.len() > 1 { bundle.label.as_str() } else { "" };
            let line = format!(
                "{:>3} {:<wide$}  ({}) {label}",
                port.ordinal(),
                port.short_name(),
                link_count(snap, p)
            );
            out.comment(line.trim_end());
        }
    }
    out.text
}

const SECTIONS: [(PortType, Direction); 4] = [
    (PortType::Audio, Direction::Out),
    (PortType::Audio, Direction::In),
    (PortType::Midi, Direction::Out),
    (PortType::Midi, Direction::In),
];

/// `grid`: output bundles (rows) against input bundles (lettered columns), all as
/// comments. `#` the bundles are fully paired (zip, fan-out or fan-in), `x` crossed
/// (channels swapped), `+` some other links, `.` none, and blank where the types differ.
/// It draws from a [`Matrix`], whose rows and columns are audio first, then MIDI, so the
/// blank cells between types form blocks.
pub fn grid(m: &Matrix, layout: Layout) -> String {
    let mut out = Out::new(&m.names, layout);
    let label = |out: &mut Out, a: &Axis| {
        let client = out.client(&a.client);
        let midi = if a.bundle.ty == PortType::Midi { " (midi)" } else { "" };
        format!("{} {}{midi}", Name(&client), a.bundle.label)
    };
    let (rows, cols) = (&m.rows, &m.cols);
    let row_labels: Vec<String> = rows.iter().map(|b| label(&mut out, b)).collect();
    let col_labels: Vec<String> = cols.iter().map(|b| label(&mut out, b)).collect();
    let letters: Vec<String> = (0..cols.len()).map(letter).collect();
    let cell = letters.last().map_or(1, String::len);

    // "# " + label + " " + cells of `cell` chars each followed by a space.
    let label_w = row_labels.iter().map(|l| l.chars().count()).max().unwrap_or(0).min(40);
    let fit = match layout.width {
        Some(w) => (w.saturating_sub(2 + label_w + 1) / (cell + 1)).min(cols.len()),
        None => cols.len(),
    };
    out.comment(&format!("grid: rows feed columns; {} full, + partial, x crossed, . none", "#"));
    let head: String = letters[..fit].iter().map(|l| format!("{l:<cell$} ")).collect();
    out.comment(format!("{:label_w$} {}", "", head).trim_end());
    for (r, label) in row_labels.iter().enumerate() {
        let cells: String = (0..fit).map(|c| format!("{:<cell$} ", state(m.cell(r, c).now))).collect();
        let name = cut(label, Some(label_w));
        out.comment(format!("{name:label_w$} {cells}").trim_end());
    }
    if fit < cols.len() {
        out.comment(&format!("{} more columns cut", cols.len() - fit));
    }
    for (i, l) in col_labels.iter().enumerate().take(fit) {
        out.comment(&format!("{:<cell$} {l}", letters[i]));
    }
    out.text
}

/// Column letters: a … z, then aa, ab, ….
fn letter(i: usize) -> String {
    let mut n = i;
    let mut s = Vec::new();
    loop {
        s.push(b'a' + (n % 26) as u8);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).expect("ascii")
}

fn state(cell: Cell) -> char {
    match cell {
        Cell::Types => ' ',
        Cell::Empty => '.',
        Cell::Full => '#',
        Cell::Crossed => 'x',
        Cell::Partial => '+',
    }
}

fn links_where(snap: &Snapshot, keep: impl Fn(&Link) -> bool) -> BTreeSet<Link> {
    snap.links().iter().copied().filter(|l| keep(l)).collect()
}

fn link_count(snap: &Snapshot, p: PortId) -> usize {
    match snap.port(p).dir() {
        Direction::Out => snap.links_from(p).count(),
        Direction::In => snap.links_to(p).count(),
    }
}

/// `6 out, [5-6] free` or `2 midi in, all free`: port counts per type, and unlinked runs.
fn summary(snap: &Snapshot, c: ClientId, dir: Direction) -> String {
    let mut parts = Vec::new();
    for ty in [PortType::Audio, PortType::Midi] {
        let ports = snap.ports_of(c, ty, dir);
        if ports.is_empty() {
            continue;
        }
        let midi = if ty == PortType::Midi { "midi " } else { "" };
        let mut part = format!("{} {midi}{}", ports.len(), dir.as_str());
        let free: Vec<u32> =
            ports.iter().filter(|&&p| link_count(snap, p) == 0).map(|&p| snap.port(p).ordinal()).collect();
        if free.len() == ports.len() {
            part += ", all free";
        } else if !free.is_empty() {
            let runs: Vec<String> = runs(&free)
                .into_iter()
                .map(|(a, b)| {
                    let r = if a == b { a.to_string() } else { format!("{a}-{b}") };
                    format!("[{midi}{r}]")
                })
                .collect();
            part += &format!(", {} free", runs.join(" "));
        }
        parts.push(part);
    }
    parts.join(", ")
}

/// Sorted numbers as inclusive runs: 1 2 3 5 → (1, 3), (5, 5).
fn runs(ns: &[u32]) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &n in ns {
        match out.last_mut() {
            Some((_, b)) if *b + 1 == n => *b = n,
            _ => out.push((n, n)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{Options, compile};
    use crate::fixture;
    use crate::matrix::View;
    use crate::resolve::Scope;
    use crate::session::Session;
    use crate::stage::Stage;
    use crate::syntax::parse;

    const FOCUSRITE_IN: &str = "alsa_input.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo";
    const FOCUSRITE_OUT: &str = "alsa_output.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo";

    fn studio() -> Snapshot {
        let extra = "out midi a2j:cap1\nin midi REAPER:MIDI in\nlink a2j:cap1 > REAPER:MIDI in\n";
        fixture::parse(&(include_str!("../testdata/studio.fake").to_owned() + extra)).unwrap()
    }

    fn names() -> Names {
        let mut n = Names::default();
        n.alias(FOCUSRITE_IN, "focusrite-in");
        n.alias(FOCUSRITE_OUT, "focusrite-out");
        n
    }

    /// Compiling a view against the snapshot it came from changes nothing.
    fn reads_back(snap: &Snapshot, text: &str) {
        let stmts = parse(text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        let plan = compile(&stmts, snap, &Scope::default(), Options { exact_all: true }).unwrap();
        assert!(plan.is_empty(), "{text}");
    }

    #[test]
    fn ls_view() {
        let s = studio();
        let text = ls(&s, &names(), Layout::default());
        assert_eq!(
            text,
            format!(
                "alias focusrite-in = {FOCUSRITE_IN}\n\
                 # focusrite-in: 2 out, [2] free\n\
                 focusrite-in[1] > REAPER[1-2]\n\
                 alias focusrite-out = {FOCUSRITE_OUT}\n\
                 # focusrite-out: 2 out, all free\n\
                 # REAPER: 6 out, [5-6] free\n\
                 REAPER[1-2] > focusrite-out\n\
                 REAPER[3-4] > Carla\n\
                 # Carla: 2 out\n\
                 Carla > REAPER[3-4]\n\
                 # Firefox: 2 out, [2] free\n\
                 Firefox[1] > OBS[1]\n\
                 # a2j: 1 midi out\n\
                 a2j[midi] > REAPER[midi]\n"
            )
        );
        reads_back(&s, &text);
    }

    #[test]
    fn snapshot_view() {
        let s = studio();
        let text = snapshot(&s, "bay snapshot: studio, 2026-09-26 19:04:05Z", &SnapshotFilter::default());
        assert!(text.starts_with(&format!(
            "# bay snapshot: studio, 2026-09-26 19:04:05Z\n\
             # {FOCUSRITE_IN}: 2 out, [2] free\n\
             exact {FOCUSRITE_IN}\n\
             {FOCUSRITE_IN}:capture_FL > REAPER:in1\n\
             {FOCUSRITE_IN}:capture_FL > REAPER:in2\n"
        )));
        assert!(
            text.contains("# Firefox: 2 out, [2] free\nexact Firefox\nFirefox:output_FL > OBS:input_FL\n")
        );
        reads_back(&s, &text);

        // Restoring it after changes puts every client back, one with no links included.
        let bare = fixture::parse("out A:o1\nout A:o2\nin B:i1\nin B:i2\nlink A:o1 > B:i1\n").unwrap();
        let saved = snapshot(&bare, "bay snapshot: bare", &SnapshotFilter::default());
        let changed = fixture::parse("out A:o1\nout A:o2\nin B:i1\nin B:i2\nlink A:o2 > B:i2\n").unwrap();
        let (plan, skipped) = restore(&saved, &changed);
        assert!(skipped.is_empty());
        assert_eq!(fixture::render(&changed.with_plan(&plan)), fixture::render(&bare));
    }

    fn restore(text: &str, on: &Snapshot) -> (crate::compile::Plan, Vec<crate::compile::Skipped>) {
        crate::compile::compile_tolerant(&parse(text).unwrap(), on, &Scope::default(), Options::default())
    }

    /// `n` system captures, the first three patched into Dexter's three inputs.
    fn rig(captures: usize) -> Snapshot {
        let mut text = String::new();
        for i in 1..=captures {
            text += &format!("out system:capture_{i}\n");
        }
        text += "in system:playback_1\nin system:playback_2\nin dexter:in_01\nin dexter:in_02\nin dexter:in_03\n\
                 out dexter:monitor_L\nout dexter:monitor_R\n";
        for i in 1..=3 {
            text += &format!("link system:capture_{i} > dexter:in_0{i}\n");
        }
        text += "link dexter:monitor_L > system:playback_1\nlink dexter:monitor_R > system:playback_2\n";
        fixture::parse(&text).unwrap()
    }

    /// Dexter's finding: saved with 3 captures, restored with 8. A whole-client ref would
    /// be skipped and `exact system` would then unplug every input. Full port names mean
    /// the same links whatever the counts.
    #[test]
    fn restoring_with_more_ports_deletes_nothing() {
        let saved = snapshot(&rig(3), "bay snapshot: rig", &SnapshotFilter::default());
        assert!(saved.contains("system:capture_1 > dexter:in_01\n"), "{saved}");
        let (plan, skipped) = restore(&saved, &rig(8));
        assert!(skipped.is_empty(), "{skipped:?}");
        assert!(plan.is_empty(), "{plan:?}");

        // The general form: save, add ports to a client, restore. Nothing that exists is
        // deleted, and every saved link is kept.
        let s = studio();
        let saved = snapshot(&s, "bay snapshot: studio", &SnapshotFilter::default());
        let more = fixture::parse(
            &(fixture::render(&s)
                + "out REAPER:out7\nout REAPER:out8\nin Carla:audio-in3\nout system:capture_1\n"),
        )
        .unwrap();
        let (plan, skipped) = restore(&saved, &more);
        assert!(skipped.is_empty(), "{skipped:?}");
        assert!(plan.del.is_empty() && plan.add.is_empty(), "{plan:?}");
    }

    /// If a client's links can't all be read, its `exact` isn't applied: a skip never
    /// turns into a cut.
    #[test]
    fn a_skip_withholds_exact() {
        let saved = snapshot(&rig(3), "bay snapshot: rig", &SnapshotFilter::default());
        // Dexter comes back with only two inputs: `... > dexter:in_03` can't be read.
        let fewer = fixture::parse(
            "out system:capture_1\nout system:capture_2\nout system:capture_3\nin system:playback_1\n\
             in system:playback_2\nin dexter:in_01\nin dexter:in_02\nout dexter:monitor_L\nout dexter:monitor_R\n\
             link system:capture_1 > dexter:in_01\nlink system:capture_3 > system:playback_1\n",
        )
        .unwrap();
        let (plan, skipped) = restore(&saved, &fewer);
        let reasons: Vec<(String, &str)> =
            skipped.iter().map(|k| (k.stmt.to_string(), k.error.kind.code())).collect();
        assert_eq!(
            reasons,
            [
                ("system:capture_3 > dexter:in_03".to_owned(), "unknown_name"),
                ("exact system".to_owned(), "withheld"),
            ]
        );
        assert!(skipped[1].error.msg.contains("1 statement(s) from it were skipped"));
        assert!(skipped[1].index.is_some(), "it points at the file's own exact line");
        // system's link that the file doesn't mention stays; what could be read is re-made.
        let named: Vec<String> =
            plan.links_named(&fewer).iter().map(|l| format!("{} {} > {}", l.add, l.from, l.to)).collect();
        assert_eq!(
            named,
            [
                "true system:capture_2 > dexter:in_02",
                "true dexter:monitor_L > system:playback_1",
                "true dexter:monitor_R > system:playback_2",
            ]
        );
    }

    /// Dexter's project routing: only links with an end on Dexter's ports, and `exact` for
    /// Dexter alone, never for the interface.
    #[test]
    fn snapshot_filters() {
        let mut g = rig(3);
        g = g.with_plan(&crate::compile::Plan {
            del: Default::default(),
            add: [crate::model::Link {
                from: g.port_named("system:capture_3").unwrap(),
                to: g.port_named("system:playback_2").unwrap(),
            }]
            .into(),
        });
        let dexter =
            SnapshotFilter { links: vec![Pattern::new("dexter")], exact: Some(vec![Pattern::new("dexter")]) };
        let text = snapshot(&g, "project routing", &dexter);
        assert!(!text.contains("exact system"), "{text}");
        assert!(text.contains("exact dexter\n"), "{text}");
        assert!(!text.contains("system:capture_3 > system:playback_2"), "{text}");
        assert!(text.contains("system:capture_1 > dexter:in_01\n"), "{text}");
        assert!(text.contains("dexter:monitor_L > system:playback_1\n"), "{text}");
        // Restoring it leaves the interface's other links alone.
        let (plan, _) = restore(&text, &g);
        assert!(plan.is_empty());
    }

    #[test]
    fn ls_client_view() {
        let s = studio();
        let text = ls_client(&s, s.client_named("REAPER").unwrap(), &names(), Layout::default());
        assert_eq!(
            text,
            "# REAPER: 6 out, [5-6] free; 4 in, 1 midi in\n\
             # feeds\n\
             alias focusrite-out = alsa_output.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo\n\
             REAPER[1-2] > focusrite-out\n\
             REAPER[3-4] > Carla\n\
             # fed by\n\
             alias focusrite-in = alsa_input.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo\n\
             focusrite-in[1] > REAPER[1-2]\n\
             Carla > REAPER[3-4]\n\
             a2j[midi] > REAPER[midi]\n"
        );
        reads_back(&s, &text);
        let obs = ls_client(&s, s.client_named("OBS").unwrap(), &names(), Layout::default());
        assert_eq!(obs, "# OBS: 2 in, [2] free\n# feeds nothing\n# fed by\nFirefox[1] > OBS[1]\n");
    }

    #[test]
    fn ports_view() {
        let s = studio();
        let (b, _) = Bundles::derive(&s, &[]);
        let text = ports(&s, &b, s.client_named("REAPER").unwrap(), &names(), Layout::default());
        assert_eq!(
            text,
            "# REAPER out\n\
             #   1 out1  (1) out1/2\n\
             #   2 out2  (1)\n\
             #   3 out3  (1) out3/4\n\
             #   4 out4  (1)\n\
             #   5 out5  (0) out5/6\n\
             #   6 out6  (0)\n\
             # REAPER in\n\
             #   1 in1  (1) in1/2\n\
             #   2 in2  (1)\n\
             #   3 in3  (1) in3/4\n\
             #   4 in4  (1)\n\
             # REAPER midi in\n\
             #   1 MIDI in  (1)\n"
        );
        reads_back(&s, &text);
    }

    #[test]
    fn grid_view() {
        let s = studio();
        let text =
            grid(&Matrix::build(&s, &Session::new(), &Stage::new(), &View::default()), Layout::default());
        assert_eq!(
            text,
            format!(
                "alias focusrite-in = {FOCUSRITE_IN}\n\
                 alias focusrite-out = {FOCUSRITE_OUT}\n\
                 # grid: rows feed columns; # full, + partial, x crossed, . none\n\
                 #                             a b c d e f\n\
                 # focusrite-in capture_FL/FR  . + . . .\n\
                 # focusrite-out monitor_FL/FR . . . . .\n\
                 # REAPER out1/2               # . . . .\n\
                 # REAPER out3/4               . . . # .\n\
                 # REAPER out5/6               . . . . .\n\
                 # Carla audio-out1/2          . . # . .\n\
                 # Firefox output_FL/FR        . . . . +\n\
                 # a2j cap1 (midi)                       #\n\
                 # a focusrite-out playback_FL/FR\n\
                 # b REAPER in1/2\n\
                 # c REAPER in3/4\n\
                 # d Carla audio-in1/2\n\
                 # e OBS input_FL/FR\n\
                 # f REAPER MIDI in (midi)\n"
            )
        );
        reads_back(&s, &text);

        let narrow = grid(
            &Matrix::build(&s, &Session::new(), &Stage::new(), &View::default()),
            Layout { width: Some(36) },
        );
        assert!(narrow.contains("# 3 more columns cut\n"), "{narrow}");
        assert!(narrow.lines().filter(|l| l.starts_with('#')).all(|l| l.chars().count() <= 36), "{narrow}");
        reads_back(&s, &narrow);
    }

    #[test]
    fn width_cuts_comments_only() {
        let s = studio();
        let text = ls(&s, &Names::default(), Layout { width: Some(20) });
        for line in text.lines() {
            if line.starts_with('#') {
                assert!(line.chars().count() <= 20, "{line}");
            }
        }
        assert!(text.contains(&format!("{FOCUSRITE_IN}[1] > REAPER[1-2]\n")));
        reads_back(&s, &text);
    }

    #[test]
    fn report_view() {
        use crate::backend::{Backend, Report};
        let mut fake = crate::fake::Fake::from_fixture(include_str!("../testdata/studio.fake")).unwrap();
        fake.fail_on("OBS:input_FR");
        let s = fake.snapshot().unwrap();
        let plan = compile(
            &parse("Carla !> REAPER; Firefox > OBS").unwrap(),
            &s,
            &Scope::default(),
            Options::default(),
        )
        .unwrap();
        let text = report(&s, &fake.run(&s, &plan), &names());
        assert_eq!(
            text,
            "Carla !> REAPER\n# failed: Firefox:output_FR > OBS:input_FR: the fake backend was told to fail\n"
        );
        assert_eq!(report(&s, &Report::default(), &names()), "# no change\n");
    }

    #[test]
    fn letters() {
        let l: Vec<String> = [0, 25, 26, 27, 51, 52, 701, 702].into_iter().map(letter).collect();
        assert_eq!(l, ["a", "z", "aa", "ab", "az", "ba", "zz", "aaa"]);
    }
}
