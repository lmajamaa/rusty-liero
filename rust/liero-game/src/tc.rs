//! Loading a TC for the C++ game: `Common::load` and `Common::precompute`.
//!
//! The reading itself is `liero_data::load`, from a file node (`fs.rs`),
//! since a TC can be a folder, a zip file or both joined. C++ copies the
//! result into `Common` from the `LieroTcData` views here, which point into
//! the `LieroTc` until it's freed.

use crate::level::{LieroTexture, LIERO_TEXTURE_COUNT};
use crate::gfx::LieroColor;
use crate::rand::LieroRand;
use crate::sfx::samples_from_tc;
use crate::text::LieroStr;
use liero_data::model::{self, Common};
use liero_data::Error;
use crate::fs::{LieroFsNode, Node};

#[repr(C)]
pub struct LieroTcWeapon {
    /// Its index in the TC's weapons.
    pub id: i32,
    pub name: LieroStr,
    pub id_str: LieroStr,
    pub detect_distance: i32,
    pub affect_by_worm: bool,
    pub blow_away: i32,
    pub gravity: i32,
    pub shadow: bool,
    pub laser_sight: bool,
    pub launch_sound: i32,
    pub loop_sound: i32,
    pub explo_sound: i32,
    pub speed: i32,
    pub add_speed: i32,
    pub distribution: i32,
    pub parts: i32,
    pub recoil: i32,
    pub mult_speed: i32,
    pub delay: i32,
    pub loading_time: i32,
    pub ammo: i32,
    pub create_on_exp: i32,
    pub dirt_effect: i32,
    pub leave_shells: i32,
    pub leave_shell_delay: i32,
    pub play_reload_sound: bool,
    pub worm_explode: bool,
    pub expl_ground: bool,
    pub worm_collide: bool,
    pub fire_cone: i32,
    pub collide_with_objects: bool,
    pub affect_by_explosions: bool,
    pub bounce: i32,
    pub time_to_explo: i32,
    pub time_to_explo_v: i32,
    pub hit_damage: i32,
    pub blood_on_hit: i32,
    pub start_frame: i32,
    pub num_frames: i32,
    pub loop_anim: bool,
    pub shot_type: i32,
    pub color_bullets: i32,
    pub splinter_amount: i32,
    pub splinter_colour: i32,
    pub splinter_type: i32,
    pub splinter_scatter: i32,
    pub obj_trail_type: i32,
    pub obj_trail_delay: i32,
    pub part_trail_type: i32,
    pub part_trail_obj: i32,
    pub part_trail_delay: i32,
    pub chain_explosion: bool,
}

#[repr(C)]
pub struct LieroTcNObject {
    pub id: i32,
    pub id_str: LieroStr,
    pub detect_distance: i32,
    pub gravity: i32,
    pub speed: i32,
    pub speed_v: i32,
    pub distribution: i32,
    pub blow_away: i32,
    pub bounce: i32,
    pub hit_damage: i32,
    pub worm_explode: bool,
    pub expl_ground: bool,
    pub worm_destroy: bool,
    pub blood_on_hit: i32,
    pub start_frame: i32,
    pub num_frames: i32,
    pub draw_on_map: bool,
    pub color_bullets: i32,
    pub create_on_exp: i32,
    pub affect_by_explosions: bool,
    pub dirt_effect: i32,
    pub splinter_amount: i32,
    pub splinter_colour: i32,
    pub splinter_type: i32,
    pub blood_trail: bool,
    pub blood_trail_delay: i32,
    pub leave_obj: i32,
    pub leave_obj_delay: i32,
    pub time_to_explo: i32,
    pub time_to_explo_v: i32,
}

#[repr(C)]
pub struct LieroTcSObject {
    pub id: i32,
    pub id_str: LieroStr,
    pub start_sound: i32,
    pub num_sounds: i32,
    pub anim_delay: i32,
    pub start_frame: i32,
    pub num_frames: i32,
    pub detect_range: i32,
    pub damage: i32,
    pub blow_away: i32,
    pub shadow: bool,
    pub shake: i32,
    pub flash: i32,
    pub dirt_effect: i32,
}

#[repr(C)]
pub struct LieroTcSound {
    pub name: LieroStr,
    /// Signed 8-bit samples.
    pub data: *const u8,
    pub len: usize,
    /// False when the WAV file had another format: the game has no sound.
    pub valid: bool,
    /// The 16-bit samples the mixer plays, twice as many as `len`
    /// (`SfxSample::createSound`). They stay put while a sound plays.
    pub samples: *const i16,
    pub sample_count: usize,
}

#[repr(C)]
pub struct LieroTcSprites {
    pub data: *const u8,
    pub width: i32,
    pub height: i32,
    pub count: i32,
}

#[repr(C)]
pub struct LieroTcFontChar {
    /// 7x8 pixels, row-major: 8 where set.
    pub data: [u8; 56],
    pub width: i32,
}

/// Characters in the font (`Font::chars`).
pub const LIERO_TC_FONT_CHARS: usize = 250;

/// Everything `Common` gets from a TC.
#[repr(C)]
pub struct LieroTcData {
    pub weapons: *const LieroTcWeapon,
    pub weapon_count: usize,
    pub nobjects: *const LieroTcNObject,
    pub nobject_count: usize,
    pub sobjects: *const LieroTcSObject,
    pub sobject_count: usize,
    pub sounds: *const LieroTcSound,
    pub sound_count: usize,
    /// In the order of `LIERO_CDEFS`, `LIERO_SDEFS` and `LIERO_HDEFS`.
    pub constants: *const i32,
    pub constant_count: usize,
    pub texts: *const LieroStr,
    pub text_count: usize,
    pub hacks: *const bool,
    pub hack_count: usize,
    /// 6-bit VGA palette, red, green and blue for each of 256 entries.
    pub palette: [u8; 768],
    /// The same palette as the game's `Palette` holds it (`Common::exepal`).
    pub palette_colors: [LieroColor; 256],
    /// (from, to) for each of 4 colour animations.
    pub color_anim: [i32; 8],
    pub materials: [u8; 256],
    pub textures: [LieroTexture; LIERO_TEXTURE_COUNT],
    /// [bonus * 2 + (0 = timer, 1 = timerV)].
    pub bonus_rand_timer: [i32; 4],
    pub bonus_sobjects: [i32; 2],
    pub bonus_frames: [i32; 2],
    /// [(0 = off, 1 = on) * 7 + key].
    pub ai_params: [i32; 14],
    pub small_sprites: LieroTcSprites,
    pub large_sprites: LieroTcSprites,
    pub text_sprites: LieroTcSprites,
    pub font: [LieroTcFontChar; LIERO_TC_FONT_CHARS],
    /// Precomputed: weapon indexes sorted by name, and the worm and fire cone
    /// sprites facing both ways (`Common::precompute`).
    pub weap_order: *const i32,
    pub worm_sprites: LieroTcSprites,
    pub fire_cone_sprites: LieroTcSprites,
}

/// A loaded TC, or the error that stopped loading it.
pub struct LieroTc {
    error: String,
    common: Option<Common>,
    weapons: Vec<LieroTcWeapon>,
    nobjects: Vec<LieroTcNObject>,
    sobjects: Vec<LieroTcSObject>,
    sounds: Vec<LieroTcSound>,
    samples: Vec<Vec<i16>>,
    constants: Vec<i32>,
    texts: Vec<LieroStr>,
    hacks: Vec<bool>,
    weap_order: Vec<i32>,
    worm_sprites: Vec<u8>,
    fire_cone_sprites: Vec<u8>,
    data: Option<Box<LieroTcData>>,
}

fn str(s: &[u8]) -> LieroStr {
    LieroStr { ptr: s.as_ptr(), len: s.len() }
}

fn sprites(s: &model::SpriteSet) -> LieroTcSprites {
    LieroTcSprites { data: s.data.as_ptr(), width: s.width as i32, height: s.height as i32, count: s.count as i32 }
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

impl LieroTc {
    fn failed(error: String) -> Self {
        LieroTc {
            error,
            common: None,
            weapons: Vec::new(),
            nobjects: Vec::new(),
            sobjects: Vec::new(),
            sounds: Vec::new(),
            samples: Vec::new(),
            constants: Vec::new(),
            texts: Vec::new(),
            hacks: Vec::new(),
            weap_order: Vec::new(),
            worm_sprites: Vec::new(),
            fire_cone_sprites: Vec::new(),
            data: None,
        }
    }

    fn new(common: Common) -> Result<Self, Error> {
        if common.large_sprites.count < 37 {
            return Err(Error("the TC has too few large sprites".into()));
        }
        let mut tc = LieroTc::failed(String::new());
        let c = &common;
        tc.weapons = c
            .weapons
            .iter()
            .enumerate()
            .map(|(i, w)| LieroTcWeapon {
                id: i as i32,
                name: str(&w.name),
                id_str: str(w.id_str.as_bytes()),
                detect_distance: w.detect_distance,
                affect_by_worm: w.affect_by_worm,
                blow_away: w.blow_away,
                gravity: w.gravity,
                shadow: w.shadow,
                laser_sight: w.laser_sight,
                launch_sound: w.launch_sound,
                loop_sound: w.loop_sound,
                explo_sound: w.explo_sound,
                speed: w.speed,
                add_speed: w.add_speed,
                distribution: w.distribution,
                parts: w.parts,
                recoil: w.recoil,
                mult_speed: w.mult_speed,
                delay: w.delay,
                loading_time: w.loading_time,
                ammo: w.ammo,
                create_on_exp: w.create_on_exp,
                dirt_effect: w.dirt_effect,
                leave_shells: w.leave_shells,
                leave_shell_delay: w.leave_shell_delay,
                play_reload_sound: w.play_reload_sound,
                worm_explode: w.worm_explode,
                expl_ground: w.expl_ground,
                worm_collide: w.worm_collide,
                fire_cone: w.fire_cone,
                collide_with_objects: w.collide_with_objects,
                affect_by_explosions: w.affect_by_explosions,
                bounce: w.bounce,
                time_to_explo: w.time_to_explo,
                time_to_explo_v: w.time_to_explo_v,
                hit_damage: w.hit_damage,
                blood_on_hit: w.blood_on_hit,
                start_frame: w.start_frame,
                num_frames: w.num_frames,
                loop_anim: w.loop_anim,
                shot_type: w.shot_type,
                color_bullets: w.color_bullets,
                splinter_amount: w.splinter_amount,
                splinter_colour: w.splinter_colour,
                splinter_type: w.splinter_type,
                splinter_scatter: w.splinter_scatter,
                obj_trail_type: w.obj_trail_type,
                obj_trail_delay: w.obj_trail_delay,
                part_trail_type: w.part_trail_type,
                part_trail_obj: w.part_trail_obj,
                part_trail_delay: w.part_trail_delay,
                chain_explosion: w.chain_explosion,
            })
            .collect();
        tc.nobjects = c
            .nobjects
            .iter()
            .enumerate()
            .map(|(i, n)| LieroTcNObject {
                id: i as i32,
                id_str: str(n.id_str.as_bytes()),
                detect_distance: n.detect_distance,
                gravity: n.gravity,
                speed: n.speed,
                speed_v: n.speed_v,
                distribution: n.distribution,
                blow_away: n.blow_away,
                bounce: n.bounce,
                hit_damage: n.hit_damage,
                worm_explode: n.worm_explode,
                expl_ground: n.expl_ground,
                worm_destroy: n.worm_destroy,
                blood_on_hit: n.blood_on_hit,
                start_frame: n.start_frame,
                num_frames: n.num_frames,
                draw_on_map: n.draw_on_map,
                color_bullets: n.color_bullets,
                create_on_exp: n.create_on_exp,
                affect_by_explosions: n.affect_by_explosions,
                dirt_effect: n.dirt_effect,
                splinter_amount: n.splinter_amount,
                splinter_colour: n.splinter_colour,
                splinter_type: n.splinter_type,
                blood_trail: n.blood_trail,
                blood_trail_delay: n.blood_trail_delay,
                leave_obj: n.leave_obj,
                leave_obj_delay: n.leave_obj_delay,
                time_to_explo: n.time_to_explo,
                time_to_explo_v: n.time_to_explo_v,
            })
            .collect();
        tc.sobjects = c
            .sobjects
            .iter()
            .enumerate()
            .map(|(i, s)| LieroTcSObject {
                id: i as i32,
                id_str: str(s.id_str.as_bytes()),
                start_sound: s.start_sound,
                num_sounds: s.num_sounds,
                anim_delay: s.anim_delay,
                start_frame: s.start_frame,
                num_frames: s.num_frames,
                detect_range: s.detect_range,
                damage: s.damage,
                blow_away: s.blow_away,
                shadow: s.shadow,
                shake: s.shake,
                flash: s.flash,
                dirt_effect: s.dirt_effect,
            })
            .collect();
        tc.samples = c
            .sounds
            .iter()
            .map(|s| {
                let data = s.data.as_deref().unwrap_or(&[]);
                let mut out = vec![0i16; data.len() * 2];
                samples_from_tc(data, &mut out);
                out
            })
            .collect();
        tc.sounds = c
            .sounds
            .iter()
            .zip(&tc.samples)
            .map(|(s, samples)| {
                let data = s.data.as_deref().unwrap_or(&[]);
                LieroTcSound {
                    name: str(s.name.as_bytes()),
                    data: data.as_ptr(),
                    len: data.len(),
                    valid: s.data.is_some(),
                    samples: samples.as_ptr(),
                    sample_count: samples.len(),
                }
            })
            .collect();
        tc.constants = c.constants.iter().map(|&(_, v)| v).collect();
        tc.texts = c.texts.iter().map(|(_, t)| str(t)).collect();
        tc.hacks = c.hacks.iter().map(|&(_, h)| h).collect();
        tc.weap_order = weapon_order(&c.weapons);
        tc.worm_sprites = worm_sprites(&c.large_sprites);
        tc.fire_cone_sprites = fire_cone_sprites(&c.large_sprites);

        let mut palette = [0u8; 768];
        for (i, entry) in c.palette.iter().enumerate() {
            palette[i * 3..i * 3 + 3].copy_from_slice(entry);
        }
        let mut color_anim = [0; 8];
        for (i, &(from, to)) in c.color_anim.iter().enumerate() {
            color_anim[i * 2] = from;
            color_anim[i * 2 + 1] = to;
        }
        let textures = c.textures.map(|t| LieroTexture {
            n_draw_back: t.n_draw_back,
            m_frame: t.m_frame,
            s_frame: t.s_frame,
            r_frame: t.r_frame,
        });
        let font = std::array::from_fn(|i| match c.font.get(i) {
            Some(ch) => LieroTcFontChar { data: ch.data, width: ch.width as i32 },
            None => LieroTcFontChar { data: [0; 56], width: 0 },
        });

        let data = LieroTcData {
            weapons: tc.weapons.as_ptr(),
            weapon_count: tc.weapons.len(),
            nobjects: tc.nobjects.as_ptr(),
            nobject_count: tc.nobjects.len(),
            sobjects: tc.sobjects.as_ptr(),
            sobject_count: tc.sobjects.len(),
            sounds: tc.sounds.as_ptr(),
            sound_count: tc.sounds.len(),
            constants: tc.constants.as_ptr(),
            constant_count: tc.constants.len(),
            texts: tc.texts.as_ptr(),
            text_count: tc.texts.len(),
            hacks: tc.hacks.as_ptr(),
            hack_count: tc.hacks.len(),
            palette,
            palette_colors: std::array::from_fn(|i| LieroColor {
                r: palette[i * 3],
                g: palette[i * 3 + 1],
                b: palette[i * 3 + 2],
                unused: 0,
            }),
            color_anim,
            materials: c.materials,
            textures,
            bonus_rand_timer: [
                c.bonus_rand_timer[0][0],
                c.bonus_rand_timer[0][1],
                c.bonus_rand_timer[1][0],
                c.bonus_rand_timer[1][1],
            ],
            bonus_sobjects: c.bonus_sobjects,
            bonus_frames: c.bonus_frames,
            ai_params: std::array::from_fn(|i| c.ai_params[i / 7][i % 7]),
            small_sprites: sprites(&c.small_sprites),
            large_sprites: sprites(&c.large_sprites),
            text_sprites: sprites(&c.text_sprites),
            font,
            weap_order: tc.weap_order.as_ptr(),
            worm_sprites: LieroTcSprites { data: tc.worm_sprites.as_ptr(), width: 16, height: 16, count: 2 * 2 * 21 },
            fire_cone_sprites: LieroTcSprites { data: tc.fire_cone_sprites.as_ptr(), width: 16, height: 16, count: 2 * 7 },
        };
        tc.data = Some(Box::new(data));
        // The views point into `common`'s heap data, which moving it keeps.
        tc.common = Some(common);
        Ok(tc)
    }
}

impl LieroTc {
    /// F8 on the menu: the developers' random TC (`Gfx::randomizeTc`). The
    /// values come from a generator seeded with 14, as the C++ did. The
    /// order the object types get theirs in is shuffled from `shuffle_seed`;
    /// the C++ shuffled it with `std::random_device`, so it never repeated.
    pub fn randomize(&mut self, shuffle_seed: u32) {
        let Some(c) = &self.common else { return };
        let mut r = LieroRand::new(14);
        let sobjects = self.sobjects.len() as u32;
        let nobjects = self.nobjects.len() as u32;
        let sounds = self.sounds.len() as u32;
        let small = c.small_sprites.count as u32;
        let large = c.large_sprites.count as u32;

        let mut order: Vec<usize> = (0..self.nobjects.len()).collect();
        let mut shuffle = LieroRand::new(shuffle_seed);
        for i in (1..order.len()).rev() {
            order.swap(i, shuffle.below(i as u32 + 1) as usize);
        }

        // `r(n)` as the C++ wrote it: unsigned, then stored in an int.
        let mut r = |n: u32| r.below(n) as i32;
        for w in &mut self.weapons {
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
            w.leave_shells = (r(1) == 0) as i32;
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
            let n = &mut self.nobjects[i];
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
            n.splinter_type = if idx > 0 { order[r(idx as u32) as usize] as i32 } else { 0 };
            n.time_to_explo = 50 + r(70 * 3);
            n.time_to_explo_v = r(30);
            n.worm_destroy = r(3) == 0;
            n.worm_explode = r(2) == 0;
        }

        for s in &mut self.sobjects {
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

/// F8's random TC (`LieroTc::randomize`). What `liero_tc_data` points to
/// changes in place.
///
/// # Safety
/// `tc` must come from `liero_tc_load`.
#[no_mangle]
pub unsafe extern "C" fn liero_tc_randomize(tc: &mut LieroTc, shuffle_seed: u32) {
    tc.randomize(shuffle_seed);
}

/// Loads the TC in `node`, each file at its path inside it (`/` between
/// folders).
fn load(node: &Node) -> LieroTc {
    let mut read = |path: &str| {
        let file = path.as_bytes().split(|&c| c == b'/').fold(node.clone(), |n, part| n.go(part));
        file.read().ok_or_else(|| Error(format!("Could not read {path}")))
    };
    match liero_data::load::load(&mut read).and_then(LieroTc::new) {
        Ok(tc) => tc,
        Err(e) => LieroTc::failed(e.0),
    }
}

/// Loads the TC in `node` (`Common::load`). Always returns a TC to free with
/// `liero_tc_free`; `liero_tc_error` says whether loading failed.
#[no_mangle]
pub extern "C" fn liero_tc_load(node: &LieroFsNode) -> *mut LieroTc {
    Box::into_raw(Box::new(load(&node.0)))
}

/// Why loading failed, or an empty string.
///
/// # Safety
/// `tc` must come from `liero_tc_load`.
#[no_mangle]
pub unsafe extern "C" fn liero_tc_error(tc: &LieroTc) -> LieroStr {
    str(tc.error.as_bytes())
}

/// The loaded data, or null if loading failed. Valid until `liero_tc_free`.
///
/// # Safety
/// `tc` must come from `liero_tc_load`.
#[no_mangle]
pub unsafe extern "C" fn liero_tc_data(tc: &LieroTc) -> *const LieroTcData {
    match &tc.data {
        Some(d) => &**d,
        None => std::ptr::null(),
    }
}

/// # Safety
/// `tc` must come from `liero_tc_load` and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn liero_tc_free(tc: *mut LieroTc) {
    if !tc.is_null() {
        drop(Box::from_raw(tc));
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
    #[cfg_attr(target_os = "emscripten", ignore = "reads the bundled TC from the host")]
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
    #[cfg_attr(target_os = "emscripten", ignore = "reads the bundled TC from the host")]
    fn weapons_are_ordered_by_name() {
        let c = bundled();
        let order = weapon_order(&c.weapons);
        assert_eq!(order.len(), 40);
        assert!(order.windows(2).all(|w| c.weapons[w[0] as usize].name <= c.weapons[w[1] as usize].name));
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
    #[cfg_attr(target_os = "emscripten", ignore = "reads the bundled TC from the host")]
    fn a_zipped_tc_loads_as_its_folder_does() {
        let dir = "target/tc-tests/zipped";
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap();
        let tc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../TC/openliero");
        std::fs::write(format!("{dir}/openliero.zip"), stored_zip(&tc)).unwrap();

        let from_folder = LieroFsNode(Node::new(b"../../TC/openliero"));
        let from_zip = LieroFsNode(Node::new(format!("{dir}/openliero").as_bytes()));
        unsafe {
            let (a, b) = (liero_tc_load(&from_folder), liero_tc_load(&from_zip));
            assert!((*b).error.is_empty(), "{}", (*b).error);
            let (da, db) = (&*liero_tc_data(&*a), &*liero_tc_data(&*b));
            assert_eq!(da.weapon_count, db.weapon_count);
            assert_eq!(da.sound_count, db.sound_count);
            for i in 0..da.sound_count {
                let (sa, sb) = (&*da.sounds.add(i), &*db.sounds.add(i));
                assert_eq!((sa.valid, sa.len), (sb.valid, sb.len));
            }
            liero_tc_free(a);
            liero_tc_free(b);
        }
    }

    #[test]
    #[cfg_attr(target_os = "emscripten", ignore = "reads the bundled TC from the host")]
    fn loads_from_a_node_and_reports_errors() {
        // Relative to the crate, where the tests run.
        let node = LieroFsNode(Node::new(b"../../TC/openliero"));
        unsafe {
            let tc = liero_tc_load(&node);
            assert!((*tc).error.is_empty(), "{}", (*tc).error);
            let data = &*liero_tc_data(&*tc);
            assert_eq!(data.weapon_count, 40);
            assert_eq!(data.font[b'A' as usize].width, 5);
            liero_tc_free(tc);

            let missing = LieroFsNode(Node::new(b"target/no-such-tc"));
            let tc = liero_tc_load(&missing);
            assert_eq!(liero_tc_error(&*tc).bytes(), b"Could not read tc.cfg");
            assert!(liero_tc_data(&*tc).is_null());
            liero_tc_free(tc);
        }
    }
}
