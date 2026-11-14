//! The TC as loaded (`hashTc`) and F8's random TC (`hashRandomTc`), from
//! `src/game/hashtc.cpp`. The TOML parts are hashed as the text gvl's writer
//! wrote, which `liero_data::save` writes byte for byte.

use super::{Fnv, Out};
use crate::fs::Node;
use crate::tc::{load_checked, LieroTcData, SharedTc};
use liero_data::model::SpriteSet;
use liero_data::save;

/// The TC the hash run uses.
pub(super) fn bundled() -> SharedTc {
    load_checked(&Node::new(b"TC/openliero")).expect("the hash run's TC")
}

/// A sprite set as `addSprites` hashes it.
fn add_sprites(h: &mut Fnv, s: &SpriteSet) {
    let size = s.width * s.height;
    h.i32(s.width as i32);
    h.i32(s.height as i32);
    h.i32(size as i32);
    h.i32(s.count as i32);
    h.u32((size * s.count) as u32);
    h.bytes(&s.data);
}

pub fn hash_tc(out: &mut Out) {
    let tc = bundled();
    let t = tc.borrow();
    let m = t.model().expect("the TC loaded");
    let d: &LieroTcData = t.data().expect("the TC loaded");

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

    for (i, s) in m.sounds.iter().enumerate() {
        let data = s.data.as_deref().unwrap_or(&[]);
        let mut h = Fnv::default();
        h.bytes(s.name.as_bytes());
        h.u32(data.len() as u32);
        h.bytes(data);
        h.bool(s.data.is_some());
        out.hash("tc-sound", i as i64, h);
    }

    let mut h = Fnv::default();
    add_sprites(&mut h, &m.small_sprites);
    add_sprites(&mut h, &m.large_sprites);
    add_sprites(&mut h, &m.text_sprites);
    for c in &d.palette_colors {
        for v in [c.r, c.g, c.b, c.unused] {
            h.u32(u32::from(v));
        }
    }
    out.hash("tc-sprites", 0, h);

    let mut h = Fnv::default();
    h.u32(m.font.len() as u32);
    for c in &m.font {
        h.bytes(&c.data);
        h.i32(c.width as i32);
    }
    out.hash("tc-font", 0, h);

    let mut h = Fnv::default();
    h.u32(d.model.weapons.len() as u32);
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
    let mut t = tc.borrow_mut();
    // Any seed: only the shuffle, which isn't hashed, uses it.
    t.randomize(0);
    let m = t.model().expect("the TC loaded");
    for (i, w) in m.weapons.iter().enumerate() {
        out.hash("tc-random-weapon", i as i64, Fnv::of(&save::weapon_text(m, w)));
    }
    for (i, s) in m.sobjects.iter().enumerate() {
        out.hash("tc-random-sobject", i as i64, Fnv::of(&save::sobject_text(s)));
    }
}
