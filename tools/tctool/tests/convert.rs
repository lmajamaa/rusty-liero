//! Runs the tctool binary and compares the TC it writes with a checked-in
//! listing: one line per file with its SHA-256 and path, as `sha256sum`
//! prints them, so `sha256sum -c` in the TC folder checks a listing too.
//!
//! The original game files can't be in the repository, so CI converts a
//! synthetic installation that this test generates: LIERO.EXE and LIERO.CHR
//! are pseudo-random bytes and LIERO.SND holds 30 sounds of random noise.
//! Random values reach every field over its whole range, which a real game's
//! data doesn't. Set LIERO_133_DIR to also convert an original Liero 1.33.
//!
//! When the output changes on purpose, rewrite the listings with
//! `TCTOOL_UPDATE_LISTINGS=1 cargo test`, and say why in the commit.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Size of the Liero 1.33 executable, which tctool looks for.
const EXE_LEN: usize = 135856;
/// Size of Liero 1.33's LIERO.CHR.
const CHR_LEN: usize = 35028;
/// Where the weapon names are, 40 Pascal strings of 14 bytes each.
const WEAPON_NAMES: usize = 0x1B676;

/// SplitMix64, so the synthetic files are the same on every machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
}

/// Writes LIERO.EXE, LIERO.CHR and LIERO.SND of random data to `dir`.
fn write_synthetic_installation(dir: &Path) {
    let mut rng = Rng(1998);
    fs::create_dir_all(dir).unwrap();

    // Weapons are written to files named after them, so give them distinct
    // names; everything else in the EXE, texts included, stays random.
    let mut exe = rng.bytes(EXE_LEN);
    for i in 0..40 {
        let name = format!("Weapon {i:02}");
        let at = WEAPON_NAMES + i * 14;
        exe[at] = name.len() as u8;
        exe[at + 1..at + 1 + name.len()].copy_from_slice(name.as_bytes());
    }
    fs::write(dir.join("LIERO.EXE"), exe).unwrap();

    fs::write(dir.join("LIERO.CHR"), rng.bytes(CHR_LEN)).unwrap();

    // LIERO.SND: a count, then a name, offset and length for each sound,
    // then the samples. Every seventh sound is empty.
    let count = 30;
    let mut header = (count as u16).to_le_bytes().to_vec();
    let mut samples = Vec::new();
    let data_start = 2 + count * 16;
    for i in 0..count {
        let len = if i % 7 == 3 {
            0
        } else {
            1 + (rng.next() % 3000) as usize
        };
        let mut name = [0u8; 8];
        let text = format!("SOUND{i:02}");
        name[..text.len()].copy_from_slice(text.as_bytes());
        header.extend_from_slice(&name);
        header.extend_from_slice(&((data_start + samples.len()) as u32).to_le_bytes());
        header.extend_from_slice(&(len as u32).to_le_bytes());
        samples.extend(rng.bytes(len));
    }
    header.extend(samples);
    fs::write(dir.join("LIERO.SND"), header).unwrap();
}

/// Runs tctool on `source` and returns the TC folder it wrote.
fn convert(source: &Path, name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("convert-{name}"));
    let tc = root.join("TC").join(name);
    let _ = fs::remove_dir_all(&tc);
    let output = Command::new(env!("CARGO_BIN_EXE_tctool"))
        .arg("--config-root")
        .arg(&root)
        .arg("--tc-name")
        .arg(name)
        .arg(source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "tctool failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    tc
}

/// `sha256sum` lines for every file under `dir`, sorted by path.
fn listing(dir: &Path) -> String {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.push((rel, sha256_hex(&fs::read(&path).unwrap())));
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    files.sort();
    files.iter().map(|(path, hash)| format!("{hash}  {path}\n")).collect()
}

/// Compares `actual` with the listing in tests/`name`, or rewrites the
/// listing when TCTOOL_UPDATE_LISTINGS is set.
fn check_listing(actual: &str, name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(name);
    if std::env::var_os("TCTOOL_UPDATE_LISTINGS").is_some() {
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    if actual == expected {
        return;
    }
    let lines = |s: &str| s.lines().map(str::to_owned).collect::<std::collections::BTreeSet<_>>();
    let (want, got) = (lines(&expected), lines(actual));
    let mut report = String::new();
    for line in want.difference(&got) {
        report += &format!("- {line}\n");
    }
    for line in got.difference(&want) {
        report += &format!("+ {line}\n");
    }
    panic!(
        "the converted TC differs from tests/{name}:\n{report}\
         If the change is intended, run with TCTOOL_UPDATE_LISTINGS=1 to rewrite the listing."
    );
}

#[test]
fn a_synthetic_installation_converts_as_listed() {
    let source = Path::new(env!("CARGO_TARGET_TMPDIR")).join("synthetic-installation");
    write_synthetic_installation(&source);
    let tc = convert(&source, "synthetic");
    check_listing(&listing(&tc), "synthetic.sha256");
}

/// Set LIERO_133_DIR to a folder with Liero 1.33's LIERO.EXE, LIERO.CHR and
/// LIERO.SND to run this; they aren't in the repository.
#[test]
fn liero_133_converts_as_listed() {
    let Some(dir) = std::env::var_os("LIERO_133_DIR") else {
        eprintln!("LIERO_133_DIR is not set; skipping");
        return;
    };
    let tc = convert(Path::new(&dir), "liero133");
    check_listing(&listing(&tc), "liero133.sha256");
}

#[test]
fn sha256_matches_known_values() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let long = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
    assert_eq!(
        sha256_hex(long),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

/// SHA-256 (FIPS 180-4), here so the test needs no crates.
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
        0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
        0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
        0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());

    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes(word.try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}
