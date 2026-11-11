//! A card's control device (`/dev/snd/controlC0`), through the kernel's ioctls
//! (`<sound/asound.h>`): the controls listed, read and written, and their changes heard.
//!
//! The structs are the kernel's for a 64-bit system (x86-64, arm64), their sizes checked by the
//! tests against the header's.

use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;
use std::time::Duration;

use libc::{c_int, c_long};

use crate::controls::{Control, Controls, Kind};

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct ElemId {
    pub numid: u32,
    pub iface: c_int,
    pub device: u32,
    pub subdevice: u32,
    pub name: [u8; 44],
    pub index: u32,
}

#[repr(C)]
pub(crate) struct ElemList {
    pub offset: u32,
    pub space: u32,
    pub used: u32,
    pub count: u32,
    pub pids: *mut ElemId,
    pub reserved: [u8; 50],
}

#[repr(C, align(8))]
pub(crate) struct ElemInfo {
    pub id: ElemId,
    pub kind: c_int,
    pub access: u32,
    pub count: u32,
    pub owner: c_int,
    /// A union: an int's min, max and step (longs); an enum's items (u32), the item asked
    /// for (u32) and its name (64 bytes).
    pub value: [u8; 128],
    pub reserved: [u8; 64],
}

#[repr(C)]
pub(crate) struct ElemValue {
    pub id: ElemId,
    pub indirect: u32,
    pub pad: u32,
    /// A union: longs for bools and ints, u32s for enums.
    pub value: [c_long; 128],
    pub reserved: [u8; 128],
}

#[repr(C)]
pub(crate) struct CardInfo {
    pub card: c_int,
    pub pad: c_int,
    pub id: [u8; 16],
    pub driver: [u8; 16],
    pub name: [u8; 32],
    pub longname: [u8; 80],
    pub reserved: [u8; 16],
    pub mixername: [u8; 80],
    pub components: [u8; 128],
}

#[repr(C)]
pub(crate) struct Event {
    pub kind: c_int,
    pub mask: u32,
    pub id: ElemId,
}

// The ioctls' numbers, from the header (they carry the structs' sizes above).
const CARD_INFO: u64 = 0x8178_5501;
const ELEM_LIST: u64 = 0xc050_5510;
const ELEM_INFO: u64 = 0xc110_5511;
const ELEM_READ: u64 = 0xc4c8_5512;
const ELEM_WRITE: u64 = 0xc4c8_5513;
const SUBSCRIBE_EVENTS: u64 = 0xc004_5516;
const TLV_READ: u64 = 0xc008_551a;

const TYPE_BOOLEAN: c_int = 1;
const TYPE_INTEGER: c_int = 2;
const TYPE_ENUMERATED: c_int = 3;
const ACCESS_READ: u32 = 1 << 0;
const ACCESS_WRITE: u32 = 1 << 1;
const ACCESS_TLV_READ: u32 = 1 << 4;
const EVENT_ELEM: c_int = 0;
const EVENT_MASK_VALUE: u32 = 1 << 0;

const IFACES: [&str; 7] = ["CARD", "HWDEP", "MIXER", "PCM", "RAWMIDI", "TIMER", "SEQUENCER"];

fn zeroed<T>() -> T {
    // SAFETY: only for the plain-data structs above, for which all zeros is a valid value.
    unsafe { std::mem::zeroed() }
}

fn text(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

/// A card's control device, open to read and write.
pub struct Card {
    fd: OwnedFd,
}

impl Card {
    /// Opens a control device: `/dev/snd/controlC0`, or the `/dev/snd/by-id` link to one.
    pub fn open(path: &Path) -> io::Result<Card> {
        let p = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: a valid C string; the fd returned is owned here.
        let fd = unsafe { libc::open(p.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC | libc::O_NONBLOCK) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: just opened, and owned by nothing else.
        Ok(Card { fd: unsafe { OwnedFd::from_raw_fd(fd) } })
    }

    fn ioctl<T>(&self, req: u64, arg: &mut T) -> io::Result<()> {
        // SAFETY: arg is the struct the request takes, of the size its number says.
        let r = unsafe { libc::ioctl(self.fd.as_raw_fd(), req as _, arg as *mut T) };
        if r < 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

    /// The card's short name (`Scarlett 18i20 USB`) and its driver's (`USB-Audio`).
    pub fn name(&self) -> io::Result<(String, String)> {
        let mut i: CardInfo = zeroed();
        self.ioctl(CARD_INFO, &mut i)?;
        Ok((text(&i.name), text(&i.driver)))
    }

    fn info(&self, numid: u32, item: Option<u32>) -> io::Result<ElemInfo> {
        let mut i: ElemInfo = zeroed();
        i.id.numid = numid;
        if let Some(item) = item {
            i.value[4..8].copy_from_slice(&item.to_ne_bytes());
        }
        self.ioctl(ELEM_INFO, &mut i)?;
        Ok(i)
    }

    /// A control's dB range in hundredths, from its TLV, if it has one.
    fn db_range(&self, numid: u32, min: i64, max: i64) -> Option<(i64, i64)> {
        let mut buf = [0u32; 64];
        buf[0] = numid;
        buf[1] = ((buf.len() - 2) * 4) as u32;
        // SAFETY: buf is the snd_ctl_tlv header and the room its length says.
        let r = unsafe { libc::ioctl(self.fd.as_raw_fd(), TLV_READ as _, buf.as_mut_ptr()) };
        if r < 0 {
            return None;
        }
        db_of_tlv(&buf[2..], min, max)
    }

    /// Asks the kernel to say when a value changes ([`Card::changed`]).
    pub fn subscribe(&self) -> io::Result<()> {
        let mut on: c_int = 1;
        self.ioctl(SUBSCRIBE_EVENTS, &mut on)
    }

    /// The controls whose values changed (by numid), waiting up to wait for the first.
    pub fn changed(&self, wait: Duration) -> io::Result<Vec<u32>> {
        let mut p = libc::pollfd { fd: self.fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: one pollfd.
        let r = unsafe { libc::poll(&mut p, 1, wait.as_millis().min(i32::MAX as u128) as c_int) };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut out = Vec::new();
        loop {
            let mut e: Event = zeroed();
            let size = std::mem::size_of::<Event>();
            // SAFETY: reads at most one event into e.
            let n = unsafe { libc::read(self.fd.as_raw_fd(), (&mut e as *mut Event).cast(), size) };
            if n < size as isize {
                break;
            }
            if e.kind == EVENT_ELEM && e.mask != u32::MAX && e.mask & EVENT_MASK_VALUE != 0 {
                out.push(e.id.numid);
            }
        }
        Ok(out)
    }
}

/// The dB range in a TLV (its type, its length in bytes, then its data), for raw values min to
/// max: a scale (type 1), or a min and max (4, and 5 with a mute); inside a container (0).
fn db_of_tlv(tlv: &[u32], min: i64, max: i64) -> Option<(i64, i64)> {
    let (&kind, &len) = (tlv.first()?, tlv.get(1)?);
    let data = tlv.get(2..2 + (len as usize).div_ceil(4))?;
    match kind {
        0 => {
            let mut rest = data;
            while rest.len() >= 2 {
                if let Some(r) = db_of_tlv(rest, min, max) {
                    return Some(r);
                }
                rest = rest.get(2 + (rest[1] as usize).div_ceil(4)..)?;
            }
            None
        }
        1 if data.len() >= 2 => {
            let lo = data[0] as i32 as i64;
            let step = (data[1] & 0xffff) as i64;
            Some((lo, lo + step * (max - min)))
        }
        4 | 5 if data.len() >= 2 => Some((data[0] as i32 as i64, data[1] as i32 as i64)),
        _ => None,
    }
}

impl Controls for Card {
    fn list(&mut self) -> io::Result<Vec<Control>> {
        let mut l: ElemList = zeroed();
        self.ioctl(ELEM_LIST, &mut l)?;
        let mut ids: Vec<ElemId> = vec![zeroed(); l.count as usize];
        l.space = l.count;
        l.pids = ids.as_mut_ptr();
        self.ioctl(ELEM_LIST, &mut l)?;
        ids.truncate(l.used as usize);
        let mut out = Vec::new();
        for id in ids {
            let i = self.info(id.numid, None)?;
            let access = i.access;
            let long = |k: usize| {
                let n = std::mem::size_of::<c_long>();
                let mut b = [0u8; 8];
                b[..n].copy_from_slice(&i.value[k * n..(k + 1) * n]);
                i64::from_ne_bytes(b)
            };
            let kind = match i.kind {
                TYPE_BOOLEAN => Kind::Bool,
                TYPE_INTEGER => {
                    let (min, max, step) = (long(0), long(1), long(2));
                    let db =
                        if access & ACCESS_TLV_READ != 0 { self.db_range(id.numid, min, max) } else { None };
                    Kind::Int { min, max, step, db }
                }
                TYPE_ENUMERATED => {
                    let items = u32::from_ne_bytes(i.value[0..4].try_into().unwrap());
                    let mut names = Vec::new();
                    for k in 0..items {
                        names.push(text(&self.info(id.numid, Some(k))?.value[8..72]));
                    }
                    Kind::Enum(names)
                }
                _ => Kind::Other,
            };
            out.push(Control {
                numid: id.numid,
                iface: IFACES.get(id.iface as usize).map_or("?".into(), |s| s.to_string()),
                index: id.index,
                name: text(&id.name),
                readable: access & ACCESS_READ != 0,
                writable: access & ACCESS_WRITE != 0,
                kind,
                count: i.count,
            });
        }
        Ok(out)
    }

    fn read(&mut self, numid: u32) -> io::Result<Vec<i64>> {
        let i = self.info(numid, None)?;
        let mut v: ElemValue = zeroed();
        v.id.numid = numid;
        self.ioctl(ELEM_READ, &mut v)?;
        let n = (i.count as usize).min(128);
        Ok(match i.kind {
            TYPE_BOOLEAN | TYPE_INTEGER => v.value[..n].to_vec(),
            TYPE_ENUMERATED => {
                // SAFETY: the union read as the u32s an enum's values are.
                let items: &[u32; 256] = unsafe { &*(v.value.as_ptr().cast()) };
                items[..n].iter().map(|&x| x as i64).collect()
            }
            _ => Vec::new(),
        })
    }

    fn write(&mut self, numid: u32, values: &[i64]) -> io::Result<()> {
        let i = self.info(numid, None)?;
        if values.len() != i.count as usize || values.len() > 128 {
            return Err(io::Error::from_raw_os_error(libc::EINVAL));
        }
        let mut v: ElemValue = zeroed();
        v.id.numid = numid;
        match i.kind {
            TYPE_BOOLEAN | TYPE_INTEGER => {
                for (d, &x) in v.value.iter_mut().zip(values) {
                    *d = x as c_long;
                }
            }
            TYPE_ENUMERATED => {
                // SAFETY: the union written as the u32s an enum's values are.
                let items: &mut [u32; 256] = unsafe { &mut *(v.value.as_mut_ptr().cast()) };
                for (d, &x) in items.iter_mut().zip(values) {
                    *d = x as u32;
                }
            }
            _ => return Err(io::Error::from_raw_os_error(libc::EINVAL)),
        }
        self.ioctl(ELEM_WRITE, &mut v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    /// The sizes `<sound/asound.h>` gives on a 64-bit system, which the ioctls' numbers carry.
    #[test]
    fn the_structs_are_the_kernels() {
        assert_eq!(size_of::<ElemId>(), 64);
        assert_eq!(size_of::<ElemList>(), 80);
        assert_eq!(size_of::<ElemInfo>(), 272);
        assert_eq!(size_of::<ElemValue>(), 1224);
        assert_eq!(size_of::<CardInfo>(), 376);
        assert_eq!(size_of::<Event>(), 72);
        assert_eq!(std::mem::offset_of!(ElemInfo, value), 80);
        assert_eq!(std::mem::offset_of!(ElemValue, value), 72);
        let size = |req: u64| (req >> 16) & 0x3fff;
        assert_eq!(size(ELEM_INFO), 272);
        assert_eq!(size(ELEM_READ), 1224);
        assert_eq!(size(ELEM_LIST), 80);
        assert_eq!(size(CARD_INFO), 376);
    }

    #[test]
    fn db_ranges_from_tlvs() {
        // The 18i20's mixer gains: min and max, −80 to +12 dB.
        assert_eq!(db_of_tlv(&[4, 8, (-8000i32) as u32, 1200], 0, 184), Some((-8000, 1200)));
        // A scale: from −127 dB, 1 dB a step, over 0 to 127.
        assert_eq!(db_of_tlv(&[1, 8, (-12700i32) as u32, 100], 0, 127), Some((-12700, 0)));
        // Inside a container, after a TLV that isn't dB.
        assert_eq!(db_of_tlv(&[0, 28, 9, 4, 0, 5, 8, (-600i32) as u32, 0], 0, 10), Some((-600, 0)));
        assert_eq!(db_of_tlv(&[9, 4, 0], 0, 1), None);
    }
}
