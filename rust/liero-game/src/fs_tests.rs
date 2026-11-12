use super::*;
use std::fs;

/// A folder of its own under the crate's `target`, relative as the game's
/// paths are (an absolute path would start at ".").
fn scratch(name: &str) -> String {
    let dir = format!("target/fs-tests/{name}");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Raw deflate streams from Python's zlib (level 6, wbits -15). Not made
/// with miniz_oxide's compressor: a debug build makes its tables on the
/// stack, which overflows the wasm tests' 64 KB.
fn deflated(data: &[u8]) -> Vec<u8> {
    match data {
        b"zip" => vec![171, 202, 44, 0, 0],
        b"data" => vec![75, 73, 44, 73, 4, 0],
        d if d == [b'a'; 48] => vec![75, 76, 36, 13, 0, 0],
        _ => panic!("no deflate stream for {data:?}"),
    }
}

/// A zip with `files` (name, contents, deflated), and `bad_crc` on the
/// first file's checksum.
fn zip(files: &[(&str, &[u8], bool)], bad_crc: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (i, (name, data, deflate)) in files.iter().enumerate() {
        let body = if *deflate { deflated(data) } else { data.to_vec() };
        let crc = crc32(data) ^ (bad_crc && i == 0) as u32;
        let method: u16 = if *deflate { 8 } else { 0 };
        let offset = out.len() as u32;
        let attr: u32 = if name.ends_with('/') { 0x10 } else { 0 };

        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&[20, 0, 0, 0]);
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&body);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&[20, 0, 20, 0, 0, 0]);
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&[0; 4]);
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(body.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 8]);
        central.extend_from_slice(&attr.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let start = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

fn names(n: &Node) -> Vec<(String, bool)> {
    n.list()
        .into_iter()
        .map(|e| (String::from_utf8(e.name).unwrap(), e.is_dir))
        .collect()
}

#[test]
fn paths_start_at_the_current_folder() {
    assert_eq!(Node::new(b"").full_path(), b".");
    assert_eq!(Node::new(b"/").full_path(), b".");
    assert_eq!(Node::new(b"/liero/x").full_path(), b"./liero/x");
    assert_eq!(Node::new(b"a/b/").full_path(), b"./a/b");
    // The empty part joins as "./a/", which takes "b" without another '/'.
    assert_eq!(Node::new(b"a//b").full_path(), b"./a/b");
    assert_eq!(join_path(b"", b"x"), b"x");
    assert_eq!(join_path(b"a\\", b"x"), b"a\\x");
}

#[test]
fn a_zip_reads_as_a_folder_joined_with_one_of_its_name() {
    let dir = scratch("joined");
    fs::create_dir_all(format!("{dir}/TC/Mod/sub")).unwrap();
    fs::write(format!("{dir}/TC/Mod/plain.txt"), b"folder").unwrap();
    fs::write(format!("{dir}/TC/Mod/both.txt"), b"from the folder").unwrap();
    fs::write(
        format!("{dir}/TC/Mod.zip"),
        zip(
            &[
                ("both.txt", b"from the zip", false),
                ("sounds/", b"", false),
                (
                    "sounds/a.wav",
                    b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    true,
                ),
                ("zipped.txt", b"zip", true),
            ],
            false,
        ),
    )
    .unwrap();
    fs::write(format!("{dir}/TC/Other.zip"), zip(&[("x.dat", b"x", false)], false)).unwrap();
    fs::write(format!("{dir}/TC/notes.txt"), b"").unwrap();

    let tc = Node::new(format!("{dir}/TC").as_bytes());
    // Other.zip lists as a folder, without its extension; Mod is listed once.
    assert_eq!(
        names(&tc),
        [
            ("Mod".into(), true),
            ("Other".into(), true),
            ("notes.txt".into(), false)
        ]
    );

    let m = tc.go(b"Mod");
    assert_eq!(m.full_path(), format!("./{dir}/TC/Mod").as_bytes());
    assert_eq!(
        names(&m),
        [
            ("both.txt".into(), false),
            ("plain.txt".into(), false),
            ("sounds".into(), true),
            ("sub".into(), true),
            ("zipped.txt".into(), false)
        ]
    );
    // The folder is read before the zip.
    assert_eq!(m.go(b"both.txt").read().unwrap(), b"from the folder");
    assert_eq!(m.go(b"zipped.txt").read().unwrap(), b"zip");
    assert_eq!(m.go(b"sounds").go(b"a.wav").read().unwrap(), [b'a'; 48]);
    assert!(m.go(b"sounds").go(b"a.wav").exists());
    assert!(!m.go(b"nothing").exists());
    assert!(m.go(b"nothing").read().is_none());

    // Only in the zip: the path is the zip's.
    let x = tc.go(b"Other").go(b"x.dat");
    assert_eq!(x.full_path(), format!("./{dir}/TC/Other/x.dat").as_bytes());
    assert_eq!(x.read().unwrap(), b"x");
    let named = tc.go(b"Other.zip").go(b"x.dat");
    assert_eq!(named.read().unwrap(), b"x");
}

#[test]
fn a_bad_checksum_or_archive_reads_nothing() {
    let dir = scratch("bad");
    fs::write(format!("{dir}/A.zip"), zip(&[("f", b"data", true)], true)).unwrap();
    fs::write(format!("{dir}/B.zip"), b"not a zip").unwrap();
    let root = Node::new(dir.as_bytes());
    assert!(root.go(b"A").go(b"f").read().is_none());
    assert!(names(&root.go(b"B")).is_empty());
}

#[test]
fn files_open_in_other_cases() {
    // The whole path changes case, folders too. (What a case-sensitive
    // disk fails to open can't be tested where the disk isn't.)
    let lower = scratch("lc");
    fs::write(format!("{lower}/names.dat"), b"2").unwrap();
    assert_eq!(Node::new(b"target/fs-tests/LC/NAMES.DAT").read().unwrap(), b"2");
}

#[test]
fn writing_makes_the_folders() {
    let dir = scratch("write");
    let n = Node::new(format!("{dir}/Profiles/deep/BOT.lpf").as_bytes());
    let p = n.prepare_write().unwrap().to_vec();
    assert_eq!(p, format!("./{dir}/Profiles/deep/BOT.lpf").as_bytes());
    assert!(fs::metadata(format!("{dir}/Profiles/deep/BOT.lpf")).is_ok());
}

#[test]
fn a_listing_keeps_the_first_of_a_name() {
    let v = vec![
        Entry {
            name: b"b".to_vec(),
            is_dir: false,
        },
        Entry {
            name: b"a".to_vec(),
            is_dir: true,
        },
        Entry {
            name: b"a".to_vec(),
            is_dir: false,
        },
    ];
    let s = sort_listing(v);
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].name, b"a");
    assert!(s[0].is_dir);
    assert_eq!(s[1].name, b"b");
}
