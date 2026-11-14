//! Loading a TC for the C++ game: `Common::load` and `Common::precompute`.
//!
//! The reading itself is `liero_data::load`, from a file node (`fs.rs`),
//! since a TC can be a folder, a zip file or both joined. C++ copies the
//! result into `Common` from the `TcData` here, which owns everything
//! it holds.

use crate::fs::Node;
use crate::gfx::Color;
use crate::rand::Rand;
use crate::sfx::samples_from_tc;
use liero_data::model::SpriteSet;
use liero_data::model::{self, Common};
use liero_data::Error;
use std::cell::{Ref, RefCell};
use std::rc::Rc;

/// A loaded TC: the model as `liero_data` reads it, and what the game
/// works out from it.
pub struct TcData {
    /// The TC as loaded, with F8's changes.
    pub model: Common,
    /// Each of the model's sounds as the 16-bit samples the mixer plays,
    /// twice as many as the model's (`SfxSample::createSound`), and none
    /// for a sound the game can't play. The TC's sounds are never changed
    /// after loading, so they stay put while a sound plays.
    pub samples: Vec<Vec<i16>>,
    /// The model's palette as the game's `Palette` holds it
    /// (`Common::exepal`).
    pub palette_colors: [Color; 256],
    pub materials: [crate::material::Material; 256],
    /// Precomputed: weapon indexes sorted by name, and the worm and fire cone
    /// sprites facing both ways (`Common::precompute`).
    pub weap_order: Vec<i32>,
    pub worm_sprites: SpriteSet,
    pub fire_cone_sprites: SpriteSet,
}

/// A loaded TC, or the error that stopped loading it.
pub struct Tc {
    error: String,
    data: Option<Box<TcData>>,
}

/// Weapon indexes sorted by name, as bytes. (The C++ used `std::sort`, which
/// isn't stable; names in a TC are distinct, so the order is the same.)
pub fn weapon_order(weapons: &[model::Weapon]) -> Vec<i32> {
    let mut order: Vec<i32> = (0..weapons.len() as i32).collect();
    order.sort_by(|&a, &b| weapons[a as usize].name.cmp(&weapons[b as usize].name));
    order
}

/// The 21 worm frames (large sprites 16..37) facing right and mirrored to
/// face left, for both worms; the second worm's colours 30..34 become 39..43.
/// Sprite `frame + dir * 21 + worm * 42`, 16x16 each. A mirrored row is
/// shifted by one pixel and its last pixel cleared, as the original did.
pub fn worm_sprites(large: &model::SpriteSet) -> Vec<u8> {
    let mut out = vec![0u8; 16 * 16 * 2 * 2 * 21];
    let sprite = |f: usize, dir: usize, w: usize| (f + dir * 7 * 3 + w * 2 * 7 * 3) * 256;
    for i in 0..21 {
        for y in 0..16 {
            for x in 0..16 {
                let mut pix = large.data[(16 + i) * 256 + y * 16 + x];
                out[sprite(i, 1, 0) + y * 16 + x] = pix;
                if x == 15 {
                    out[sprite(i, 0, 0) + y * 16 + 15] = 0;
                } else {
                    out[sprite(i, 0, 0) + y * 16 + 14 - x] = pix;
                }
                if (30..=34).contains(&pix) {
                    pix += 9;
                }
                out[sprite(i, 1, 1) + y * 16 + x] = pix;
                if x == 15 {
                    out[sprite(i, 0, 1) + y * 16 + 15] = 0;
                } else {
                    out[sprite(i, 0, 1) + y * 16 + 14 - x] = pix;
                }
            }
        }
    }
    out
}

/// The 7 fire cone frames (large sprites 9..16) facing right and mirrored:
/// sprite `frame + dir * 7`.
pub fn fire_cone_sprites(large: &model::SpriteSet) -> Vec<u8> {
    let mut out = vec![0u8; 16 * 16 * 2 * 7];
    let sprite = |f: usize, dir: usize| (f + dir * 7) * 256;
    for i in 0..7 {
        for y in 0..16 {
            for x in 0..16 {
                let pix = large.data[(9 + i) * 256 + y * 16 + x];
                out[sprite(i, 1) + y * 16 + x] = pix;
                if x == 15 {
                    out[sprite(i, 0) + y * 16 + 15] = 0;
                } else {
                    out[sprite(i, 0) + y * 16 + 14 - x] = pix;
                }
            }
        }
    }
    out
}

impl Tc {
    fn failed(error: String) -> Self {
        Tc { error, data: None }
    }

    fn new(common: Common) -> Result<Self, Error> {
        if common.large_sprites.count < 37 {
            return Err(Error("the TC has too few large sprites".into()));
        }
        let mut tc = Tc::failed(String::new());
        let c = &common;
        let samples = c
            .sounds
            .iter()
            .map(|s| {
                let data = s.data.as_deref().unwrap_or(&[]);
                let mut samples = vec![0i16; data.len() * 2];
                samples_from_tc(data, &mut samples);
                samples
            })
            .collect();

        let data = TcData {
            samples,
            palette_colors: c.palette.map(|[r, g, b]| Color { r, g, b, unused: 0 }),
            materials: c.materials.map(|flags| crate::material::Material { flags }),
            weap_order: weapon_order(&c.weapons),
            worm_sprites: SpriteSet {
                data: worm_sprites(&c.large_sprites),
                width: 16,
                height: 16,
                count: 2 * 2 * 21,
            },
            fire_cone_sprites: SpriteSet {
                data: fire_cone_sprites(&c.large_sprites),
                width: 16,
                height: 16,
                count: 2 * 7,
            },
            // Last: the fields above borrow it.
            model: common,
        };
        tc.data = Some(Box::new(data));
        Ok(tc)
    }
}

impl TcData {
    /// A TC with no tables and every other field zero, for tests.
    #[cfg(test)]
    pub(crate) fn empty() -> Box<Self> {
        let mut tc = crate::objects::zeroed_box::<Self>();
        // SAFETY: all zeros is a valid value of every field but the tables
        // and sprites, which are written before the box is used.
        unsafe {
            let p = tc.as_mut_ptr();
            std::ptr::addr_of_mut!((*p).model).write(empty_model());
            std::ptr::addr_of_mut!((*p).samples).write(Vec::new());
            std::ptr::addr_of_mut!((*p).weap_order).write(Vec::new());
            std::ptr::addr_of_mut!((*p).worm_sprites).write(Default::default());
            std::ptr::addr_of_mut!((*p).fire_cone_sprites).write(Default::default());
            tc.assume_init()
        }
    }

    /// What levels are made with (`Common::levelCommon`).
    pub(crate) fn level_common(&self) -> crate::level::Common<'_> {
        crate::level::Common {
            materials: &self.materials,
            textures: &self.model.textures,
            large_sprites: &self.model.large_sprites.data,
        }
    }
}

impl Tc {
    /// F8 on the menu: the developers' random TC (`Gfx::randomizeTc`). The
    /// values come from a generator seeded with 14, as the C++ did. The
    /// order the object types get theirs in is shuffled from `shuffle_seed`;
    /// the C++ shuffled it with `std::random_device`, so it never repeated.
    pub fn randomize(&mut self, shuffle_seed: u32) {
        let Some(d) = &mut self.data else {
            return;
        };
        let mut r = Rand::new(14);
        let sobjects = d.model.sobjects.len() as u32;
        let nobjects = d.model.nobjects.len() as u32;
        let sounds = d.model.sounds.len() as u32;
        let small = d.model.small_sprites.count as u32;
        let large = d.model.large_sprites.count as u32;

        let mut order: Vec<usize> = (0..d.model.nobjects.len()).collect();
        let mut shuffle = Rand::new(shuffle_seed);
        for i in (1..order.len()).rev() {
            order.swap(i, shuffle.below(i as u32 + 1) as usize);
        }

        // `r(n)` as the C++ wrote it: unsigned, then stored in an int.
        let mut r = |n: u32| r.below(n) as i32;
        for w in &mut d.model.weapons {
            w.add_speed = r(30) - 5;
            w.affect_by_explosions = r(2) == 0;
            w.affect_by_worm = r(3) == 0;
            w.ammo = r(20) + 1;
            w.blood_on_hit = r(50);
            w.blow_away = r(10);
            w.bounce = r(90);
            w.collide_with_objects = r(10) == 0;
            w.color_bullets = 3 + r(250);
            w.create_on_exp = r(sobjects);
            w.delay = r(70);
            w.detect_distance = r(20);
            w.dirt_effect = r(9);
            w.distribution = r(5000) - 2500;
            w.expl_ground = r(2) == 0;
            w.explo_sound = r(sounds);
            w.fire_cone = r(10);
            w.gravity = r(2000) - 1000;
            w.hit_damage = r(20);
            w.laser_sight = r(5) == 0;
            w.launch_sound = r(sounds);
            w.leave_shell_delay = r(30);
            w.leave_shells = i32::from(r(1) == 0);
            w.loading_time = r(70 * 3);
            w.loop_anim = r(10) == 0;
            w.loop_sound = 0;
            w.mult_speed = if r(2) != 0 { 100 } else { 99 + r(5) };
            w.obj_trail_delay = 10 + r(70);
            w.obj_trail_type = if r(4) == 0 { r(sobjects) } else { -1 };
            w.parts = if r(2) == 0 { r(10) } else { 1 };
            w.part_trail_delay = 10 + r(70);
            w.part_trail_obj = if r(4) == 0 { r(nobjects) } else { -1 };
            w.part_trail_type = r(2);
            w.play_reload_sound = r(2) == 0;
            w.recoil = r(20);
            w.shadow = r(2) == 0;
            w.shot_type = r(5);
            w.speed = r(200);
            w.splinter_amount = if r(5) == 0 { r(10) } else { 0 };
            w.splinter_colour = r(256);
            w.splinter_scatter = r(2);
            w.splinter_type = r(nobjects);
            w.start_frame = r(small.wrapping_sub(13));
            w.num_frames = r(5);
            w.time_to_explo = 50 + r(200);
            w.time_to_explo_v = 10 + r(50);
            w.worm_collide = r(3) > 0;
            w.worm_explode = r(3) > 0;
        }

        for (idx, &i) in order.iter().enumerate() {
            let n = &mut d.model.nobjects[i];
            n.affect_by_explosions = r(5) == 0;
            n.blood_on_hit = r(5);
            n.blood_trail = r(10) == 0;
            n.blood_trail_delay = r(20) + 3;
            n.blow_away = r(10);
            n.bounce = r(90);
            n.color_bullets = 3 + r(250);
            n.create_on_exp = if r(3) == 0 { r(sobjects) } else { -1 };
            n.detect_distance = r(20);
            n.dirt_effect = r(9);
            n.distribution = r(5000) - 2500;
            n.draw_on_map = r(20) == 0;
            n.expl_ground = r(4) > 0;
            n.gravity = r(2000) - 1000;
            n.hit_damage = r(10);
            n.leave_obj = if r(5) == 0 { r(sobjects) } else { -1 };
            n.leave_obj_delay = 10 + r(80);
            n.start_frame = r(small.wrapping_sub(13));
            n.num_frames = r(5);
            n.speed = r(150);
            n.splinter_amount = if idx > 0 && r(5) == 0 { r(10) } else { 0 };
            n.splinter_colour = r(256);
            n.splinter_type = if idx > 0 {
                order[r(idx as u32) as usize] as i32
            } else {
                0
            };
            n.time_to_explo = 50 + r(70 * 3);
            n.time_to_explo_v = r(30);
            n.worm_destroy = r(3) == 0;
            n.worm_explode = r(2) == 0;
        }

        for s in &mut d.model.sobjects {
            s.anim_delay = 1 + r(10);
            s.blow_away = if r(2) == 0 { r(10000) } else { 0 };
            s.damage = r(30);
            s.detect_range = r(20);
            s.dirt_effect = r(9);
            s.flash = r(5);
            s.start_frame = r(large.wrapping_sub(7));
            s.num_frames = r(7);
            s.start_sound = r(sounds);
            s.shake = r(10);
            s.shadow = r(2) != 0;
            s.num_sounds = 1;
        }
    }
}

/// Loads the TC in `node`, each file at its path inside it (`/` between
/// folders).
fn load(node: &Node) -> Tc {
    let mut read = |path: &str| {
        let file = path
            .as_bytes()
            .split(|&c| c == b'/')
            .fold(node.clone(), |n, part| n.go(part));
        file.read().ok_or_else(|| Error(format!("Could not read {path}")))
    };
    match liero_data::load::load(&mut read).and_then(Tc::new) {
        Ok(tc) => tc,
        Err(e) => Tc::failed(e.0),
    }
}

/// A TC as the app, the controllers and the games hold it (C++'s
/// `shared_ptr<Common>`). Only F8 changes it after loading.
pub type SharedTc = Rc<RefCell<Tc>>;

/// Loads the TC in `node` (`Common::load`). Always returns a TC; its
/// `error` says whether loading failed.
pub fn load_shared(node: &Node) -> SharedTc {
    Rc::new(RefCell::new(load(node)))
}

/// The data of a TC that loaded. Panics on one that didn't.
pub fn tc_data(tc: &RefCell<Tc>) -> Ref<'_, TcData> {
    Ref::map(tc.borrow(), |t| t.data().expect("the TC didn't load"))
}

impl Tc {
    /// The loaded data, or none if loading failed.
    pub fn data(&self) -> Option<&TcData> {
        self.data.as_deref()
    }

    /// Why loading failed, or empty.
    pub fn error(&self) -> &str {
        &self.error
    }

    /// A TC of `data` alone, for tests.
    #[cfg(test)]
    pub(crate) fn of(data: Box<TcData>) -> SharedTc {
        Rc::new(RefCell::new(Tc {
            error: String::new(),
            data: Some(data),
        }))
    }
}

impl Tc {
    /// The TC as loaded, with F8's changes.
    pub(crate) fn model(&self) -> Option<&Common> {
        self.data.as_ref().map(|d| &d.model)
    }
}

/// Loads the TC in `node` as `Common::load` did, or the message it threw:
/// the TC can't be read, or its constants, texts and hacks aren't the
/// game's.
pub fn load_checked(node: &Node) -> Result<SharedTc, String> {
    let tc = load(node);
    let Some(d) = tc.data.as_deref() else {
        return Err(format!("Could not load the TC: {}", tc.error));
    };
    let m = &d.model;
    if m.constants.len() != liero_data::exe::CONSTANT_ORDER.len()
        || m.texts.len() != liero_data::exe::TEXTS.len()
        || m.hacks.len() != liero_data::exe::HACKS.len()
    {
        return Err("The TC's constants, texts and hacks don't match the game's".into());
    }
    Ok(Rc::new(RefCell::new(tc)))
}

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// The index of TC constant `name` (C++'s `C` enum).
pub(crate) const fn c(name: &str) -> usize {
    let order = liero_data::exe::CONSTANT_ORDER;
    let mut i = 0;
    while i < order.len() {
        if str_eq(order[i], name) {
            return i;
        }
        i += 1;
    }
    panic!("no such constant");
}

/// The index of TC hack `name` (C++'s `H` enum).
pub(crate) const fn h(name: &str) -> usize {
    let hacks = liero_data::exe::HACKS;
    let mut i = 0;
    while i < hacks.len() {
        if str_eq(hacks[i].0, name) {
            return i;
        }
        i += 1;
    }
    panic!("no such hack");
}

/// The index of TC text `name` (C++'s `S` enum).
pub(crate) const fn s(name: &str) -> usize {
    let texts = liero_data::exe::TEXTS;
    let mut i = 0;
    while i < texts.len() {
        if str_eq(texts[i].0, name) {
            return i;
        }
        i += 1;
    }
    panic!("no such text");
}

impl TcData {
    /// TC constant `i` (`common.C[i]`), from `c`.
    pub(crate) fn constant(&self, i: usize) -> i32 {
        self.model.constants[i].1
    }

    /// TC hack `i` (`common.H[i]`), from `h`.
    pub(crate) fn hack(&self, i: usize) -> bool {
        self.model.hacks[i].1
    }

    /// TC text `i` (`common.S[i]`), from `s`.
    pub(crate) fn text(&self, i: usize) -> &[u8] {
        &self.model.texts[i].1
    }

    /// The weapons in menu order (`weapOrder`), as indexes into `weapons`.
    pub(crate) fn weapon_order(&self) -> &[i32] {
        &self.weap_order
    }

    /// Weapon `i`'s name.
    pub(crate) fn weapon_name(&self, i: usize) -> &[u8] {
        &self.model.weapons[i].name
    }
}

/// A model with no tables, sprites or sounds, for tests.
#[cfg(test)]
fn empty_model() -> Common {
    let sprites = || model::SpriteSet {
        width: 0,
        height: 0,
        count: 0,
        data: Vec::new(),
    };
    Common {
        constants: Vec::new(),
        texts: Vec::new(),
        hacks: Vec::new(),
        palette: [[0; 3]; 256],
        color_anim: [(0, 0); 4],
        materials: [0; 256],
        weapons: Vec::new(),
        nobjects: Vec::new(),
        sobjects: Vec::new(),
        textures: [model::Texture::default(); 9],
        bonus_rand_timer: [[0; 2]; 2],
        bonus_sobjects: [0; 2],
        bonus_frames: [0; 2],
        ai_params: [[0; 7]; 2],
        large_sprites: sprites(),
        small_sprites: sprites(),
        text_sprites: sprites(),
        font: Vec::new(),
        sounds: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn bundled() -> Common {
        liero_data::load::load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../TC/openliero")).unwrap()
    }

    #[test]
    fn worm_sprites_face_both_ways_in_both_colours() {
        let c = bundled();
        let s = worm_sprites(&c.large_sprites);
        let px = |f: usize, dir: usize, w: usize, x: usize, y: usize| s[(f + dir * 21 + w * 42) * 256 + y * 16 + x];
        let src = |f: usize, x: usize, y: usize| c.large_sprites.data[(16 + f) * 256 + y * 16 + x];
        let mut recoloured = 0;
        for f in 0..21 {
            for y in 0..16 {
                assert_eq!(px(f, 0, 0, 15, y), 0);
                for x in 0..16 {
                    assert_eq!(px(f, 1, 0, x, y), src(f, x, y));
                    if x < 15 {
                        assert_eq!(px(f, 0, 0, 14 - x, y), src(f, x, y));
                    }
                    let p = src(f, x, y);
                    if (30..=34).contains(&p) {
                        recoloured += 1;
                        assert_eq!(px(f, 1, 1, x, y), p + 9);
                    } else {
                        assert_eq!(px(f, 1, 1, x, y), p);
                    }
                }
            }
        }
        assert!(recoloured > 0);
    }

    #[test]
    fn weapons_are_ordered_by_name() {
        let c = bundled();
        let order = weapon_order(&c.weapons);
        assert_eq!(order.len(), 40);
        assert!(order
            .windows(2)
            .all(|w| c.weapons[w[0] as usize].name <= c.weapons[w[1] as usize].name));
    }

    /// A zip of the files under `dir`, stored, with `/` between folders.
    fn stored_zip(dir: &Path) -> Vec<u8> {
        fn files(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
            let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let name = format!("{prefix}{}", e.file_name().to_str().unwrap());
                if e.path().is_dir() {
                    files(&e.path(), &format!("{name}/"), out);
                } else {
                    out.push((name, std::fs::read(e.path()).unwrap()));
                }
            }
        }
        let mut list = Vec::new();
        files(dir, "", &mut list);
        let (mut out, mut central) = (Vec::new(), Vec::new());
        for (name, data) in &list {
            let crc = crate::fs::tests_crc32(data);
            let head = |sig: u32| {
                let mut h = sig.to_le_bytes().to_vec();
                h.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
                h.extend_from_slice(&crc.to_le_bytes());
                h.extend_from_slice(&(data.len() as u32).to_le_bytes());
                h.extend_from_slice(&(data.len() as u32).to_le_bytes());
                h.extend_from_slice(&(name.len() as u16).to_le_bytes());
                h
            };
            let offset = out.len() as u32;
            out.extend(head(0x0403_4b50));
            out.extend_from_slice(&[0, 0]);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            // The central header has "version made by" before the rest.
            let mut c = 0x0201_4b50u32.to_le_bytes().to_vec();
            c.extend_from_slice(&[20, 0]);
            c.extend(head(0).split_off(4));
            c.extend_from_slice(&[0; 12]);
            c.extend_from_slice(&offset.to_le_bytes());
            c.extend_from_slice(name.as_bytes());
            central.extend(c);
        }
        let start = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(list.len() as u16).to_le_bytes());
        out.extend_from_slice(&(list.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    #[test]
    fn a_zipped_tc_loads_as_its_folder_does() {
        let dir = "target/tc-tests/zipped";
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap();
        let tc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../TC/openliero");
        std::fs::write(format!("{dir}/openliero.zip"), stored_zip(&tc)).unwrap();

        let from_folder = Node::new(b"../../TC/openliero");
        let from_zip = Node::new(format!("{dir}/openliero").as_bytes());
        let (a, b) = (load_shared(&from_folder), load_shared(&from_zip));
        assert!(b.borrow().error.is_empty(), "{}", b.borrow().error);
        let (da, db) = (tc_data(&a), tc_data(&b));
        assert_eq!(da.model.weapons.len(), db.model.weapons.len());
        assert!(da.model.sounds == db.model.sounds);
        assert_eq!(da.samples, db.samples);
    }

    #[test]
    fn loads_from_a_node_and_reports_errors() {
        // Relative to the crate, where the tests run.
        let node = Node::new(b"../../TC/openliero");
        let tc = load_shared(&node);
        assert!(tc.borrow().error.is_empty(), "{}", tc.borrow().error);
        let data = tc_data(&tc);
        assert_eq!(data.model.weapons.len(), 40);
        assert_eq!(data.model.font[b'A' as usize].width, 5);

        assert!(load_checked(&node).is_ok());
        assert_eq!(
            load_checked(&Node::new(b"target/no-such-tc")).err().as_deref(),
            Some("Could not load the TC: Could not read tc.cfg")
        );

        let missing = Node::new(b"target/no-such-tc");
        let tc = load_shared(&missing);
        assert_eq!(tc.borrow().error, "Could not read tc.cfg");
        assert!(tc.borrow().data().is_none());
    }
}
