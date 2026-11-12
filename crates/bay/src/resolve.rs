//! Resolving refs to ports against a snapshot (`SYNTAX.md`, "Refs"): aliases, bundle
//! names, forgiving client names, `client:port`, and selectors.

use std::collections::HashMap;

use crate::bundle::BundleDecl;
use crate::error::ErrorKind::{
    AliasLoop, AmbiguousName, OutOfRange, Redefined, Syntax, TypeMismatch, UnknownName, WrongSide,
};
use crate::error::Problem;
use crate::model::{ClientId, Direction, PortId, PortType, Snapshot};
use crate::syntax::{Ref, Selector};

/// Aliases are followed at most this many times.
const MAX_HOPS: usize = 8;

/// The names one input (a patch, or the session) defines. Bundle targets stay unresolved,
/// so the same scope works against every snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scope {
    aliases: HashMap<String, String>,
    bundles: HashMap<String, Ref>,
}

impl Scope {
    /// Defines an alias. Redefining a name with a different meaning is an error.
    pub fn alias(&mut self, name: &str, target: &str) -> Result<(), Problem> {
        self.check_free(name, self.aliases.get(name).is_some_and(|t| t == target))?;
        self.aliases.insert(name.to_owned(), target.to_owned());
        Ok(())
    }

    /// Defines a bundle. The target's selector must carry a direction.
    pub fn bundle(&mut self, name: &str, target: &Ref) -> Result<(), Problem> {
        self.check_free(name, self.bundles.get(name) == Some(target))?;
        self.bundles.insert(name.to_owned(), target.clone());
        Ok(())
    }

    fn check_free(&self, name: &str, same: bool) -> Result<(), Problem> {
        if !same && (self.aliases.contains_key(name) || self.bundles.contains_key(name)) {
            return Err(Problem::new(Redefined, format!("{name} is already defined differently")));
        }
        Ok(())
    }

    /// Defines or replaces a name (an alias or bundle of the same name goes).
    pub fn set(&mut self, def: Definition) {
        match def {
            Definition::Alias { name, target } => {
                self.bundles.remove(&name);
                self.aliases.insert(name, target);
            }
            Definition::Bundle { name, target } => {
                self.aliases.remove(&name);
                self.bundles.insert(name, target);
            }
        }
    }

    /// Removes a name. False if it wasn't defined.
    pub fn remove(&mut self, name: &str) -> bool {
        self.aliases.remove(name).is_some() | self.bundles.remove(name).is_some()
    }

    pub fn aliases(&self) -> impl Iterator<Item = (&str, &str)> {
        self.aliases.iter().map(|(n, t)| (n.as_str(), t.as_str()))
    }

    pub fn bundles(&self) -> impl Iterator<Item = (&str, &Ref)> {
        self.bundles.iter().map(|(n, r)| (n.as_str(), r))
    }
}

/// A name a scope can hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Definition {
    Alias { name: String, target: String },
    Bundle { name: String, target: Ref },
}

/// Resolves names against a snapshot through scopes, innermost first.
pub struct Resolver<'a> {
    snap: &'a Snapshot,
    scopes: Vec<&'a Scope>,
}

impl<'a> Resolver<'a> {
    pub fn new(snap: &'a Snapshot, scopes: Vec<&'a Scope>) -> Resolver<'a> {
        Resolver { snap, scopes }
    }

    /// The ports a ref selects on one side of `>`: outputs on the left, inputs on the right.
    pub fn ports(&self, r: &Ref, side: Direction) -> Result<Vec<PortId>, Problem> {
        if r.sel.dir.is_some_and(|d| d != side) {
            return Err(Problem::new(WrongSide, format!("{r} is on the {} side of >", side_word(side))));
        }
        let name = self.follow(&r.name)?;
        if !self.is_exact(name)
            && let Some(target) = self.lookup_bundle(name)
        {
            return self.bundle_ports(name, target, &r.sel, side);
        }
        if name.contains(':') {
            if !r.sel.is_empty() {
                return Err(Problem::new(Syntax, format!("{r}: a port takes no [ ]")));
            }
            return self.port(name, side).map(|p| vec![p]);
        }
        let client = self.client(name)?;
        self.select(client, r.sel.ty, r.sel.range, side, &r.to_string())
    }

    /// A client by name, for `exact`: aliases, then the forgiving client match.
    pub fn client_of(&self, name: &str) -> Result<ClientId, Problem> {
        self.client(self.follow(name)?)
    }

    /// A declared bundle resolved to ordinals, for [`crate::Bundles::derive`].
    pub fn bundle_decl(&self, name: &str, target: &Ref) -> Result<BundleDecl, Problem> {
        let dir =
            target.sel.dir.ok_or_else(|| Problem::new(Syntax, format!("bundle {name} needs a direction")))?;
        let client = self.client(self.follow(&target.name)?)?;
        let ports = self.select(client, target.sel.ty, target.sel.range, dir, &target.to_string())?;
        let first = self.snap.port(ports[0]);
        Ok(BundleDecl {
            name: name.to_owned(),
            client: self.snap.client(client).name().to_owned(),
            ty: first.ty(),
            dir,
            first: first.ordinal(),
            last: first.ordinal() + ports.len() as u32 - 1,
        })
    }

    /// Every bundle visible through the scopes, inner ones shadowing outer ones.
    pub fn bundle_decls(&self) -> (Vec<BundleDecl>, Vec<Problem>) {
        let (mut decls, mut problems, mut seen) = (Vec::new(), Vec::new(), Vec::new());
        for scope in &self.scopes {
            for (name, target) in scope.bundles() {
                if seen.contains(&name) {
                    continue;
                }
                seen.push(name);
                match self.bundle_decl(name, target) {
                    Ok(d) => decls.push(d),
                    Err(e) => problems.push(e),
                }
            }
        }
        (decls, problems)
    }

    fn follow<'n>(&'n self, name: &'n str) -> Result<&'n str, Problem> {
        let mut n = name;
        for _ in 0..=MAX_HOPS {
            if self.is_exact(n) {
                return Ok(n);
            }
            match self.scopes.iter().find_map(|s| s.aliases.get(n)) {
                Some(t) => n = t,
                None => return Ok(n),
            }
        }
        Err(Problem::new(
            AliasLoop,
            format!("alias {name} goes round in circles (or more than {MAX_HOPS} hops)"),
        ))
    }

    /// An exact client or full port name, which no alias or bundle can shadow, so a
    /// printed raw name always reads back as itself.
    fn is_exact(&self, name: &str) -> bool {
        self.snap.client_named(name).is_some() || self.snap.port_named(name).is_some()
    }

    fn lookup_bundle(&self, name: &str) -> Option<&Ref> {
        self.scopes.iter().find_map(|s| s.bundles.get(name))
    }

    fn bundle_ports(
        &self,
        name: &str,
        target: &Ref,
        sel: &Selector,
        side: Direction,
    ) -> Result<Vec<PortId>, Problem> {
        let decl = self.bundle_decl(name, target)?;
        if decl.dir != side {
            let dir = if decl.dir == Direction::Out { "outputs" } else { "inputs" };
            return Err(Problem::new(
                WrongSide,
                format!("bundle {name} is {dir}, but it's on the {} side of >", side_word(side)),
            ));
        }
        if sel.ty.is_some_and(|t| t != decl.ty) {
            return Err(Problem::new(TypeMismatch, format!("bundle {name} is {}", decl.ty.as_str())));
        }
        let client = self.snap.client_named(&decl.client).expect("bundle_decl found it");
        let all = self.snap.ports_of(client, decl.ty, decl.dir);
        let ports = &all[decl.first as usize - 1..decl.last as usize];
        within(ports, sel.range, || format!("bundle {name} has {} ports", ports.len()))
    }

    fn port(&self, name: &str, side: Direction) -> Result<PortId, Problem> {
        let check = |p: PortId| {
            let dir = self.snap.port(p).dir();
            if dir == side {
                Ok(p)
            } else {
                let what = if dir == Direction::Out { "an output" } else { "an input" };
                Err(Problem::new(
                    WrongSide,
                    format!("{name} is {what}, but it's on the {} side of >", side_word(side)),
                ))
            }
        };
        if let Some(p) = self.snap.port_named(name) {
            return check(p);
        }
        let (client, port) = name.split_once(':').expect("port refs contain ':'");
        let client = self.client(self.follow(client)?)?;
        let candidates: Vec<(PortId, &str)> = self
            .snap
            .client(client)
            .ports()
            .iter()
            .map(|&p| (p, self.snap.port(p)))
            .filter(|(_, p)| p.dir() == side)
            .map(|(id, p)| (id, p.short_name()))
            .collect();
        match forgiving(port, &candidates) {
            Found::One(p) => check(p),
            Found::None => Err(Problem::new(UnknownName, format!("no {} {name}", side_word(side)))),
            Found::Many(names) => Err(Problem::new(
                AmbiguousName,
                format!("{name} matches several ports: {}", names.join(", ")),
            )),
        }
    }

    fn client(&self, name: &str) -> Result<ClientId, Problem> {
        if let Some(c) = self.snap.client_named(name) {
            return Ok(c);
        }
        let candidates: Vec<(ClientId, &str)> = self.snap.clients().map(|(id, c)| (id, c.name())).collect();
        match forgiving(name, &candidates) {
            Found::One(c) => Ok(c),
            Found::None => {
                Err(Problem::new(UnknownName, format!("no client, bundle or alias matches {name}")))
            }
            Found::Many(names) => Err(Problem::new(
                AmbiguousName,
                format!("{name} matches several clients: {}", names.join(", ")),
            )),
        }
    }

    /// A client's ports of one side, by type (audio, else MIDI when there's no audio on
    /// that side) and range.
    fn select(
        &self,
        client: ClientId,
        ty: Option<PortType>,
        range: Option<(u32, u32)>,
        side: Direction,
        shown: &str,
    ) -> Result<Vec<PortId>, Problem> {
        let cname = self.snap.client(client).name();
        let ty = ty.unwrap_or_else(|| {
            let audio = self.snap.ports_of(client, PortType::Audio, side);
            if audio.is_empty() && !self.snap.ports_of(client, PortType::Midi, side).is_empty() {
                PortType::Midi
            } else {
                PortType::Audio
            }
        });
        let ports = self.snap.ports_of(client, ty, side);
        let what = format!("{} {}", ty.as_str(), if side == Direction::Out { "outputs" } else { "inputs" });
        if ports.is_empty() {
            return Err(Problem::new(OutOfRange, format!("{shown}: {cname} has no {what}")));
        }
        within(&ports, range, || format!("{shown}: {cname} has {} {what}", ports.len()))
    }
}

/// The 1-based `range` of `ports`, all of them without one.
fn within(
    ports: &[PortId],
    range: Option<(u32, u32)>,
    too_few: impl Fn() -> String,
) -> Result<Vec<PortId>, Problem> {
    match range {
        None => Ok(ports.to_vec()),
        Some((a, b)) if b as usize <= ports.len() => Ok(ports[a as usize - 1..b as usize].to_vec()),
        Some(_) => Err(Problem::new(OutOfRange, too_few())),
    }
}

fn side_word(side: Direction) -> &'static str {
    if side == Direction::Out { "output" } else { "input" }
}

enum Found<T> {
    None,
    One(T),
    Many(Vec<String>),
}

/// Exact, then a single case-insensitive match, then a single case-insensitive substring.
fn forgiving<T: Copy>(want: &str, candidates: &[(T, &str)]) -> Found<T> {
    if let Some(&(t, _)) = candidates.iter().find(|(_, n)| *n == want) {
        return Found::One(t);
    }
    let lower = want.to_lowercase();
    let exact: Vec<_> = candidates.iter().filter(|(_, n)| n.to_lowercase() == lower).collect();
    let hits = if exact.is_empty() {
        candidates.iter().filter(|(_, n)| n.to_lowercase().contains(&lower)).collect()
    } else {
        exact
    };
    match hits.as_slice() {
        [] => Found::None,
        [(t, _)] => Found::One(*t),
        many => Found::Many(many.iter().map(|(_, n)| n.to_string()).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;
    use crate::syntax::parse;

    fn snap() -> Snapshot {
        let text = include_str!("../testdata/studio.fake").to_owned()
            + "out midi a2j:midi_capture_1\nin midi REAPER:MIDI in\nout midi REAPER:MIDI out\n";
        fixture::parse(&text).unwrap()
    }

    fn r(text: &str) -> Ref {
        match parse(&format!("{text} > x")).unwrap().remove(0).stmt {
            crate::syntax::Statement::Connect { from, .. } => from,
            _ => unreachable!(),
        }
    }

    fn names(s: &Snapshot, ports: &[PortId]) -> Vec<String> {
        ports.iter().map(|&p| s.port(p).name().to_owned()).collect()
    }

    #[test]
    fn clients_and_ranges() {
        let s = snap();
        let session = Scope::default();
        let res = Resolver::new(&s, vec![&session]);
        let got = |t: &str, side| res.ports(&r(t), side).map(|p| names(&s, &p)).map_err(|e| e.msg);
        assert_eq!(got("rea[2-3]", Direction::Out).unwrap(), ["REAPER:out2", "REAPER:out3"]);
        assert_eq!(
            got("REAPER", Direction::In).unwrap(),
            ["REAPER:in1", "REAPER:in2", "REAPER:in3", "REAPER:in4"]
        );
        assert_eq!(got("reaper[midi]", Direction::In).unwrap(), ["REAPER:MIDI in"]);
        // No audio outputs, so MIDI.
        assert_eq!(got("a2j", Direction::Out).unwrap(), ["a2j:midi_capture_1"]);
        assert_eq!(got("REAPER[7]", Direction::Out).unwrap_err(), "REAPER[7]: REAPER has 6 audio outputs");
        assert_eq!(got("OBS", Direction::Out).unwrap_err(), "OBS: OBS has no audio outputs");
        assert_eq!(
            got("alsa", Direction::Out).unwrap_err(),
            "alsa matches several clients: alsa_input.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo, \
             alsa_output.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo"
        );
        assert_eq!(got("nope", Direction::Out).unwrap_err(), "no client, bundle or alias matches nope");
        assert_eq!(got("REAPER[in]", Direction::Out).unwrap_err(), "REAPER[in] is on the output side of >");
    }

    #[test]
    fn ports_by_name() {
        let s = snap();
        let session = Scope::default();
        let res = Resolver::new(&s, vec![&session]);
        let got = |t: &str, side| res.ports(&r(t), side).map(|p| names(&s, &p)).map_err(|e| e.msg);
        assert_eq!(got("Firefox:output_FR", Direction::Out).unwrap(), ["Firefox:output_FR"]);
        assert_eq!(got("fire:fr", Direction::Out).unwrap(), ["Firefox:output_FR"]);
        assert_eq!(got("obs:input_fl", Direction::In).unwrap(), ["OBS:input_FL"]);
        assert_eq!(got("REAPER:MIDI", Direction::Out).unwrap(), ["REAPER:MIDI out"]);
        assert_eq!(got("REAPER:out", Direction::Out).unwrap_err().split(':').next(), Some("REAPER"));
        assert_eq!(
            got("OBS:input_FL", Direction::Out).unwrap_err(),
            "OBS:input_FL is an input, but it's on the output side of >"
        );
        assert_eq!(got("OBS:x", Direction::In).unwrap_err(), "no input OBS:x");
        assert_eq!(
            got("OBS:input_FL[1]", Direction::In).unwrap_err(),
            "OBS:input_FL[1]: a port takes no [ ]"
        );
    }

    #[test]
    fn aliases_and_bundles_through_scopes() {
        let s = snap();
        let mut session = Scope::default();
        session.alias("phones", "alsa_output.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo").unwrap();
        session.alias("daw", "REAPER").unwrap();
        session.bundle("stems", &r("daw[out 3-6]")).unwrap();
        let mut patch = Scope::default();
        patch.alias("daw", "Carla").unwrap();
        patch.alias("loop", "loop").unwrap();

        let res = Resolver::new(&s, vec![&patch, &session]);
        let got = |t: &str, side| res.ports(&r(t), side).map(|p| names(&s, &p)).map_err(|e| e.msg);
        assert_eq!(got("phones[1]", Direction::In).unwrap().len(), 1);
        // The patch's daw shadows the session's, and the bundle follows it.
        assert_eq!(got("daw", Direction::Out).unwrap(), ["Carla:audio-out1", "Carla:audio-out2"]);
        assert_eq!(got("stems", Direction::Out).unwrap_err(), "daw[out 3-6]: Carla has 2 audio outputs");

        let res = Resolver::new(&s, vec![&session]);
        let got = |t: &str, side| res.ports(&r(t), side).map(|p| names(&s, &p)).map_err(|e| e.msg);
        assert_eq!(got("stems[2-3]", Direction::Out).unwrap(), ["REAPER:out4", "REAPER:out5"]);
        assert_eq!(got("stems[5]", Direction::Out).unwrap_err(), "bundle stems has 4 ports");
        assert_eq!(
            got("stems", Direction::In).unwrap_err(),
            "bundle stems is outputs, but it's on the input side of >"
        );
        assert_eq!(got("stems[midi]", Direction::Out).unwrap_err(), "bundle stems is audio");
        let (decls, problems) = res.bundle_decls();
        assert!(problems.is_empty());
        assert_eq!((decls[0].client.as_str(), decls[0].first, decls[0].last), ("REAPER", 3, 6));

        let res = Resolver::new(&s, vec![&patch]);
        assert!(res.ports(&r("loop"), Direction::Out).unwrap_err().msg.contains("round in circles"));
    }

    #[test]
    fn exact_names_beat_aliases_and_bundles() {
        let s = snap();
        let mut session = Scope::default();
        session.alias("REAPER", "Carla").unwrap();
        session.bundle("OBS", &r("REAPER[in 1-2]")).unwrap();
        session.alias("Firefox:output_FL", "Carla:audio-out1").unwrap();
        let res = Resolver::new(&s, vec![&session]);
        let got = |t: &str, side| res.ports(&r(t), side).map(|p| names(&s, &p)).map_err(|e| e.msg);
        assert_eq!(got("REAPER[1]", Direction::Out).unwrap(), ["REAPER:out1"]);
        assert_eq!(got("OBS", Direction::In).unwrap(), ["OBS:input_FL", "OBS:input_FR"]);
        assert_eq!(got("Firefox:output_FL", Direction::Out).unwrap(), ["Firefox:output_FL"]);
        // Forgiving matches still come after aliases.
        let mut session = Scope::default();
        session.alias("rea", "Carla").unwrap();
        let res = Resolver::new(&s, vec![&session]);
        assert_eq!(
            names(&s, &res.ports(&r("rea"), Direction::Out).unwrap()),
            ["Carla:audio-out1", "Carla:audio-out2"]
        );
    }

    #[test]
    fn scope_redefinition() {
        let mut sc = Scope::default();
        sc.alias("a", "x").unwrap();
        sc.alias("a", "x").unwrap();
        assert_eq!(sc.alias("a", "y").unwrap_err().msg, "a is already defined differently");
        assert_eq!(sc.bundle("a", &r("x[out]")).unwrap_err().msg, "a is already defined differently");
    }
}
