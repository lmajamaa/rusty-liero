//! The TC as loaded (`hashTc`) and F8's random TC (`hashRandomTc`), from
//! `src/game/hashtc.cpp`. The TOML parts are hashed as the text gvl's writer
//! wrote, which `liero_data::save` writes byte for byte.

use super::{Fnv, Out};
use crate::fs::Node;
use crate::tc::{load_checked, LieroTc, LieroTcData, LieroTcSprites, LIERO_TC_FONT_CHARS};
use liero_data::save;

/// The TC the hash run uses.
pub(super) fn bundled() -> crate::shared::Shared<LieroTc> {
    load_checked(&Node::new(b"TC/openliero")).expect("the hash run's TC")
}

/// # Safety
/// `p` is valid for `n` items, or `n` is 0.
unsafe fn items<'a, T>(p: *const T, n: usize) -> &'a [T] {
    unsafe {
        if n == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(p, n)
        }
    }
}

/// A sprite set as `addSprites` hashes it.
fn add_sprites(h: &mut Fnv, s: &LieroTcSprites) {
    let size = s.width * s.height;
    h.i32(s.width);
    h.i32(s.height);
    h.i32(size);
    h.i32(s.count);
    let bytes = size as usize * s.count as usize;
    h.u32(bytes as u32);
    h.bytes(s.pixels());
}

pub fn hash_tc(out: &mut Out) {
    let tc = bundled();
    let m = tc.model().expect("the TC loaded");
    // SAFETY: a TC from `load_checked` has its data.
    let d: &LieroTcData = unsafe { &*crate::tc::liero_tc_data(&tc) };

    out.hash("tc-cfg", 0, Fnv::of(&save::tc_text(m)));

    for (i, w) in m.weapons.iter().enumerate() {
        let mut h = Fnv::of(&save::weapon_text(m, w));
        h.i32(i as i32);
        h.bytes(w.id_str.as_bytes());
        out.hash("tc-weapon", i as i64, h);
    }
    for (i, n) in m.nobjects.iter().enumerate() {
        let mut h = Fnv::of(&save::nobject_text(m, n));
        h.i32(i as i32);
        h.bytes(n.id_str.as_bytes());
        out.hash("tc-nobject", i as i64, h);
    }
    for (i, s) in m.sobjects.iter().enumerate() {
        let mut h = Fnv::of(&save::sobject_text(s));
        h.i32(i as i32);
        h.bytes(s.id_str.as_bytes());
        out.hash("tc-sobject", i as i64, h);
    }

    let sounds = &d.sounds;
    for (i, s) in sounds.iter().enumerate() {
        let mut h = Fnv::default();
        // SAFETY: the sound's name and samples are the TC's.
        unsafe {
            h.bytes(s.name.bytes());
            h.u32(s.len as u32);
            h.bytes(items(s.data, s.len));
        }
        h.bool(s.valid);
        out.hash("tc-sound", i as i64, h);
    }

    let mut h = Fnv::default();
    add_sprites(&mut h, &d.small_sprites);
    add_sprites(&mut h, &d.large_sprites);
    add_sprites(&mut h, &d.text_sprites);
    for c in &d.palette_colors {
        for v in [c.r, c.g, c.b, c.unused] {
            h.u32(u32::from(v));
        }
    }
    out.hash("tc-sprites", 0, h);

    let mut h = Fnv::default();
    h.u32(LIERO_TC_FONT_CHARS as u32);
    for c in &d.font {
        h.bytes(&c.data);
        h.i32(c.width);
    }
    out.hash("tc-font", 0, h);

    let mut h = Fnv::default();
    h.u32(d.weapons.len() as u32);
    for &i in &d.weap_order {
        h.i32(i);
    }
    add_sprites(&mut h, &d.worm_sprites);
    add_sprites(&mut h, &d.fire_cone_sprites);
    out.hash("tc-computed", 0, h);
}

/// F8's random TC, on a TC of its own. Only the weapons and special objects:
/// the order the other objects get their values in was shuffled with
/// `std::random_device` in the C++.
pub fn hash_random_tc(out: &mut Out) {
    let tc = bundled();
    // SAFETY: nothing else has this TC.
    let t = unsafe { &mut *tc.as_ptr() };
    // Any seed: only the shuffle, which isn't hashed, uses it.
    t.randomize(0);
    let m = t.model().expect("the TC loaded");
    for i in 0..m.weapons.len() {
        out.hash(
            "tc-random-weapon",
            i as i64,
            Fnv::of(&save::weapon_text(m, &t.weapon_model(i))),
        );
    }
    for i in 0..m.sobjects.len() {
        out.hash(
            "tc-random-sobject",
            i as i64,
            Fnv::of(&save::sobject_text(&t.sobject_model(i))),
        );
    }
}
