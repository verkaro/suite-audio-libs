//! Prints every control of a Focusrite's control device in the text form `Fake` reads
//! (`controls`), with its values now: how `testdata/` is made from a real interface. Reads
//! only; it writes nothing to the card.
//!
//! ```sh
//! cargo run -p as-focusrite --example dump -- /dev/snd/by-id/usb-Focusrite_Clarett+_8Pre_SERIAL-00
//! ```

fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or("dump CONTROL-DEVICE (/dev/snd/controlC0 or its by-id link)")?;
    let mut card =
        as_focusrite::Card::open(std::path::Path::new(&path)).map_err(|e| format!("{path}: {e}"))?;
    let (name, driver) = card.name().map_err(|e| e.to_string())?;
    eprintln!("{name} ({driver})");
    print!("{}", as_focusrite::dump(&mut card).map_err(|e| e.to_string())?);
    Ok(())
}
