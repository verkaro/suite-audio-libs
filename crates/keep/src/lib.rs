//! Nothing overwritten, the family's rule, in one place (written for the suite, 2026-10-29, after
//! the way Dexter's export, place's renders and scape's modules each did it):
//!
//! - [`create_new`]: a file created exclusively (`O_EXCL`), never over one.
//! - [`numbered`]: the next of `bed-001.wav`, `bed-002.wav`…, never reusing a number.
//! - [`Folder`]: a numbered folder built hidden and renamed into place only once it's whole
//!   (`renameat2`'s `RENAME_NOREPLACE`), removed if it's dropped unfinished.
//! - [`save`]: a settings file replaced by writing a new one and renaming it over the old, so a
//!   crash leaves the old or the new, never half of either.

use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// Opens `path` for writing, created now; an error if anything's there.
pub fn create_new(path: impl AsRef<Path>) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

/// Creates the next numbered file in `dir`, `stem-NNN.ext` (three digits, more past 999), one
/// past the highest number any `stem-NNN` there has, file or folder, whatever its extension. A
/// number freed by deleting is never given again, so a note naming `bed-004` can't come to mean
/// another render.
pub fn numbered(dir: impl AsRef<Path>, stem: &str, ext: &str) -> io::Result<(PathBuf, File)> {
    let dir = dir.as_ref();
    let mut n = highest(dir, stem)? + 1;
    loop {
        let path = dir.join(format!("{}.{ext}", name(stem, n)));
        match create_new(&path) {
            Ok(f) => return Ok((path, f)),
            // Another process took it between the look and the create: the next.
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(e),
        }
    }
}

/// A numbered folder being made: hidden in its parent as `.stem.incomplete-PID/` until
/// [`Folder::finish`] renames it to the next `stem-NNN/`. Dropped unfinished, it's removed with
/// what's in it.
#[derive(Debug)]
pub struct Folder {
    parent: PathBuf,
    stem: String,
    work: PathBuf,
    done: bool,
}

impl Folder {
    /// Starts one in `parent` (which must exist). A hidden folder of this name left by an earlier
    /// run of this process's id (a crash, the id reused) is in the way, and an error.
    pub fn new(parent: impl AsRef<Path>, stem: &str) -> io::Result<Folder> {
        let parent = parent.as_ref().to_path_buf();
        let work = parent.join(format!(".{stem}.incomplete-{}", std::process::id()));
        fs::create_dir(&work)?;
        Ok(Folder { parent, stem: stem.to_string(), work, done: false })
    }

    /// Where its files go while it's made.
    pub fn path(&self) -> &Path {
        &self.work
    }

    /// Renames it into place as the next `stem-NNN/`, and says where.
    pub fn finish(mut self) -> io::Result<PathBuf> {
        let mut n = highest(&self.parent, &self.stem)? + 1;
        loop {
            let to = self.parent.join(name(&self.stem, n));
            match rename_new(&self.work, &to) {
                Ok(()) => {
                    self.done = true;
                    sync_dir(&self.parent)?;
                    return Ok(to);
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => n += 1,
                Err(e) => return Err(e),
            }
        }
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        if !self.done {
            let _ = fs::remove_dir_all(&self.work);
        }
    }
}

/// Replaces `path` with `bytes`: written to a hidden file beside it, flushed to the disk, renamed
/// over it, and the folder flushed, so a crash leaves the old file or the new one whole.
pub fn save(path: impl AsRef<Path>, bytes: &[u8]) -> io::Result<()> {
    let path = path.as_ref();
    let file =
        path.file_name().ok_or_else(|| io::Error::other(format!("{}: no file name", path.display())))?;
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let tmp = parent.join(format!(".{}.new-{}", file.to_string_lossy(), std::process::id()));
    let written = (|| {
        // The hidden file is this process's own: one a crash left behind is written over.
        let mut f = OpenOptions::new().write(true).create(true).truncate(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written?;
    sync_dir(parent)
}

/// rename(2) that fails with `AlreadyExists` rather than replace anything at `to`, a file or a
/// folder, even an empty one.
pub fn rename_new(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let c = |p: &Path| CString::new(p.as_os_str().as_bytes()).map_err(io::Error::other);
    let (f, t) = (c(from.as_ref())?, c(to.as_ref())?);
    // SAFETY: two NUL-terminated paths that outlive the call; renameat2 only reads them.
    let r = unsafe {
        libc::renameat2(libc::AT_FDCWD, f.as_ptr(), libc::AT_FDCWD, t.as_ptr(), libc::RENAME_NOREPLACE)
    };
    if r == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

/// `stem-NNN`.
fn name(stem: &str, n: u64) -> String {
    format!("{stem}-{n:03}")
}

/// The highest number of any `stem-NNN` in `dir`, with or without an extension, or 0.
fn highest(dir: &Path, stem: &str) -> io::Result<u64> {
    let mut most = 0;
    for entry in fs::read_dir(dir)? {
        let file = entry?.file_name();
        let Some(rest) = file.to_str().and_then(|f| f.strip_prefix(stem)).and_then(|r| r.strip_prefix('-'))
        else {
            continue;
        };
        let digits = rest.split('.').next().unwrap_or("");
        if digits.len() >= 3 && digits.bytes().all(|b| b.is_ascii_digit()) {
            most = most.max(digits.parse().unwrap_or(0));
        }
    }
    Ok(most)
}

/// Flushes a folder's entries (a rename in it) to the disk.
fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}
