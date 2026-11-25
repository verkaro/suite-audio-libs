//! What a layout would write to a Focusrite as it is now, printed and not written: a dry run of
//! `Layout::apply`. Reads the card only.
//!
//! ```sh
//! cargo run -p as-focusrite --example layout -- /dev/snd/by-id/usb-Focusrite_Clarett+_8Pre_SERIAL-00 internal
//! ```

use as_focusrite::layout::{Layout, Rig, Way};

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (Some(path), Some(way)) = (args.next(), args.next()) else {
        return Err("layout CONTROL-DEVICE internal|external".into());
    };
    let way = Way::named(&way).ok_or("internal or external")?;
    let mut card =
        as_focusrite::Card::open(std::path::Path::new(&path)).map_err(|e| format!("{path}: {e}"))?;
    let (name, _) = card.name().map_err(|e| e.to_string())?;
    let rig =
        Rig::named(&name).ok_or(format!("{name}: not a rig laid out (the 18i20 or the Clarett+ 8Pre)"))?;
    let f = as_focusrite::Focusrite::read(&mut card).map_err(|e| e.to_string())?.ok_or("not a Focusrite")?;
    let now = as_focusrite::routes(&mut card, &f).map_err(|e| e.to_string())?;
    let mixing = as_focusrite::Mixing::read(&mut card, &f).map_err(|e| e.to_string())?;
    let writes = Layout::of(rig, way).writes(&f, &now, &mixing).map_err(|e| e.to_string())?;
    println!(
        "{} laid out {}: {} controls would change (nothing written)",
        rig.name(),
        way.word(),
        writes.len()
    );
    for w in writes {
        println!("  {}", w.what);
    }
    Ok(())
}
