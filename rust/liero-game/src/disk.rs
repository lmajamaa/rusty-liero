//! What the file layer (`fs.rs`) asks of the disk, from one of two places:
//! `std::fs`, or a tree of folders and files in memory. The web build has no
//! `std::fs` (on `wasm32-unknown-unknown` every call fails), so it will keep
//! its files in the tree, with the TC put in at start and saves kept by the
//! page. Natively the hash run can use the tree too, which shows it gives
//! the same lines as the disk.
//!
//! The tree follows what the disk does on Linux, where the hashes were
//! recorded: names are case-sensitive bytes, any but `/` (a profile's name
//! can have DOS characters such as ö, byte 0x94), a path's `.` parts are skipped
//! and `..` goes up, a folder opens for reading but reads as nothing, and a
//! file can't be made where a folder is or in a folder that isn't there.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};

/// The files of a tree, by path from its root, with `/` between parts;
/// folders are the paths with `None`. The root is the empty path.
#[derive(Default)]
pub struct Tree {
    entries: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    /// The files written, added to or removed since `take_changes`.
    changed: BTreeSet<Vec<u8>>,
}

thread_local! {
    /// This thread's tree, if it uses one instead of `std::fs`.
    static TREE: RefCell<Option<Tree>> = const { RefCell::new(None) };
}

/// Makes this thread's files an empty tree in memory, in place of the
/// disk (or of the tree it had).
pub fn use_tree() {
    TREE.with(|t| *t.borrow_mut() = Some(Tree::default()));
}

/// Back to `std::fs`, dropping the tree.
pub fn use_disk() {
    TREE.with(|t| *t.borrow_mut() = None);
}

/// Whether this thread's files are a tree in memory.
pub fn uses_tree() -> bool {
    TREE.with(|t| t.borrow().is_some())
}

/// `f` on the tree, or `disk` without one.
fn on<R>(tree: impl FnOnce(&mut Tree) -> R, disk: impl FnOnce() -> R) -> R {
    TREE.with(|t| match t.borrow_mut().as_mut() {
        Some(t) => tree(t),
        None => disk(),
    })
}

/// The path as the tree keys it: its parts without `.` and with `..` taking
/// the one before away, joined by `/`. None if it goes above the root.
fn key(p: &[u8]) -> Option<Vec<u8>> {
    let mut parts: Vec<&[u8]> = Vec::new();
    for part in p.split(|&c| c == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join(&b'/'))
}

/// The folder `k` is in, if it has one (the root doesn't).
fn parent(k: &[u8]) -> Option<&[u8]> {
    if k.is_empty() {
        return None;
    }
    Some(k.iter().rposition(|&c| c == b'/').map_or(&[][..], |i| &k[..i]))
}

impl Tree {
    fn get(&self, p: &[u8]) -> Option<&Option<Vec<u8>>> {
        let k = key(p)?;
        if k.is_empty() {
            return Some(&None);
        }
        self.entries.get(&k)
    }

    /// Whether `k` can be made: its folder is there and is a folder.
    fn can_make(&self, k: &[u8]) -> bool {
        match parent(k) {
            Some(b"") => true,
            Some(d) => matches!(self.entries.get(d), Some(None)),
            None => false,
        }
    }

    /// Writes the file `p`, making it if its folder is there. False if it
    /// can't be, or `p` is a folder.
    fn put(&mut self, p: &[u8], data: Vec<u8>) -> bool {
        let Some(k) = key(p) else { return false };
        let ok = match self.entries.get(&k) {
            Some(e) => e.is_some(),
            None => self.can_make(&k),
        };
        if ok {
            self.changed.insert(k.clone());
            self.entries.insert(k, Some(data));
        }
        ok
    }

    /// Every file, by path from the root, in byte order of the paths.
    pub fn files(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        self.entries.iter().filter_map(|(k, v)| Some((k.as_slice(), v.as_deref()?)))
    }
}

/// The files written, added to or removed since the last call, by path
/// from the tree's root, with what each holds now (None if removed), in
/// byte order of the paths; nothing without a tree. The web build saves
/// these.
pub fn take_changes() -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
    TREE.with(|t| {
        let mut t = t.borrow_mut();
        let Some(t) = t.as_mut() else { return Vec::new() };
        let changed = std::mem::take(&mut t.changed);
        changed.into_iter().map(|k| {
            let now = t.entries.get(&k).cloned().flatten();
            (k, now)
        }).collect()
    })
}

/// The tree's files, by path from its root (see `Tree::files`), or nothing
/// without a tree.
pub fn tree_files() -> Vec<(Vec<u8>, Vec<u8>)> {
    TREE.with(|t| t.borrow().as_ref().map_or_else(Vec::new, |t| t.files().map(|(k, v)| (k.to_vec(), v.to_vec())).collect()))
}

/// The path for `std::fs`: its bytes as they are where names are bytes
/// (Linux, macOS); elsewhere only UTF-8 can name a file, so
/// anything else reads as missing.
#[cfg(unix)]
fn path_str(p: &[u8]) -> &std::path::Path {
    use std::os::unix::ffi::OsStrExt;
    std::path::Path::new(std::ffi::OsStr::from_bytes(p))
}

#[cfg(not(unix))]
fn path_str(p: &[u8]) -> &std::path::Path {
    std::path::Path::new(std::str::from_utf8(p).unwrap_or("\u{0}"))
}

/// Whether `p` is a folder, if it's there.
pub fn is_dir(p: &[u8]) -> Option<bool> {
    on(|t| t.get(p).map(Option::is_none), || fs::metadata(path_str(p)).ok().map(|m| m.is_dir()))
}

pub fn exists(p: &[u8]) -> bool {
    is_dir(p).is_some()
}

/// `fopen` and reading it all: a folder opens but reads as nothing.
pub fn open_read(p: &[u8]) -> Option<Vec<u8>> {
    on(
        |t| t.get(p).map(|e| e.clone().unwrap_or_default()),
        || {
            let mut f = fs::File::open(path_str(p)).ok()?;
            let mut data = Vec::new();
            if f.read_to_end(&mut data).is_err() {
                data.clear();
            }
            Some(data)
        },
    )
}

/// The file's bytes; None for a folder or nothing.
pub fn read(p: &[u8]) -> Option<Vec<u8>> {
    on(|t| t.get(p)?.clone(), || fs::read(path_str(p)).ok())
}

/// The names in folder `p`, in no order; None if it isn't a folder.
pub fn list(p: &[u8]) -> Option<Vec<Vec<u8>>> {
    on(
        |t| {
            let k = key(p)?;
            if !k.is_empty() && !matches!(t.entries.get(&k), Some(None)) {
                return None;
            }
            let prefix = if k.is_empty() { Vec::new() } else { [k.as_slice(), b"/"].concat() };
            let names = t
                .entries
                .range(prefix.clone()..)
                .take_while(|(c, _)| c.starts_with(&prefix))
                .map(|(c, _)| &c[prefix.len()..])
                .filter(|rest| !rest.is_empty() && !rest.contains(&b'/'))
                .map(<[u8]>::to_vec)
                .collect();
            Some(names)
        },
        || Some(fs::read_dir(path_str(p)).ok()?.flatten().map(|e| e.file_name().into_encoded_bytes()).collect()),
    )
}

/// Makes folder `p` in a folder that's there. False if it can't, or
/// something is already there.
pub fn create_dir(p: &[u8]) -> bool {
    on(
        |t| match key(p) {
            Some(k) if !t.entries.contains_key(&k) && t.can_make(&k) => {
                t.entries.insert(k, None);
                true
            }
            _ => false,
        },
        || fs::create_dir(path_str(p)).is_ok(),
    )
}

/// Makes file `p` empty, as `fopen(p, "wb")` does. False if it can't be.
pub fn create(p: &[u8]) -> bool {
    write(p, &[])
}

/// Replaces file `p` with `data`, making it if its folder is there.
pub fn write(p: &[u8], data: &[u8]) -> bool {
    on(|t| t.put(p, data.to_vec()), || fs::write(path_str(p), data).is_ok())
}

/// Adds `data` to the end of file `p`, which is there.
pub fn append(p: &[u8], data: &[u8]) -> bool {
    on(
        |t| {
            let Some(k) = key(p) else { return false };
            let Some(Some(f)) = t.entries.get_mut(&k) else { return false };
            f.extend_from_slice(data);
            t.changed.insert(k);
            true
        },
        || fs::OpenOptions::new().append(true).open(path_str(p)).and_then(|mut f| f.write_all(data)).is_ok(),
    )
}

/// Removes file `p`. False if it isn't a file.
pub fn remove_file(p: &[u8]) -> bool {
    on(
        |t| match key(p) {
            Some(k) if matches!(t.entries.get(&k), Some(Some(_))) => {
                t.entries.remove(&k);
                t.changed.insert(k);
                true
            }
            _ => false,
        },
        || fs::remove_file(path_str(p)).is_ok(),
    )
}

#[cfg(test)]
#[path = "disk_tests.rs"]
mod tests;
