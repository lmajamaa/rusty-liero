//! Level files cut short, with and without a palette after the pixels
//! (`hashLevelFiles` in `src/game/hashrun.cpp`): the level each gives, a
//! random one when loading fails.

use super::{Fnv, Out};
use crate::fs::Node;
use crate::level::LieroLevelBox;
use crate::rand::LieroRand;
use crate::settings::LieroSettingsBox;

pub fn hash_level_files(out: &mut Out) {
    let tc = super::tc::bundled();
    // SAFETY: a TC from `load_checked` has its data.
    let d = unsafe { &*crate::tc::liero_tc_data(&tc) };

    let pixels = 504 * 350;
    let level: Vec<u8> = (0..pixels).map(|i| (i * 7 + i / 504) as u8).collect();
    let palette: Vec<u8> = (0..768).map(|i| (i * 13 + 5) as u8).collect();
    let cat = |parts: &[&[u8]]| parts.concat();
    let files: [Vec<u8>; 7] = [
        level.clone(),
        level[..1000].to_vec(),
        cat(&[&level, b"POWERLEVEL", &palette]),
        cat(&[&level, b"POWERLEVEL", &palette[..100]]),
        cat(&[&level, b"POWER"]),
        cat(&[&level, b"NOTAPOWERL", &palette]),
        Vec::new(),
    ];

    for i in 0..=files.len() {
        // The last one isn't written, so it's missing.
        let path = format!("/tmp/level-{i}.lev");
        if let Some(f) = files.get(i) {
            assert!(Node::new(path.as_bytes()).write(f), "Could not write {path}");
        }
        for with_palette in 0..2 {
            let settings = LieroSettingsBox::defaults();
            // SAFETY: nothing else has the new settings.
            let s = unsafe { &mut *settings.as_ptr() };
            s.data.random_level = false;
            s.level_file = path.clone().into_bytes();
            s.data.load_powerlevel_palette = with_palette != 0;

            let mut rand = LieroRand::new(1234 + i as u32);
            let mut lev = LieroLevelBox::new();
            // SAFETY: the TC's tables live as long as it does.
            unsafe { lev.generate_from_settings(&d.level_common(), &d.palette_colors, s, &mut rand) };

            let l = &lev.data().level;
            let mut h = Fnv::default();
            h.i32(l.width);
            h.i32(l.height);
            for &p in lev.pixels() {
                h.u32(u32::from(p));
            }
            for c in &lev.data().origpal {
                for v in [c.r, c.g, c.b] {
                    h.u32(u32::from(v));
                }
            }
            out.hash("level-file", (i * 2 + with_palette) as i64, h);
        }
    }
}
