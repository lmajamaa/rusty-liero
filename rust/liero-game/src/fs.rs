//! The file layer (`filesystem.cpp`): a node is a path that can be listed,
//! read or written, and a `.zip` file reads as a folder of the same name
//! without the extension, joined with a real folder of that name if there's
//! one. Only the game's own `/` separators are understood, as the web build
//! needed; the Windows branches are gone.
//!
//! Kept from the C++ as the hashes need it: a path starting with `/` begins
//! at `.`, which is `/` in the browser and in the hash run; the listing is
//! sorted by bytes with `std::sort` and then made unique, so of two entries
//! with one name it keeps whichever that sort left first; a file is opened
//! as named, then upper case, lower case and lower case with a capital
//! first letter (all of the path, not just the leaf).

use crate::cpp_sort;
use crate::text::LieroStr;
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::fs;
use std::io::Read;
use std::rc::Rc;

/// `joinPath`: `leaf` under `root`, with a `/` between unless `root` is
/// empty or already ends in one.
pub fn join_path(root: &[u8], leaf: &[u8]) -> Vec<u8> {
    let mut out = root.to_vec();
    if !root.is_empty() && !matches!(root.last(), Some(b'\\' | b'/')) {
        out.push(b'/');
    }
    out.extend_from_slice(leaf);
    out
}

fn path_str(p: &[u8]) -> &str {
    // Paths the game makes are ASCII; anything else can't name a file the
    // browser's file system has, so it reads as missing.
    std::str::from_utf8(p).unwrap_or("\u{0}")
}

fn exists(p: &[u8]) -> bool {
    fs::metadata(path_str(p)).is_ok()
}

fn is_dir(p: &[u8]) -> Option<bool> {
    fs::metadata(path_str(p)).ok().map(|m| m.is_dir())
}

/// `tolerantFOpen` for reading.
fn tolerant_read(p: &[u8]) -> Option<Vec<u8>> {
    let lower = p.to_ascii_lowercase();
    let mut capital = lower.clone();
    if let Some(c) = capital.first_mut() {
        *c = c.to_ascii_uppercase();
    }
    for name in [p.to_vec(), p.to_ascii_uppercase(), lower, capital] {
        if let Ok(mut f) = fs::File::open(path_str(&name)) {
            let mut data = Vec::new();
            // A folder opens but doesn't read, as with `fopen`: no bytes.
            if f.read_to_end(&mut data).is_err() {
                data.clear();
            }
            return Some(data);
        }
    }
    None
}

/// `create_directories`: each folder before a `/` in `p`, past its first
/// character. False if one couldn't be made.
fn create_directories(p: &[u8]) -> bool {
    for i in 1..p.len() {
        if p[i] == b'/' {
            let dir = path_str(&p[..i]);
            if fs::metadata(dir).is_err() && fs::create_dir(dir).is_err() {
                return false;
            }
        }
    }
    true
}

// ---- Zip files

/// An entry of a zip's central directory.
struct ZipEntry {
    /// miniz doesn't read encrypted entries.
    encrypted: bool,
    method: u16,
    crc: u32,
    compressed: usize,
    size: usize,
    local_offset: usize,
}

struct ZipArchive {
    data: Vec<u8>,
    entries: Vec<ZipEntry>,
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xedb8_8320 } else { c >> 1 };
        }
    }
    !c
}

/// A zip folder's tree: what `FsNodeZipFile` built from the archive.
struct ZipNode {
    path: Vec<u8>,
    /// The entry to read, if it's a file in the archive.
    entry: Option<usize>,
    is_dir: bool,
    children: BTreeMap<Vec<u8>, Rc<ZipNode>>,
}

/// While building the tree.
struct ZipBuild {
    path: Vec<u8>,
    entry: Option<usize>,
    is_dir: bool,
    children: BTreeMap<Vec<u8>, ZipBuild>,
}

impl ZipBuild {
    fn child(&mut self, part: &[u8], entry: Option<usize>, is_dir: bool) -> &mut ZipBuild {
        let path = join_path(&self.path, part);
        // The first entry for a name makes it, as `if (!c) c.reset(...)`.
        self.children
            .entry(part.to_vec())
            .or_insert_with(|| ZipBuild { path, entry, is_dir, children: BTreeMap::new() })
    }

    fn finish(self) -> Rc<ZipNode> {
        Rc::new(ZipNode {
            path: self.path,
            entry: self.entry,
            is_dir: self.is_dir,
            children: self.children.into_iter().map(|(k, v)| (k, v.finish())).collect(),
        })
    }
}

impl ZipArchive {
    /// Reads `path`'s central directory, with its entries' names. An archive
    /// that can't be read has none, as miniz's reader left it.
    fn open(path: &[u8]) -> (ZipArchive, Vec<(Vec<u8>, bool)>) {
        let data = fs::read(path_str(path)).unwrap_or_default();
        let mut archive = ZipArchive { data, entries: Vec::new() };
        let names = archive.read_directory().unwrap_or_default();
        if names.len() != archive.entries.len() {
            archive.entries.clear();
        }
        (archive, names)
    }

    fn read_directory(&mut self) -> Option<Vec<(Vec<u8>, bool)>> {
        let d = &self.data;
        // The end of central directory record, in the last 64 KB.
        let min = d.len().saturating_sub(22 + 0xffff);
        let end = (min..=d.len().checked_sub(22)?).rev().find(|&i| u32_at(d, i) == Some(0x0605_4b50))?;
        let count = u16_at(d, end + 10)? as usize;
        let mut at = u32_at(d, end + 16)? as usize;

        let mut names = Vec::with_capacity(count);
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            if u32_at(d, at)? != 0x0201_4b50 {
                return None;
            }
            let name_len = u16_at(d, at + 28)? as usize;
            let extra_len = u16_at(d, at + 30)? as usize;
            let comment_len = u16_at(d, at + 32)? as usize;
            let attr = u32_at(d, at + 38)?;
            let name = d.get(at + 46..at + 46 + name_len)?.to_vec();
            // `mz_zip_reader_is_file_a_directory`: a name ending in '/', or
            // the DOS directory attribute.
            let is_dir = name.last() == Some(&b'/') || attr & 0x10 != 0;
            entries.push(ZipEntry {
                encrypted: u16_at(d, at + 8)? & 1 != 0,
                method: u16_at(d, at + 10)?,
                crc: u32_at(d, at + 16)?,
                compressed: u32_at(d, at + 20)? as usize,
                size: u32_at(d, at + 24)? as usize,
                local_offset: u32_at(d, at + 42)? as usize,
            });
            names.push((name, is_dir));
            at += 46 + name_len + extra_len + comment_len;
        }
        self.entries = entries;
        Some(names)
    }

    /// `mz_zip_reader_extract_to_heap`: stored or deflated, with the CRC
    /// checked. None if it can't be.
    fn extract(&self, i: usize) -> Option<Vec<u8>> {
        let e = self.entries.get(i).filter(|e| !e.encrypted)?;
        let d = &self.data;
        let at = e.local_offset;
        if u32_at(d, at)? != 0x0403_4b50 {
            return None;
        }
        let start = at + 30 + u16_at(d, at + 26)? as usize + u16_at(d, at + 28)? as usize;
        let raw = d.get(start..start.checked_add(e.compressed)?)?;
        let out = match e.method {
            0 => raw.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(raw, e.size).ok()?,
            _ => return None,
        };
        (out.len() == e.size && crc32(&out) == e.crc).then_some(out)
    }
}

/// `FsNodeZipFile`'s constructor: the tree of the archive at `path`.
fn zip_root(path: &[u8]) -> (Rc<ZipArchive>, Rc<ZipNode>) {
    let (archive, names) = ZipArchive::open(path);
    let mut root = ZipBuild { path: path.to_vec(), entry: None, is_dir: true, children: BTreeMap::new() };
    for (index, (name, is_dir)) in names.iter().enumerate() {
        // miniz copied a name into 260 bytes, with its terminator.
        let name = &name[..name.len().min(259)];
        let mut cur = &mut root;
        let mut beg = 0;
        for i in 0..name.len() {
            if name[i] == b'/' {
                cur = cur.child(&name[beg..i], None, true);
                beg = i + 1;
            }
        }
        if beg != name.len() {
            cur.child(&name[beg..], Some(index), *is_dir);
        }
    }
    (Rc::new(archive), root.finish())
}

// ---- Nodes

#[derive(Clone)]
enum Layer {
    Folder(Vec<u8>),
    Zip(Rc<ZipArchive>, Rc<ZipNode>),
}

/// A node: its layers, the first one's path being the node's. No layers is
/// C++'s null node.
#[derive(Clone, Default)]
pub struct Node(Vec<Layer>);

/// A listing's entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: Vec<u8>,
    pub is_dir: bool,
}

/// `DirectoryListing::sort`: `std::sort` by name, then `std::unique`.
fn sort_listing(mut v: Vec<Entry>) -> Vec<Entry> {
    let mut order: Vec<usize> = (0..v.len()).collect();
    cpp_sort::sort(&mut order, &|&a: &usize, &b: &usize| v[a].name < v[b].name);
    let mut out: Vec<Entry> = Vec::with_capacity(v.len());
    for i in order {
        let e = std::mem::replace(&mut v[i], Entry { name: Vec::new(), is_dir: false });
        if out.last().map_or(true, |l| l.name != e.name) {
            out.push(e);
        }
    }
    out
}

impl Layer {
    fn path(&self) -> &[u8] {
        match self {
            Layer::Folder(p) => p,
            Layer::Zip(_, n) => &n.path,
        }
    }

    fn list(&self) -> Vec<Entry> {
        match self {
            Layer::Folder(p) => {
                let dir = if p.is_empty() { b".".as_slice() } else { p };
                let mut out = Vec::new();
                if let Ok(rd) = fs::read_dir(path_str(dir)) {
                    for e in rd.flatten() {
                        let mut name = e.file_name().into_encoded_bytes();
                        // Skipped if it can't be looked at, as `stat` failing.
                        let Some(mut dir) = is_dir(&join_path(p, &name)) else { continue };
                        if name.ends_with(b".zip") {
                            name.truncate(name.len() - 4);
                            dir = true;
                        }
                        out.push(Entry { name, is_dir: dir });
                    }
                }
                sort_listing(out)
            }
            Layer::Zip(_, n) => sort_listing(n.children.iter().map(|(k, c)| Entry { name: k.clone(), is_dir: c.is_dir }).collect()),
        }
    }

    /// The layers of `name` under this one.
    fn go(&self, name: &[u8], out: &mut Vec<Layer>) {
        match self {
            Layer::Folder(p) => {
                let full = join_path(p, name);
                let mut zip = full.clone();
                zip.extend_from_slice(b".zip");
                out.push(Layer::Folder(full.clone()));
                if exists(&zip) {
                    let (a, n) = zip_root(&zip);
                    out.push(Layer::Zip(a, n));
                } else if full.ends_with(b".zip") && exists(&full) {
                    let (a, n) = zip_root(&full);
                    out.push(Layer::Zip(a, n));
                }
            }
            Layer::Zip(a, n) => {
                if let Some(c) = n.children.get(name) {
                    out.push(Layer::Zip(a.clone(), c.clone()));
                }
            }
        }
    }

    fn exists(&self) -> bool {
        match self {
            Layer::Folder(p) => exists(p),
            Layer::Zip(..) => true,
        }
    }

    fn read(&self) -> Option<Vec<u8>> {
        match self {
            Layer::Folder(p) => tolerant_read(p),
            Layer::Zip(a, n) => a.extract(n.entry?),
        }
    }
}

impl Node {
    /// `FsNode(path)`: from `.` through each part of `path`.
    pub fn new(path: &[u8]) -> Node {
        let root = || Node(vec![Layer::Folder(b".".to_vec())]);
        if path.is_empty() {
            return root();
        }
        // Before the first part, or after a part that wasn't there, a part
        // starts again at "." (an empty one, as before a leading '/', is
        // "." itself); after that each part goes one deeper. A trailing '/'
        // adds no part.
        let step = |n: Node, part: &[u8]| match (n.is_null(), part.is_empty()) {
            (true, true) => root(),
            (true, false) => root().go(part),
            (false, _) => n.go(part),
        };
        let mut node = Node::default();
        let mut beg = 0;
        for i in 0..path.len() {
            if path[i] == b'/' {
                node = step(node, &path[beg..i]);
                beg = i + 1;
            }
        }
        if beg != path.len() {
            node = step(node, &path[beg..]);
        }
        node
    }

    pub fn is_null(&self) -> bool {
        self.0.is_empty()
    }

    pub fn full_path(&self) -> &[u8] {
        self.0.first().map_or(&[], |l| l.path())
    }

    /// `iter()`: every layer's entries, sorted and unique.
    pub fn list(&self) -> Vec<Entry> {
        match self.0.len() {
            0 => Vec::new(),
            1 => self.0[0].list(),
            _ => sort_listing(self.0.iter().flat_map(|l| l.list()).collect()),
        }
    }

    pub fn go(&self, name: &[u8]) -> Node {
        let mut out = Vec::new();
        for l in &self.0 {
            l.go(name, &mut out);
        }
        Node(out)
    }

    pub fn exists(&self) -> bool {
        self.0.iter().any(|l| l.exists())
    }

    /// The first layer's bytes that can be read.
    pub fn read(&self) -> Option<Vec<u8>> {
        self.0.iter().find_map(|l| l.read())
    }

    /// The path to write to (`tryToSink`): the first folder layer's, made
    /// ready by creating it, and its folders if that fails. None if it
    /// can't be created.
    pub fn prepare_write(&self) -> Option<&[u8]> {
        let p = self.0.iter().find_map(|l| match l {
            Layer::Folder(p) => Some(p.as_slice()),
            Layer::Zip(..) => None,
        })?;
        let create = || fs::File::create(path_str(p)).is_ok();
        if create() || (create_directories(p) && create()) {
            Some(p)
        } else {
            None
        }
    }
}

impl Node {
    /// Writes the whole file, as `toSink` and a writer did. False if it
    /// can't be created or written.
    pub fn write(&self, data: &[u8]) -> bool {
        match self.prepare_write() {
            Some(p) => fs::write(path_str(p), data).is_ok(),
            None => false,
        }
    }
}

// ---- What C++ calls

/// A file node for C++'s `FsNode` to own.
pub struct LieroFsNode(pub(crate) Node);

fn boxed(n: Node) -> *mut LieroFsNode {
    Box::into_raw(Box::new(LieroFsNode(n)))
}

/// # Safety
/// `path` must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_fs_node_new(path: LieroStr) -> *mut LieroFsNode {
    boxed(Node::new(path.bytes()))
}

#[no_mangle]
pub extern "C" fn liero_fs_node_clone(n: &LieroFsNode) -> *mut LieroFsNode {
    boxed(n.0.clone())
}

/// # Safety
/// `n` must come from a `liero_fs_node_*` function, or be null.
#[no_mangle]
pub unsafe extern "C" fn liero_fs_node_free(n: *mut LieroFsNode) {
    if !n.is_null() {
        drop(Box::from_raw(n));
    }
}

/// `n / name`.
///
/// # Safety
/// `name` must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_fs_node_go(n: &LieroFsNode, name: LieroStr) -> *mut LieroFsNode {
    boxed(n.0.go(name.bytes()))
}

#[no_mangle]
pub extern "C" fn liero_fs_node_is_null(n: &LieroFsNode) -> bool {
    n.0.is_null()
}

/// The node's path, valid while it lives.
#[no_mangle]
pub extern "C" fn liero_fs_node_full_path(n: &LieroFsNode) -> LieroStr {
    let p = n.0.full_path();
    LieroStr { ptr: p.as_ptr(), len: p.len() }
}

#[no_mangle]
pub extern "C" fn liero_fs_node_exists(n: &LieroFsNode) -> bool {
    n.0.exists()
}

/// Calls `add(ctx, name, is_dir)` for each entry, sorted.
///
/// # Safety
/// `add` must take `ctx`.
#[no_mangle]
pub unsafe extern "C" fn liero_fs_node_list(
    n: &LieroFsNode,
    ctx: *mut c_void,
    add: unsafe extern "C" fn(ctx: *mut c_void, name: LieroStr, is_dir: bool),
) {
    for e in n.0.list() {
        add(ctx, LieroStr { ptr: e.name.as_ptr(), len: e.name.len() }, e.is_dir);
    }
}

/// Reads the node: `got(ctx, bytes)` with its contents, which are valid
/// during the call. False, without calling it, if it can't be read.
///
/// # Safety
/// `got` must take `ctx`.
#[no_mangle]
pub unsafe extern "C" fn liero_fs_node_read(
    n: &LieroFsNode,
    ctx: *mut c_void,
    got: unsafe extern "C" fn(ctx: *mut c_void, data: LieroStr),
) -> bool {
    match n.0.read() {
        Some(d) => {
            got(ctx, LieroStr { ptr: d.as_ptr(), len: d.len() });
            true
        }
        None => false,
    }
}

/// Writes `data` as the whole file. False if it can't be.
///
/// # Safety
/// `data` must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_fs_node_write(n: &LieroFsNode, data: LieroStr) -> bool {
    n.0.write(data.bytes())
}

/// Makes the node writable, creating its folders if needed, and gives the
/// path to write it at (valid while the node lives), or an empty one if it
/// can't be.
#[no_mangle]
pub extern "C" fn liero_fs_node_prepare_write(n: &LieroFsNode) -> LieroStr {
    match n.0.prepare_write() {
        Some(p) => LieroStr { ptr: p.as_ptr(), len: p.len() },
        None => LieroStr { ptr: std::ptr::null(), len: 0 },
    }
}

#[cfg(test)]
#[path = "fs_tests.rs"]
mod tests;
