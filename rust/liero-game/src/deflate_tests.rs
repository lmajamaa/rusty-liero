use super::*;
use std::fs;

const NO_WASM: &str = "miniz_oxide's debug compressor builds its tables on the 64 KB wasm stack";

/// 200 KB that compress, but not to nothing.
fn sample() -> Vec<u8> {
    let mut x = 1u32;
    (0..200_000)
        .map(|i| {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            if i % 7 == 0 {
                (x >> 24) as u8
            } else {
                (i / 100) as u8
            }
        })
        .collect()
}

/// `data` compressed through a file, written in `chunk`-byte calls.
fn compressed(name: &str, data: &[u8], chunk: usize) -> Vec<u8> {
    let dir = format!("target/deflate-tests/{name}");
    let _ = fs::remove_dir_all(&dir);
    let node = Node::new(format!("{dir}/x.lrp").as_bytes());
    {
        let mut d = Deflater::new(&node).unwrap();
        for c in data.chunks(chunk) {
            d.write(c);
        }
    }
    fs::read(format!("{dir}/x.lrp")).unwrap()
}

#[test]
fn round_trips_whatever_the_writes() {
    let _ = NO_WASM;
    let data = sample();
    let a = compressed("one", &data, data.len());
    let b = compressed("many", &data, 1000);
    // The stream doesn't depend on how the bytes came in.
    assert_eq!(a, b);
    assert_eq!(&a[..2], &[0x78, 0x9c], "a zlib header at the default level");
    assert!(a.len() < data.len());
    assert_eq!(inflate(&a), Inflated { data, error: false });
}

#[test]
fn a_cut_stream_gives_the_whole_pieces_then_fails() {
    let data = sample();
    let z = compressed("cut", &data, data.len());
    let r = inflate(&z[..z.len() * 3 / 4]);
    assert!(r.error);
    assert!(!r.data.is_empty());
    assert_eq!(r.data.len() % PIECE, 0);
    assert_eq!(r.data, data[..r.data.len()]);

    // A wrong checksum fails at the end, with the last piece lost.
    let mut bad = z.clone();
    *bad.last_mut().unwrap() ^= 1;
    let r = inflate(&bad);
    assert!(r.error);
    assert_eq!(r.data, data[..data.len() / PIECE * PIECE]);
}

#[test]
fn what_isnt_zlib_fails_at_once() {
    assert_eq!(
        inflate(b"This is not a replay."),
        Inflated {
            data: Vec::new(),
            error: true
        }
    );
    assert_eq!(
        inflate(b""),
        Inflated {
            data: Vec::new(),
            error: true
        }
    );
    // zlib.compress(b"hi") from Python.
    let hi = [0x78, 0x9c, 0xcb, 0xc8, 0x04, 0x00, 0x01, 0x3b, 0x00, 0xd2];
    assert_eq!(
        inflate(&hi),
        Inflated {
            data: b"hi".to_vec(),
            error: false
        }
    );
}

#[test]
fn nothing_written_leaves_an_empty_file() {
    let z = compressed("empty", b"", 1);
    assert!(z.is_empty());
    assert_eq!(
        inflate(&z),
        Inflated {
            data: Vec::new(),
            error: true
        }
    );
}

/// 300 KB that don't compress, so each call has more than a piece to give.
fn noise() -> Vec<u8> {
    let mut x = 7u32;
    (0..300_000)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

#[test]
fn more_than_a_piece_at_a_time_round_trips() {
    let data = noise();
    let z = compressed("noise", &data, data.len());
    assert!(z.len() > 3 * PIECE);
    assert_eq!(inflate(&z), Inflated { data, error: false });
}

#[test]
fn the_stream_is_miniz_oxide_0_9_1_at_level_6() {
    // Pinned so a level or crate change shows. Replay files differ from
    // what miniz.c 1.14 wrote anyway; the hashes check what they inflate to.
    let z = compressed("pinned", &sample(), 4096);
    assert_eq!((z.len(), crate::fs::tests_crc32(&z)), (52530, 0x86814e91));
}
