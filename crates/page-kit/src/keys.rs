//! A tool's keys held to the family's (`STYLE.md`, "Keys"; `KEYS` in `page-kit.js`).

use crate::JS;

/// Checks a tool's declared keys (its `keys.json`: a list of `{"key", "does", "screen"}`, with
/// `"note"` to say where, and `"differs"`, the reason, when a common key means something else)
/// against the family's table, and its pages' sources against that list. Lists everything wrong:
///
/// - a common key whose `does` isn't the table's, word for word, with no `differs`. So a change
///   to the table fails here when a tool's pin moves onto it, until the tool has dealt with it;
/// - a key a page compares against (`k === "x"`, `e.key === "x"`) that isn't declared. That's a
///   scan for those literal comparisons, so a net with holes in it.
pub fn check_keys(declared: &str, pages: &[&str]) -> Result<(), String> {
    let common = common();
    let list = match Reader::new(declared).value() {
        Some(J::Arr(list)) => list,
        _ => return Err("keys.json: not a list".into()),
    };
    let mut wrong = Vec::new();
    let mut keys = Vec::new();
    for e in &list {
        let (Some(key), Some(does), Some(screen)) = (e.get("key"), e.get("does"), e.get("screen")) else {
            wrong.push(format!("keys.json: an entry wants key, does and screen: {e:?}"));
            continue;
        };
        keys.push(key.to_string());
        let means = common.iter().find(|(k, _)| k == key).map(|(_, m)| m);
        if let Some(means) = means
            && means != does
            && e.get("differs").is_none_or(str::is_empty)
        {
            wrong.push(format!(
                "{key} on {screen}: says \"{does}\"; the family's is \"{means}\" (or say why, in differs)"
            ));
        }
    }
    for page in pages {
        for key in compared(page) {
            if !keys.contains(&key) && !wrong.iter().any(|w| w.starts_with(&format!("{key}: "))) {
                wrong.push(format!("{key}: the page handles it; keys.json doesn't list it"));
            }
        }
    }
    if wrong.is_empty() { Ok(()) } else { Err(wrong.join("\n")) }
}

/// The table, read from the script.
fn common() -> Vec<(String, String)> {
    let at = JS.find("const KEYS = ").expect("KEYS") + "const KEYS = ".len();
    let Some(J::Arr(list)) = Reader::new(&JS[at..]).value() else { panic!("KEYS isn't JSON") };
    list.iter()
        .map(|e| (e.get("key").expect("key").to_string(), e.get("means").expect("means").to_string()))
        .collect()
}

/// Every string a page compares a key against: `k === "x"`, `key === "x"`, `e.key === "x"`.
fn compared(page: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(n) = page[from..].find("===") {
        let at = from + n;
        from = at + 3;
        let before = page[..at].trim_end();
        let name = before.rsplit(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or("");
        let start_of_name = before.len() - name.len();
        // `k` must be a whole name, or `.key` a property's; `o.k` is something else.
        let dotted = page[..start_of_name].ends_with('.');
        if !(name == "key" || (name == "k" && !dotted)) {
            continue;
        }
        let Some(rest) = page[from..].trim_start().strip_prefix('"') else { continue };
        if let Some(end) = rest.find('"') {
            let key = rest[..end].to_string();
            if !out.contains(&key) {
                out.push(key);
            }
        }
    }
    out
}

/// As much JSON as the table and a tool's list are written in.
#[derive(Debug)]
enum J {
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    fn get(&self, name: &str) -> Option<&str> {
        let J::Obj(fields) = self else { return None };
        match fields.iter().find(|(k, _)| k == name) {
            Some((_, J::Str(s))) => Some(s),
            _ => None,
        }
    }
}

struct Reader<'a>(std::iter::Peekable<std::str::Chars<'a>>);

impl<'a> Reader<'a> {
    fn new(text: &'a str) -> Self {
        Reader(text.chars().peekable())
    }

    fn gap(&mut self) {
        while self.0.next_if(|c| c.is_whitespace()).is_some() {}
    }

    fn eat(&mut self, c: char) -> Option<()> {
        self.gap();
        self.0.next_if_eq(&c).map(|_| ())
    }

    fn value(&mut self) -> Option<J> {
        self.gap();
        match *self.0.peek()? {
            '"' => self.string().map(J::Str),
            '[' => {
                self.0.next();
                let mut list = Vec::new();
                if self.eat(']').is_some() {
                    return Some(J::Arr(list));
                }
                loop {
                    list.push(self.value()?);
                    if self.eat(']').is_some() {
                        return Some(J::Arr(list));
                    }
                    self.eat(',')?;
                }
            }
            '{' => {
                self.0.next();
                let mut fields = Vec::new();
                if self.eat('}').is_some() {
                    return Some(J::Obj(fields));
                }
                loop {
                    self.gap();
                    let name = self.string()?;
                    self.eat(':')?;
                    fields.push((name, self.value()?));
                    if self.eat('}').is_some() {
                        return Some(J::Obj(fields));
                    }
                    self.eat(',')?;
                }
            }
            _ => None,
        }
    }

    fn string(&mut self) -> Option<String> {
        self.0.next_if_eq(&'"')?;
        let mut s = String::new();
        loop {
            match self.0.next()? {
                '"' => return Some(s),
                '\\' => match self.0.next()? {
                    'n' => s.push('\n'),
                    't' => s.push('\t'),
                    c @ ('"' | '\\' | '/') => s.push(c),
                    _ => return None,
                },
                c => s.push(c),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = r#"[
        {"key": "j", "does": "the next", "note": "a row", "screen": "grid"},
        {"key": "t", "does": "a mark", "screen": "grid"}
    ]"#;

    #[test]
    fn the_table_reads() {
        let t = common();
        assert!(t.iter().any(|(k, m)| k == "s" && m == "stop, then to the start"));
        assert!(t.iter().any(|(k, _)| k == " "));
        assert!(t.len() >= 20);
    }

    #[test]
    fn a_list_that_keeps_to_the_table_passes() {
        assert_eq!(check_keys(OK, &[r#"if (k === "j") go(); else if (e.key === "t") mark();"#]), Ok(()));
    }

    #[test]
    fn a_common_key_used_for_something_else_is_refused_unless_it_says_why() {
        let bad = r#"[{"key": "s", "does": "solo", "screen": "grid"}]"#;
        assert!(check_keys(bad, &[]).unwrap_err().contains("s on grid"));
        let why = r#"[{"key": "s", "does": "solo", "screen": "grid", "differs": "because"}]"#;
        assert_eq!(check_keys(why, &[]), Ok(()));
        let empty = r#"[{"key": "s", "does": "solo", "screen": "grid", "differs": ""}]"#;
        assert!(check_keys(empty, &[]).is_err());
    }

    #[test]
    fn a_key_the_page_handles_must_be_listed() {
        let e = check_keys(OK, &[r#"k === "w" || e.key === "Backspace""#]).unwrap_err();
        assert!(e.contains("w: the page") && e.contains("Backspace: the page"), "{e}");
    }

    #[test]
    fn other_comparisons_are_not_keys() {
        let page = r#"o.kind === "type"; w === "page"; o.k === "nope"; sel === k; k === other;"#;
        assert_eq!(check_keys(OK, &[page]), Ok(()));
    }

    #[test]
    fn a_list_that_is_not_one_is_said() {
        assert!(check_keys("{}", &[]).is_err());
        assert!(check_keys(r#"[{"key": "j"}]"#, &[]).unwrap_err().contains("wants key"));
    }
}
