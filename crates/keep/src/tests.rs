//! Each way of keeping, in a scratch folder: numbers given on, never over anything, never reused;
//! folders appearing only whole; saves replacing whole.

use super::*;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let d = std::env::temp_dir().join(format!("as-keep-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        TempDir(d)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> =
        fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

#[test]
fn create_new_refuses_a_file_there() {
    let d = TempDir::new("create");
    let p = d.0.join("note.txt");
    create_new(&p).unwrap().write_all(b"first").unwrap();
    assert_eq!(create_new(&p).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&p).unwrap(), b"first");
}

#[test]
fn numbered_counts_on_from_the_highest() {
    let d = TempDir::new("numbered");
    let (a, _) = numbered(&d.0, "bed", "wav").unwrap();
    let (b, _) = numbered(&d.0, "bed", "txt").unwrap();
    assert_eq!(a.file_name().unwrap(), "bed-001.wav");
    assert_eq!(b.file_name().unwrap(), "bed-002.txt");
    // A gap is never filled, and other stems don't count.
    fs::remove_file(&a).unwrap();
    fs::write(d.0.join("bedroom-009.wav"), b"").unwrap();
    fs::write(d.0.join("bed-1x.wav"), b"").unwrap();
    let (c, _) = numbered(&d.0, "bed", "wav").unwrap();
    assert_eq!(c.file_name().unwrap(), "bed-003.wav");
    // Past 999, four digits.
    fs::write(d.0.join("bed-999.wav"), b"").unwrap();
    let (e, _) = numbered(&d.0, "bed", "wav").unwrap();
    assert_eq!(e.file_name().unwrap(), "bed-1000.wav");
}

#[test]
fn a_folder_appears_only_when_finished() {
    let d = TempDir::new("folder");
    fs::create_dir(d.0.join("scene-004")).unwrap();
    let f = Folder::new(&d.0, "scene").unwrap();
    fs::write(f.path().join("mix.wav"), b"x").unwrap();
    assert_eq!(names(&d.0), [format!(".scene.incomplete-{}", std::process::id()), "scene-004".into()]);
    let to = f.finish().unwrap();
    assert_eq!(to, d.0.join("scene-005"));
    assert_eq!(names(&d.0), ["scene-004", "scene-005"]);
    assert_eq!(fs::read(to.join("mix.wav")).unwrap(), b"x");
}

#[test]
fn a_folder_dropped_unfinished_is_gone() {
    let d = TempDir::new("dropped");
    {
        let f = Folder::new(&d.0, "scene").unwrap();
        fs::write(f.path().join("mix.wav"), b"x").unwrap();
    }
    assert!(names(&d.0).is_empty());
}

#[test]
fn rename_new_never_replaces() {
    let d = TempDir::new("rename");
    let (a, b) = (d.0.join("a"), d.0.join("b"));
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    // rename(2) alone would put a over an empty b.
    assert_eq!(rename_new(&a, &b).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    fs::write(d.0.join("f"), b"1").unwrap();
    fs::write(d.0.join("g"), b"2").unwrap();
    assert_eq!(rename_new(d.0.join("f"), d.0.join("g")).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(d.0.join("g")).unwrap(), b"2");
    rename_new(d.0.join("f"), d.0.join("h")).unwrap();
    assert_eq!(fs::read(d.0.join("h")).unwrap(), b"1");
}

#[test]
fn save_replaces_whole_and_leaves_nothing_beside() {
    let d = TempDir::new("save");
    let p = d.0.join("scene.yaml");
    save(&p, b"seed: 1\n").unwrap();
    save(&p, b"seed: 4417\n").unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"seed: 4417\n");
    assert_eq!(names(&d.0), ["scene.yaml"]);
}

#[test]
fn a_failed_save_leaves_the_old() {
    let d = TempDir::new("failed");
    let p = d.0.join("scene.yaml");
    save(&p, b"seed: 1\n").unwrap();
    // A folder in the way of the rename: the save fails, the old file stays, the new one's gone.
    let blocked = d.0.join("blocked");
    fs::create_dir_all(blocked.join("in-the-way")).unwrap();
    assert!(save(&blocked, b"x").is_err());
    assert_eq!(fs::read(&p).unwrap(), b"seed: 1\n");
    assert_eq!(names(&d.0), ["blocked", "scene.yaml"]);
}
