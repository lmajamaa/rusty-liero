//! Reading a TC folder and writing it back.
//!
//! The format can't hold everything byte for byte: the bundled TC's .cfg
//! files are stored with LF line breaks (the writer uses CRLF, and the reader
//! takes either), some of its WAV files lack the padding byte after odd-length
//! data and have another value in the RIFF size field (both ignored by the
//! reader), and a font character as wide as the strip, 7 pixels, reads back
//! with width 0, as in the C++ game.

use liero_data::model::Common;
use liero_data::{exe, load, save};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("liero-data-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// Every file under `dir`, by path relative to it.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.insert(rel, fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn bundled_tc() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../TC/openliero")
}

/// Writes `common` out, reads it back and writes it again, returning both
/// sets of files. They may differ only in the font's full-width characters,
/// and writing a third time changes nothing.
fn write_twice(common: &Common, name: &str) -> (BTreeMap<String, Vec<u8>>, BTreeMap<String, Vec<u8>>) {
    let write = |common: &Common, n: usize| {
        let dir = temp_dir(&format!("{name}-{n}"));
        save::save(common, &dir).unwrap();
        let read_back = load::load_dir(&dir).unwrap();
        let written = files(&dir);
        fs::remove_dir_all(&dir).unwrap();
        (written, read_back)
    };
    let (first, again) = write(common, 1);
    let (second, again2) = write(&again, 2);
    let (third, _) = write(&again2, 3);
    assert!(second == third, "writing the read-back TC again changed files");
    for (name, data) in &first {
        if name != "sprites/font.tga" {
            assert!(second.get(name) == Some(data), "{name} changed when read back");
        }
    }
    (first, second)
}

/// The data with the pixels of zero-width font characters cleared: those
/// don't survive writing, since only pixels left of the width are written.
fn without_hidden_font_pixels(mut common: Common) -> Common {
    for ch in &mut common.font {
        for y in 0..8 {
            for x in ch.width..7 {
                ch.data[y * 7 + x] = 0;
            }
        }
    }
    common
}

fn wav_samples(file: &[u8]) -> &[u8] {
    let len = u32::from_le_bytes(file[40..44].try_into().unwrap()) as usize;
    &file[44..44 + len]
}

#[test]
fn the_bundled_tc_reads_and_writes_back() {
    let tc = bundled_tc();
    let common = load::load_dir(&tc).unwrap();
    assert_eq!(common.weapons.len(), 40);
    assert_eq!(common.nobjects.len(), 24);
    assert_eq!(common.sobjects.len(), 14);
    assert_eq!(common.sounds.len(), 30);
    assert!(common.sounds.iter().all(|s| s.data.is_some()));

    // Reading what was written gives the same data.
    let dir = temp_dir("bundled");
    save::save(&common, &dir).unwrap();
    let read_back = load::load_dir(&dir).unwrap();
    fs::remove_dir_all(&dir).unwrap();
    assert!(read_back == without_hidden_font_pixels(load::load_dir(&tc).unwrap()), "the TC read back differently");

    // And the files match the bundled ones, apart from what the format drops.
    let bundled = files(&tc);
    let (written, _) = write_twice(&common, "bundled");
    for (name, data) in &written {
        let original = &bundled[name];
        if name.ends_with(".cfg") {
            let crlf = String::from_utf8_lossy(original).replace('\n', "\r\n");
            assert!(crlf.as_bytes() == data.as_slice(), "{name} differs");
        } else if name.ends_with(".wav") {
            assert!(wav_samples(original) == wav_samples(data), "{name} has other samples");
        } else if name != "sprites/font.tga" {
            assert!(original == data, "{name} differs");
        }
    }
    let unwritten: Vec<_> = bundled.keys().filter(|k| !written.contains_key(*k)).collect();
    assert_eq!(unwritten, ["sounds/sounds.txt"]);
}

#[test]
fn full_width_font_characters_read_back_with_width_0() {
    let common = load::load_dir(&bundled_tc()).unwrap();
    assert_eq!(common.font.len(), 250);
    let full_width: Vec<usize> = (0..250).filter(|&i| common.font[i].width == 0 && common.font[i].data != [0; 56]).collect();
    assert_eq!(full_width, [27]);
    assert_eq!(common.font[b'A' as usize].width, 5);
}

/// Converts an original Liero 1.33 installation, reads the result back and
/// writes it again. Set LIERO_133_DIR to the folder with LIERO.EXE, LIERO.CHR
/// and LIERO.SND to run it; those files aren't in the repository.
#[test]
fn an_original_installation_converts_and_reads_back() {
    let Some(dir) = std::env::var_os("LIERO_133_DIR") else {
        eprintln!("LIERO_133_DIR is not set; skipping");
        return;
    };
    let dir = PathBuf::from(dir);
    let read = |name: &str| fs::read(dir.join(name)).unwrap();
    let common = exe::load(&read("LIERO.EXE"), &read("LIERO.CHR"), &read("LIERO.SND")).unwrap();

    // Apart from the font's full-width characters, nothing is lost.
    let (converted, _) = write_twice(&common, "exe");

    let bundled = files(&bundled_tc());
    let differ: Vec<_> = converted
        .iter()
        .filter(|(k, v)| {
            let b = &bundled[*k];
            if k.ends_with(".cfg") {
                String::from_utf8_lossy(b).replace('\n', "\r\n").as_bytes() != v.as_slice()
            } else if k.ends_with(".wav") {
                wav_samples(b) != wav_samples(v)
            } else {
                b != *v
            }
        })
        .map(|(k, _)| k.clone())
        .collect();
    eprintln!("converted files that differ from the bundled TC: {differ:?}");
}
