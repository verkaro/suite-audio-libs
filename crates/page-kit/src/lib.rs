//! The Dexter family's page kit, as one script each tool's server serves at `/page-kit.js` and
//! its page loads before its own (`page-kit.js`, moved in from place's page, `VENDORED.md`).

/// The script.
pub const JS: &str = include_str!("page-kit.js");

#[cfg(test)]
mod tests {
    #[test]
    fn it_has_what_pages_draw_with() {
        for f in [
            "function kitCanvas",
            "function setSkin",
            "function key(",
            "function ladder(",
            "function readout(",
            "const HITS",
        ] {
            assert!(super::JS.contains(f), "{f}");
        }
    }
}
