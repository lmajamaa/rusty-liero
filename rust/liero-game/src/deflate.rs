//! Replays' zlib streams (`gvl::deflate_source` over miniz.c), with
//! miniz_oxide: compressing at miniz's default level as a replay is
//! recorded, and inflating a replay file to play it back.
//!
//! gvl inflated into 64 KB pieces and threw at the first error, so what a
//! reader got of a broken stream was the pieces filled before it; reading
//! on then failed. `inflate` keeps to that.

use crate::fs::{LieroFsNode, Node};
use crate::text::LieroStr;
use miniz_oxide::deflate::core::{compress, create_comp_flags_from_zip_params, CompressorOxide, TDEFLFlush, TDEFLStatus};
use miniz_oxide::inflate::decompress_to_vec_zlib;
use std::ffi::c_void;
use std::fs::File;
use std::io::Write;

/// gvl's output pieces.
const PIECE: usize = 65536;

/// miniz's `MZ_DEFAULT_COMPRESSION`.
const LEVEL: i32 = 6;

/// A zlib stream being written to a file.
pub struct Deflater {
    c: Box<CompressorOxide>,
    file: File,
    out: Vec<u8>,
    /// Whether anything was written. gvl's filter never started a stream
    /// it got nothing for, so such a file stays empty.
    started: bool,
}

impl Deflater {
    /// Starts writing `node`, or None if it can't be created.
    pub fn new(node: &Node) -> Option<Deflater> {
        let path = node.prepare_write()?;
        let file = File::create(std::str::from_utf8(path).ok()?).ok()?;
        let flags = create_comp_flags_from_zip_params(LEVEL, 15, 0);
        Some(Deflater { c: Box::new(CompressorOxide::new(flags)), file, out: Vec::new(), started: false })
    }

    fn run(&mut self, mut data: &[u8], flush: TDEFLFlush) {
        let mut buf = vec![0u8; PIECE];
        loop {
            let (status, read, written) = compress(&mut self.c, data, &mut buf, flush);
            data = &data[read..];
            self.out.extend_from_slice(&buf[..written]);
            // Full pieces go to the file as they're made.
            while self.out.len() >= PIECE {
                let rest = self.out.split_off(PIECE);
                let _ = self.file.write_all(&self.out);
                self.out = rest;
            }
            // Until all of it is taken in, or with Finish until the stream
            // is done; an error stops it, as gvl's writer gave up.
            let done = match flush {
                TDEFLFlush::Finish => status == TDEFLStatus::Done,
                _ => data.is_empty() && written < buf.len(),
            };
            if done || (status != TDEFLStatus::Okay && status != TDEFLStatus::Done) {
                break;
            }
        }
    }

    pub fn write(&mut self, data: &[u8]) {
        if !data.is_empty() {
            self.started = true;
            self.run(data, TDEFLFlush::None);
        }
    }
}

impl Drop for Deflater {
    /// Finishes the stream and writes the rest.
    fn drop(&mut self) {
        if !self.started {
            return;
        }
        self.run(&[], TDEFLFlush::Finish);
        let _ = self.file.write_all(&self.out);
    }
}

/// What a zlib stream inflates to: the bytes a reader gets, and whether
/// reading past them fails.
#[derive(Debug, PartialEq, Eq)]
pub struct Inflated {
    pub data: Vec<u8>,
    pub error: bool,
}

/// Inflates `input` as gvl's reader did: in 64 KB pieces, with nothing of
/// the piece an error came up in. (The whole output at once: miniz_oxide's
/// streaming state carries a 32 KB window, which a debug build makes on the
/// 64 KB wasm stack.)
pub fn inflate(input: &[u8]) -> Inflated {
    match decompress_to_vec_zlib(input) {
        Ok(data) => Inflated { data, error: false },
        Err(e) => {
            let mut data = e.output;
            data.truncate(data.len() / PIECE * PIECE);
            Inflated { data, error: true }
        }
    }
}

// ---- What C++ calls

/// A replay file being written.
pub struct LieroDeflateOut(Deflater);

/// Starts writing a compressed file at `node`. Null if it can't be created.
#[no_mangle]
pub extern "C" fn liero_deflate_out_new(node: &LieroFsNode) -> *mut LieroDeflateOut {
    match Deflater::new(&node.0) {
        Some(d) => Box::into_raw(Box::new(LieroDeflateOut(d))),
        None => std::ptr::null_mut(),
    }
}

/// # Safety
/// `data` must be valid.
#[no_mangle]
pub unsafe extern "C" fn liero_deflate_out_write(out: &mut LieroDeflateOut, data: LieroStr) {
    out.0.write(data.bytes());
}

/// Finishes the file.
///
/// # Safety
/// `out` must come from `liero_deflate_out_new`, or be null.
#[no_mangle]
pub unsafe extern "C" fn liero_deflate_out_free(out: *mut LieroDeflateOut) {
    if !out.is_null() {
        drop(Box::from_raw(out));
    }
}

/// Inflates the file at `node`: `got(ctx, data, error)` with what a reader
/// gets of it, and whether reading on fails. False, without calling it, if
/// the file can't be read.
///
/// # Safety
/// `got` must take `ctx`.
#[no_mangle]
pub unsafe extern "C" fn liero_inflate_node(
    node: &LieroFsNode,
    ctx: *mut c_void,
    got: unsafe extern "C" fn(ctx: *mut c_void, data: LieroStr, error: bool),
) -> bool {
    match node.0.read() {
        Some(file) => {
            let r = inflate(&file);
            got(ctx, LieroStr { ptr: r.data.as_ptr(), len: r.data.len() }, r.error);
            true
        }
        None => false,
    }
}

#[cfg(test)]
#[path = "deflate_tests.rs"]
mod tests;
