//! Viewports (`viewport.cpp` and `spectatorviewport.cpp`): where each camera
//! looks, and what is drawn in it. The split-screen viewport follows one worm;
//! the spectator viewport shows the whole level with both worms' stats.
//! Results have to stay bit-identical to the C++ versions, which the frame
//! hashes in `tools/hashrun` check.
//!
//! C++ still owns the game. For each draw it fills in a `LieroScene`: plain
//! copies of the worms and objects in C++'s list order, the tables and
//! sprites they are drawn with, and the texts. Rust decides everything that
//! is drawn from that. Rust owns a viewport's own state (`LieroViewport`),
//! and the fields of C++'s `Viewport` refer to it.


use crate::ai::predictive::LieroAiTrace;
use crate::game::{LieroHoldazone, LieroRect};
use crate::gfx::{
    liero_font_draw_text, liero_gfx_blit_fire_cone, liero_gfx_blit_image, liero_gfx_blit_image_no_key_colour,
    liero_gfx_blit_image_r, liero_gfx_blit_image_trans, liero_gfx_blit_shadow_image, liero_gfx_draw_bar,
    liero_gfx_draw_dashed_line_box, liero_gfx_draw_laser_sight, liero_gfx_draw_line, liero_gfx_draw_ninjarope,
    liero_gfx_draw_shadow_line, liero_gfx_fill_rect, LieroBitmap, LieroFontChar,
};
use crate::level::{liero_level_draw_miniature, LieroLevel};
use crate::material::LieroMaterial;
use crate::math::{cos_sin, ftoi, LieroIVec2};
use crate::rand::LieroRand;
use crate::text::{time_to_string, time_to_string_ex, LieroStr};
use crate::worm::{
    liero_worm_angle_frame, LIERO_GAME_GAME_OF_TAG, LIERO_GAME_HOLDAZONE, LIERO_GAME_KILL_EM_ALL,
    LIERO_GAME_SCALES_OF_JUSTICE,
};

/// The viewport that follows one worm, in split screen.
pub const LIERO_VIEWPORT_WORM: i32 = 0;
/// The viewport that shows the whole level, in spectator mode.
pub const LIERO_VIEWPORT_SPECTATOR: i32 = 1;

/// The fire cone's position for each direction and angle frame, relative to
/// the worm sprite (`Common::fireConeOffset`).
const FIRE_CONE_OFFSET: [[[i32; 2]; 7]; 2] = [
    [[-3, 1], [-4, 0], [-4, -2], [-4, -4], [-3, -5], [-2, -6], [0, -6]],
    [[3, 1], [4, 0], [4, -2], [4, -4], [3, -5], [2, -6], [0, -6]],
];

/// Worm colours in the palette (`Palette::wormColourIndexes`).
const WORM_COLOUR_INDEXES: [i32; 2] = [0x58, 0x78];

/// A viewport's state. Rust owns it, and the fields of C++'s `Viewport`
/// refer to it (`liero_viewport_new`).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroViewport {
    /// Top left of what is shown, in level coordinates.
    pub x: i32,
    pub y: i32,
    /// Fixed point.
    pub shake: i32,
    pub max_x: i32,
    pub max_y: i32,
    pub center_x: i32,
    pub center_y: i32,
    pub worm_idx: i32,
    /// Where the kill message is, -8 when hidden.
    pub banner_y: i32,
    /// Where on screen the viewport is.
    pub rect: LieroRect,
    /// Used for shaking and for the laser sight.
    pub rand: LieroRand,
}

/// A C++ `SpriteSet`: `count` sprites of `width` x `height` pixels, one after
/// the other.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroSpriteSet {
    pub data: *const u8,
    pub width: i32,
    pub height: i32,
    pub count: i32,
}

/// What viewports use of a weapon type.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroViewWeapon {
    pub name: LieroStr,
    pub ammo: i32,
    pub loading_time: i32,
    /// `Weapon::computedLoadingTime` with the game's settings.
    pub computed_loading_time: i32,
    pub laser_sight: bool,
    pub color_bullets: i32,
    pub fire_cone: i32,
    pub start_frame: i32,
    pub shot_type: i32,
    pub shadow: bool,
}

/// One of a worm's weapon slots.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroViewWormWeapon {
    /// Index into `LieroScene::weapons`.
    pub weapon: i32,
    pub ammo: i32,
    pub loading_left: i32,
}

/// What viewports use of a worm.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroViewWorm {
    pub pos: LieroIVec2,
    pub visible: bool,
    pub ready: bool,
    pub health: i32,
    /// The worm's full health (`settings->health`).
    pub max_health: i32,
    pub killed_timer: i32,
    pub stats_x: i32,
    pub kills: i32,
    pub lives: i32,
    pub timer: i32,
    pub index: i32,
    pub name: LieroStr,
    /// The worm's colour (`settings->color`).
    pub color: i32,
    pub pressed_change: bool,
    pub pressed_fire: bool,
    pub current_frame: i32,
    pub direction: i32,
    pub hotspot_x: i32,
    pub hotspot_y: i32,
    pub aiming_angle: i32,
    pub make_sight_green: bool,
    pub rope_out: bool,
    pub rope_pos: LieroIVec2,
    pub fire_cone: i32,
    pub last_killed_by_idx: i32,
    pub current_weapon: i32,
    pub weapons: [LieroViewWormWeapon; 5],
    pub steerable_count: i32,
    pub steerable_sum_x: i32,
    pub steerable_sum_y: i32,
    /// Where the worm's predictive bot's plans went, with `aiTraces` on,
    /// drawn as debug output.
    pub traces: *const LieroAiTrace,
    pub trace_count: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroViewBonus {
    pub x: i32,
    pub y: i32,
    pub timer: i32,
    pub frame: i32,
    pub weapon: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroViewSObject {
    pub x: i32,
    pub y: i32,
    pub cur_frame: i32,
    /// The type's `startFrame`.
    pub start_frame: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroViewWObject {
    pub pos: LieroIVec2,
    /// Index into `LieroScene::weapons`.
    pub weapon: i32,
    pub cur_frame: i32,
    /// Where the object is in C++'s list, which picks a name for weapon 34.
    pub slot: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroViewNObject {
    pub pos: LieroIVec2,
    pub cur_frame: i32,
    /// The type's `startFrame`.
    pub start_frame: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroViewBObject {
    pub pos: LieroIVec2,
    pub color: i32,
}

/// A split-screen viewport, for the kill messages that every viewport shows.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroViewportBanner {
    pub worm_idx: i32,
    pub banner_y: i32,
}

/// Everything a viewport draws. Lists are in C++'s order, which decides what
/// is drawn on top.
#[repr(C)]
pub struct LieroScene {
    /// In the order of `Game::worms`; worm numbers are positions in it.
    pub worms: *const LieroViewWorm,
    pub worm_count: i32,
    /// `Common::weapons`.
    pub weapons: *const LieroViewWeapon,
    pub weapon_count: i32,
    pub bonuses: *const LieroViewBonus,
    pub bonus_count: i32,
    pub sobjects: *const LieroViewSObject,
    pub sobject_count: i32,
    pub wobjects: *const LieroViewWObject,
    pub wobject_count: i32,
    pub nobjects: *const LieroViewNObject,
    pub nobject_count: i32,
    pub bobjects: *const LieroViewBObject,
    pub bobject_count: i32,
    /// The split-screen viewports, `Game::viewports`.
    pub viewports: *const LieroViewportBanner,
    pub viewport_count: i32,

    pub small_sprites: LieroSpriteSet,
    pub large_sprites: LieroSpriteSet,
    pub worm_sprites: LieroSpriteSet,
    pub fire_cone_sprites: LieroSpriteSet,
    pub text_sprites: LieroSpriteSet,
    pub font: *const LieroFontChar,
    pub font_count: usize,
    /// 256 entries.
    pub materials: *const LieroMaterial,
    pub level: LieroLevel,
    pub bonus_frames: [i32; 2],

    pub reloading: LieroStr,
    pub kills: LieroStr,
    pub lives: LieroStr,
    pub press_fire: LieroStr,
    pub youre_it: LieroStr,
    pub killed_msg: LieroStr,
    pub committed_suicide_msg: LieroStr,

    pub cycles: i32,
    /// `LIERO_GAME_*`.
    pub game_mode: i32,
    pub holdazone: LieroHoldazone,
    pub zone_capture_time: i32,
    pub last_killed_idx: i32,
    pub got_changed: bool,
    pub shadow: bool,
    pub names_on_bonuses: bool,
    pub allow_viewing_spawn_point: bool,
    pub map: bool,
    /// The TC's `HRemExp` hack.
    pub rem_exp: bool,
    /// `LC(LaserWeapon)`, counting from 1.
    pub laser_weapon: i32,
    pub bonus_flicker_time: i32,
    /// `LC(NRColourBegin)` and `LC(NRColourEnd)`.
    pub nr_colour_begin: i32,
    pub nr_colour_end: i32,

    pub render_res_x: i32,
    pub render_res_y: i32,
    /// The game is running (`StateGame`), not paused or over.
    pub state_game: bool,
    pub is_replay: bool,
}

/// C++ integer division, which Rust panics on where C++ is undefined.
fn div(a: i32, b: i32) -> i32 {
    if b == 0 {
        0
    } else {
        a.wrapping_div(b)
    }
}

/// `Worm::minimapColor`.
fn minimap_color(index: i32) -> i32 {
    129 + index * 4
}

impl LieroSpriteSet {
    /// Sprite `frame`, or None where C++ would have read outside the set.
    unsafe fn sprite(&self, frame: i32) -> Option<*const u8> {
        (0..self.count)
            .contains(&frame)
            .then(|| self.data.add(frame as usize * (self.width * self.height) as usize))
    }
}

unsafe fn slice<'a, T>(ptr: *const T, len: i32) -> &'a [T] {
    if len <= 0 || ptr.is_null() {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len as usize)
    }
}

/// The scene as Rust reads it.
struct Scene<'a> {
    s: &'a LieroScene,
    worms: &'a [LieroViewWorm],
    weapons: &'a [LieroViewWeapon],
}

impl<'a> Scene<'a> {
    unsafe fn new(s: &'a LieroScene) -> Self {
        Scene { s, worms: slice(s.worms, s.worm_count), weapons: slice(s.weapons, s.weapon_count) }
    }

    /// `Game::wormByIdx`.
    fn worm(&self, idx: i32) -> Option<&'a LieroViewWorm> {
        usize::try_from(idx).ok().and_then(|i| self.worms.get(i))
    }

    fn weapon(&self, idx: i32) -> Option<&'a LieroViewWeapon> {
        usize::try_from(idx).ok().and_then(|i| self.weapons.get(i))
    }

    unsafe fn text(&self, scr: &LieroBitmap, text: &[u8], x: i32, y: i32, color: i32) {
        liero_font_draw_text(scr, self.s.font, self.s.font_count, text.as_ptr(), text.len(), x, y, color as u8, 1);
    }

    /// The text with a black shadow, as banners are drawn.
    unsafe fn banner(&self, scr: &LieroBitmap, text: &[u8], x: i32, y: i32) {
        self.text(scr, text, x + 1, y + 1, 0);
        self.text(scr, text, x, y, 50);
    }

    /// Capital letters from the small font, 4 pixels apart: `drawTextSmall`.
    unsafe fn text_small(&self, scr: &LieroBitmap, text: &[u8], mut x: i32, y: i32) {
        let set = &self.s.text_sprites;
        for &ch in text {
            let c = ch.wrapping_sub(b'A');
            if c < 26 {
                if let Some(mem) = set.sprite(c as i32) {
                    liero_gfx_blit_image(scr, mem, x, y, set.width, set.height, set.width);
                }
            }
            x += 4;
        }
    }

    /// The weapon's name in the small font, centred on x.
    unsafe fn weapon_name(&self, scr: &LieroBitmap, weapon: i32, x: i32, y: i32) {
        if let Some(w) = self.weapon(weapon) {
            let name = w.name.bytes();
            let len = name.len() as i32 * 4;
            self.text_small(scr, name, x - len / 2, y);
        }
    }

    unsafe fn blit(&self, scr: &LieroBitmap, set: &LieroSpriteSet, frame: i32, x: i32, y: i32) {
        if let Some(mem) = set.sprite(frame) {
            liero_gfx_blit_image(scr, mem, x, y, set.width, set.height, set.width);
        }
    }

    unsafe fn shadow(&self, scr: &LieroBitmap, set: &LieroSpriteSet, frame: i32, x: i32, y: i32, w: i32, h: i32) {
        if let Some(mem) = set.sprite(frame) {
            liero_gfx_blit_shadow_image(scr, self.s.materials, mem, x, y, w, h);
        }
    }

    /// Darkens the pixel at (x, y) if inside the clip rectangle and its
    /// material shows shadows.
    unsafe fn shadow_pixel(&self, scr: &LieroBitmap, x: i32, y: i32) {
        if scr.clip.contains(x, y) {
            let p = scr.pixels.offset(y as isize * scr.pitch as isize + x as isize);
            if (*self.s.materials.add(*p as usize)).see_shadow() {
                *p = (*p).wrapping_add(4);
            }
        }
    }

    unsafe fn pixel(&self, scr: &LieroBitmap, x: i32, y: i32, color: i32) {
        if scr.clip.contains(x, y) {
            *scr.pixels.offset(y as isize * scr.pitch as isize + x as isize) = color as u8;
        }
    }

    /// `getPixel(x, y) = color` without clipping, kept inside the buffer.
    unsafe fn raw_pixel(&self, scr: &LieroBitmap, x: i32, y: i32, color: i32) {
        let i = y as i64 * scr.pitch as i64 + x as i64;
        if 0 <= i && i < scr.pitch as i64 * scr.h as i64 {
            *scr.pixels.offset(i as isize) = color as u8;
        }
    }

    /// The life bar, ammo or loading bar and the kills, as both kinds of
    /// viewport draw them for a worm. `x` is where the stats start.
    unsafe fn draw_stats(&self, scr: &LieroBitmap, worm: &LieroViewWorm, x: i32, reloading_y: i32) {
        let res_y = self.s.render_res_y;
        if worm.visible {
            let width = div(worm.health.wrapping_mul(100), worm.max_health);
            liero_gfx_draw_bar(scr, x, res_y - 39, width, 2, (width / 10 + 234) as u8);
        } else {
            let width = 100 - (worm.killed_timer * 25) / 37;
            if width > 0 {
                let width = width.min(100);
                liero_gfx_draw_bar(scr, x, res_y - 39, width, 2, (width / 10 + 234) as u8);
            }
        }

        let ww = &worm.weapons[worm.current_weapon as usize % 5];
        let weapon = self.weapon(ww.weapon);
        if ww.loading_left == 0 {
            if ww.ammo > 0 {
                let width = div(ww.ammo.wrapping_mul(100), weapon.map_or(0, |w| w.ammo));
                if width > 0 {
                    liero_gfx_draw_bar(scr, x, res_y - 34, width, 2, (width / 10 + 245) as u8);
                }
            }
        } else {
            let width = match weapon {
                Some(w) if w.loading_time != 0 => 100 - div(ww.loading_left.wrapping_mul(100), w.computed_loading_time),
                _ => 100 - ww.loading_left.wrapping_mul(100),
            };
            if width > 0 {
                liero_gfx_draw_bar(scr, x, res_y - 34, width, 2, (width / 10 + 245) as u8);
            }
            if self.s.cycles % 20 > 10 && worm.visible {
                self.text(scr, self.s.reloading.bytes(), x, reloading_y, 50);
            }
        }

        let mut kills = self.s.kills.bytes().to_vec();
        kills.extend_from_slice(worm.kills.to_string().as_bytes());
        self.text(scr, &kills, x, res_y - 29, 10);
    }

    /// The worm's name, colour and the game time.
    /// `box_x` is where the colour's frame starts and `color_x` the colour.
    unsafe fn draw_name_and_time(
        &self,
        scr: &LieroBitmap,
        worm: &LieroViewWorm,
        x: i32,
        box_x: i32,
        color_x: i32,
        time_x: i32,
    ) {
        let res_y = self.s.render_res_y;
        self.text(scr, worm.name.bytes(), x, res_y - 15, 7);
        liero_gfx_fill_rect(scr, box_x, res_y - 8, 8, 8, 7);
        liero_gfx_fill_rect(scr, color_x, res_y - 7, 6, 6, worm.color as u8);
        let time = time_to_string_ex(self.s.cycles.wrapping_mul(14), false, true);
        self.text(scr, &time, time_x, res_y - 15, 7);
    }

    /// Lives, or the Holdazone or Game of Tag timer, of worm `pos` (a position
    /// in the worm list). `scale` is the viewport's multiplier for the
    /// timer's position.
    unsafe fn draw_game_mode(&self, scr: &LieroBitmap, pos: i32, x: i32, scale: i32) {
        const STATE_COLOURS: [[i32; 2]; 2] = [[6, 10], [79, 4]];
        let s = self.s;
        let Some(worm) = self.worm(pos) else { return };
        let (holder, ahead): (i32, fn(i32, i32) -> bool) = match s.game_mode {
            LIERO_GAME_KILL_EM_ALL | LIERO_GAME_SCALES_OF_JUSTICE => {
                let mut lives = s.lives.bytes().to_vec();
                lives.extend_from_slice(worm.lives.to_string().as_bytes());
                self.text(scr, &lives, x, s.render_res_y - 22, 6);
                return;
            }
            LIERO_GAME_HOLDAZONE => (s.holdazone.holder_idx, |other, mine| other <= mine),
            LIERO_GAME_GAME_OF_TAG => (s.last_killed_idx, |other, mine| other >= mine),
            _ => return,
        };

        let state = self
            .worms
            .iter()
            .enumerate()
            .any(|(i, w)| i as i32 != pos && ahead(w.timer, worm.timer)) as usize;
        let color = STATE_COLOURS[(holder != worm.index) as usize][state];

        // C++ calls drawText(scr, str, len, x, y, color) with these four
        // numbers, so 5 * scale is the length and the rest are shifted by one.
        // Past the five characters it read a NUL, which only starts a new
        // line, and then other memory; zeros stand in for that.
        let len = (5 * scale).max(0) as usize;
        let mut text = time_to_string(worm.timer).to_vec();
        text.resize(len, 0);
        self.text(scr, &text, 106 * scale + 84 * worm.index * scale, s.render_res_y - 39, color);
    }

    /// The Holdazone zone's dashed outline.
    unsafe fn draw_zone(&self, scr: &LieroBitmap, offs: LieroIVec2) {
        let z = &self.s.holdazone;
        let timing_out = z.timeout_left < 70 * 4;
        let color = if timing_out { 168 } else { 50 };
        let contender_color = match self.worm(z.contender_idx) {
            Some(c) if timing_out => minimap_color(c.index),
            Some(c) => WORM_COLOUR_INDEXES[c.index as usize & 1] + 5,
            None => color,
        };
        liero_gfx_draw_dashed_line_box(
            scr,
            z.rect.x1 + offs.x,
            z.rect.y1 + offs.y,
            color as u8,
            contender_color as u8,
            z.contender_frames,
            self.s.zone_capture_time,
            z.rect.x2 - z.rect.x1,
            z.rect.y2 - z.rect.y1,
            self.s.cycles / 10,
        );
    }

    /// The worm's flickering outline where it will spawn, while Change is held
    /// before it has spawned.
    unsafe fn draw_spawn_point(&self, scr: &LieroBitmap, worm: &LieroViewWorm, offs: LieroIVec2) {
        if self.s.allow_viewing_spawn_point && worm.pressed_change {
            let set = &self.s.worm_sprites;
            let frame = worm.current_frame + worm.direction * 7 * 3 + worm.index * 2 * 7 * 3;
            if let Some(mem) = set.sprite(frame) {
                let x = ftoi(worm.pos.x) - 7 + offs.x;
                let y = ftoi(worm.pos.y) - 5 + offs.y;
                liero_gfx_blit_image_trans(scr, mem, x, y, set.width, set.height, set.width, self.s.cycles);
            }
        }
    }

    /// Bonuses, special objects, weapon objects and non-weapon objects.
    unsafe fn draw_objects(&self, scr: &LieroBitmap, offs: LieroIVec2) {
        let s = self.s;
        let small = &s.small_sprites;
        let large = &s.large_sprites;

        for b in slice(s.bonuses, s.bonus_count) {
            if b.timer > s.bonus_flicker_time || (s.cycles & 3) == 0 {
                let f = s.bonus_frames[b.frame as usize & 1];
                self.blit(scr, small, f, ftoi(b.x) - 3 + offs.x, ftoi(b.y) - 3 + offs.y);
                if s.shadow {
                    self.shadow(scr, small, f, ftoi(b.x) - 5 + offs.x, ftoi(b.y) - 1 + offs.y, 7, 7);
                }
                if s.names_on_bonuses && b.frame == 0 {
                    self.weapon_name(scr, b.weapon, ftoi(b.x) + offs.x, ftoi(b.y) - 10 + offs.y);
                }
            }
        }

        for o in slice(s.sobjects, s.sobject_count) {
            let frame = o.cur_frame + o.start_frame;
            if let Some(mem) = large.sprite(frame) {
                liero_gfx_blit_image_r(scr, mem, o.x + offs.x, o.y + offs.y, 16, 16);
                if s.shadow {
                    liero_gfx_blit_shadow_image(scr, s.materials, mem, o.x + offs.x - 3, o.y + offs.y + 3, 16, 16);
                }
            }
        }

        for o in slice(s.wobjects, s.wobject_count) {
            let Some(w) = self.weapon(o.weapon) else { continue };
            if w.start_frame > -1 {
                let mut cur_frame = o.cur_frame;
                if w.shot_type == 2 {
                    cur_frame = (cur_frame + 4) >> 3;
                    if cur_frame < 0 {
                        cur_frame = 16;
                    } else if cur_frame > 15 {
                        cur_frame -= 16;
                    }
                } else if w.shot_type == 3 {
                    if cur_frame > 64 {
                        cur_frame -= 1;
                    }
                    cur_frame = (cur_frame - 12) >> 3;
                    cur_frame = cur_frame.clamp(0, 12);
                }

                let x = ftoi(o.pos.x) - 3;
                let y = ftoi(o.pos.y) - 3;
                let frame = w.start_frame + cur_frame;
                if s.shadow && w.shadow {
                    self.shadow(scr, small, frame, x - 3 + offs.x, y + 3 + offs.y, 7, 7);
                }
                self.blit(scr, small, frame, x + offs.x, y + offs.y);
            } else if o.cur_frame > 0 {
                let x = ftoi(o.pos.x) + offs.x;
                let y = ftoi(o.pos.y) + offs.y;
                self.pixel(scr, x, y, o.cur_frame);
                if s.shadow {
                    self.shadow_pixel(scr, x - 3, y + 3);
                }
            }

            // TODO in C++: read from the EXE.
            if !s.rem_exp && o.weapon == 34 && s.names_on_bonuses && o.cur_frame == 0 && s.weapon_count > 0 {
                let name = o.slot % s.weapon_count;
                self.weapon_name(scr, name, ftoi(o.pos.x) + offs.x, ftoi(o.pos.y) - 10 + offs.y);
            }
        }

        for o in slice(s.nobjects, s.nobject_count) {
            if o.start_frame > 0 {
                let x = ftoi(o.pos.x) - 3;
                let y = ftoi(o.pos.y) - 3;
                let frame = o.start_frame + o.cur_frame;
                if s.shadow {
                    self.shadow(scr, small, frame, x - 3 + offs.x, y + 3 + offs.y, 7, 7);
                }
                self.blit(scr, small, frame, x + offs.x, y + offs.y);
            } else if o.cur_frame > 1 {
                let x = ftoi(o.pos.x) + offs.x;
                let y = ftoi(o.pos.y) + offs.y;
                self.pixel(scr, x, y, o.cur_frame);
                if s.shadow {
                    self.shadow_pixel(scr, x - 3, y + 3);
                }
            }
        }
    }

    /// Every worm with its laser sight, rope and fire cone, and the AIs'
    /// debug output.
    unsafe fn draw_worms(&self, scr: &LieroBitmap, offs: LieroIVec2, rand: &mut LieroRand) {
        let s = self.s;
        for w in self.worms.iter() {
            if w.visible {
                let x = ftoi(w.pos.x) - 7 + offs.x;
                let y = ftoi(w.pos.y) - 5 + offs.y;
                let angle_frame = liero_worm_angle_frame(w.aiming_angle, w.direction);
                let ww = &w.weapons[w.current_weapon as usize % 5];
                let weapon = self.weapon(ww.weapon);

                if ww.loading_left == 0 {
                    if let Some(weapon) = weapon {
                        let hx = w.hotspot_x + offs.x;
                        let hy = w.hotspot_y + offs.y;
                        if weapon.laser_sight {
                            liero_gfx_draw_laser_sight(scr, rand, hx, hy, x + 7, y + 4);
                        }
                        if ww.weapon == s.laser_weapon - 1 && w.pressed_fire {
                            liero_gfx_draw_line(scr, hx, hy, x + 7, y + 4, weapon.color_bullets as u8);
                        }
                    }
                }

                if w.rope_out {
                    let rx = ftoi(w.rope_pos.x) + offs.x;
                    let ry = ftoi(w.rope_pos.y) + offs.y;
                    liero_gfx_draw_ninjarope(scr, s.nr_colour_begin, s.nr_colour_end, rx, ry, x + 7, y + 4);
                    self.blit(scr, &s.large_sprites, 84, rx - 1, ry - 1);
                    if s.shadow {
                        liero_gfx_draw_shadow_line(scr, s.materials, rx - 3, ry + 3, x + 7 - 3, y + 4 + 3);
                        self.shadow(scr, &s.large_sprites, 84, rx - 4, ry + 2, 16, 16);
                    }
                }

                if weapon.is_some_and(|w| w.fire_cone > 0) && w.fire_cone > 0 {
                    let dir = w.direction as usize & 1;
                    let af = angle_frame.clamp(0, 6) as usize;
                    if let Some(mem) = s.fire_cone_sprites.sprite(angle_frame + w.direction * 7) {
                        let [ox, oy] = FIRE_CONE_OFFSET[dir][af];
                        liero_gfx_blit_fire_cone(scr, w.fire_cone / 2, mem, ox + x, oy + y);
                    }
                }

                let frame = w.current_frame + w.direction * 7 * 3 + w.index * 2 * 7 * 3;
                self.blit(scr, &s.worm_sprites, frame, x, y);
                if s.shadow {
                    self.shadow(scr, &s.worm_sprites, frame, x - 3, y + 3, 16, 16);
                }
            }

            draw_traces(scr, slice(w.traces, w.trace_count as i32), offs);
        }
    }

    /// The worm's sight, and its weapon's name while Change is held.
    unsafe fn draw_sight(&self, scr: &LieroBitmap, worm: &LieroViewWorm, offs: LieroIVec2) {
        if !worm.visible {
            return;
        }
        let cs = cos_sin(ftoi(worm.aiming_angle) & 127);
        let x = ftoi(worm.pos.x) - 1 + ftoi(cs.x.wrapping_mul(16)) + offs.x;
        let y = ftoi(worm.pos.y) - 2 + ftoi(cs.y.wrapping_mul(16)) + offs.y;
        self.blit(scr, &self.s.small_sprites, if worm.make_sight_green { 44 } else { 43 }, x, y);

        if worm.pressed_change {
            let ww = &worm.weapons[worm.current_weapon as usize % 5];
            self.weapon_name(scr, ww.weapon, ftoi(worm.pos.x) + 1 + offs.x, ftoi(worm.pos.y) - 10 + offs.y);
        }
    }

    unsafe fn draw_bobjects(&self, scr: &LieroBitmap, offs: LieroIVec2) {
        for b in slice(self.s.bobjects, self.s.bobject_count) {
            let x = ftoi(b.pos.x) + offs.x;
            let y = ftoi(b.pos.y) + offs.y;
            self.pixel(scr, x, y, b.color);
            if self.s.shadow {
                self.shadow_pixel(scr, x - 3, y + 3);
            }
        }
    }

    unsafe fn draw_level(&self, scr: &LieroBitmap, offs: LieroIVec2) {
        let l = &self.s.level;
        liero_gfx_blit_image_no_key_colour(scr, l.data, offs.x, offs.y, l.width, l.height, l.width);
    }

    /// The split-screen viewport: `Viewport::draw`.
    unsafe fn draw_worm_view(&self, vp: &mut LieroViewport, outer: &LieroBitmap, self_index: i32) {
        let s = self.s;
        let scr = outer;
        let Some(worm) = self.worm(vp.worm_idx) else { return };
        let multiplier = s.render_res_x / 320;
        let center_x = s.render_res_x / 2;
        let stats_x = worm.stats_x * multiplier;

        self.draw_stats(scr, worm, stats_x, 164 * multiplier);
        if s.is_replay {
            self.draw_name_and_time(scr, worm, stats_x, stats_x, (worm.stats_x + 1) * multiplier, 95 * multiplier);
        }
        self.draw_game_mode(scr, vp.worm_idx, stats_x, multiplier);

        let r = vp.rect;
        let view = LieroBitmap { clip: r, ..*scr };
        let scr = &view;
        let offs = LieroIVec2 { x: r.x1 - vp.x, y: r.y1 - vp.y };

        self.draw_level(scr, offs);
        if s.game_mode == LIERO_GAME_HOLDAZONE {
            self.draw_zone(scr, offs);
        }

        if !worm.visible && worm.killed_timer <= 0 && !worm.ready {
            let cx = (r.x1 + r.x2) / 2;
            self.text(scr, s.press_fire.bytes(), cx - 30, 76, 0);
            self.text(scr, s.press_fire.bytes(), cx - 31, 75, 50);
            self.draw_spawn_point(scr, worm, offs);
        }

        if vp.banner_y > -8 && worm.health <= 0 && s.game_mode == LIERO_GAME_GAME_OF_TAG && s.got_changed {
            self.banner(scr, s.youre_it.bytes(), r.x1 + 2, vp.banner_y);
        }

        for (i, v) in slice(s.viewports, s.viewport_count).iter().enumerate() {
            let Some(other) = self.worm(v.worm_idx) else { continue };
            if i as i32 != self_index && other.health <= 0 && v.banner_y > -8 {
                let mut msg = Vec::new();
                if other.last_killed_by_idx == worm.index {
                    msg.extend_from_slice(s.killed_msg.bytes());
                    msg.extend_from_slice(other.name.bytes());
                } else {
                    msg.extend_from_slice(other.name.bytes());
                    msg.extend_from_slice(s.committed_suicide_msg.bytes());
                }
                self.banner(scr, &msg, r.x1 + 2, v.banner_y);
            }
        }

        self.draw_objects(scr, offs);
        self.draw_worms(scr, offs, &mut vp.rand);
        self.draw_sight(scr, worm, offs);
        self.draw_bobjects(scr, offs);

        if s.map {
            self.draw_minimap(outer, center_x);
        }
    }

    /// The minimap, drawn after the viewport outside its clip rectangle.
    unsafe fn draw_minimap(&self, scr: &LieroBitmap, center_x: i32) {
        let s = self.s;
        let map_x = center_x - 26;
        let map_y = s.render_res_y - 38;
        liero_level_draw_miniature(&s.level, scr.pixels, scr.pitch, map_x, map_y, 10);

        for w in self.worms {
            if w.visible {
                self.raw_pixel(scr, ftoi(w.pos.x) / 10 + map_x, ftoi(w.pos.y) / 10 + map_y, minimap_color(w.index));
            }
        }

        let z = &s.holdazone;
        if s.game_mode == LIERO_GAME_HOLDAZONE && z.timeout_left > 0 {
            let x = (z.rect.x1 + z.rect.x2) / 2 / 10 + map_x;
            let y = (z.rect.y1 + z.rect.y2) / 2 / 10 + map_y;
            let color = self.worm(z.holder_idx).map_or(168, |h| minimap_color(h.index));
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                self.pixel(scr, x + dx, y + dy, color);
            }
        }
    }

    /// The spectator viewport: `SpectatorViewport::draw`.
    unsafe fn draw_spectator_view(&self, vp: &mut LieroViewport, scr: &LieroBitmap) {
        let s = self.s;
        let multiplier = s.render_res_x / 320;
        let center_x = s.render_res_x / 2;
        let r = vp.rect;
        let offs = LieroIVec2 { x: r.x1 - vp.x, y: r.y1 - vp.y };
        let res_y = s.render_res_y;

        for (i, worm) in self.worms.iter().enumerate() {
            // C++: "fix misalignment. Not sure why this is needed".
            let offset_x = offs.x / (i as i32 + 1) + if i == 1 { 2 } else { 0 };
            let list_x = center_x - 15 + if i == 0 { -90 } else { 60 };
            let x = offset_x + worm.stats_x * multiplier;

            self.draw_stats(scr, worm, x, 164);
            self.draw_name_and_time(scr, worm, x, x - 1, x, center_x - 15);

            if s.state_game {
                for (j, ww) in worm.weapons.iter().enumerate() {
                    let y = res_y - 40 + j as i32 * 8;
                    let weapon = self.weapon(ww.weapon);
                    if ww.ammo > 0 {
                        let width = div(ww.ammo.wrapping_mul(60), weapon.map_or(0, |w| w.ammo));
                        if width > 0 {
                            liero_gfx_draw_bar(scr, list_x, y, width, 5, (width / 6 + 245) as u8);
                        }
                    }
                    let color = if worm.current_weapon == j as i32 { 187 } else { 185 };
                    if let Some(w) = weapon {
                        self.text(scr, w.name.bytes(), list_x, y, color);
                    }
                }
            }

            self.draw_game_mode(scr, i as i32, x, 1);
        }

        self.draw_level(scr, offs);
        if s.game_mode == LIERO_GAME_HOLDAZONE {
            self.draw_zone(scr, offs);
        }

        for worm in self.worms {
            if !worm.visible && worm.killed_timer <= 0 && !worm.ready {
                self.draw_spawn_point(scr, worm, offs);
            }
            if vp.banner_y > -8 && worm.health <= 0 && s.game_mode == LIERO_GAME_GAME_OF_TAG && s.got_changed {
                self.banner(scr, s.youre_it.bytes(), r.x1 + 2, vp.banner_y);
            }
        }

        for worm in self.worms {
            if worm.health <= 0 && vp.banner_y > -8 {
                let mut msg = Vec::new();
                if worm.last_killed_by_idx == worm.index {
                    msg.extend_from_slice(worm.name.bytes());
                    msg.extend_from_slice(s.committed_suicide_msg.bytes());
                } else {
                    // C++ looks the killer up by position.
                    if let Some(killer) = self.worm(worm.last_killed_by_idx) {
                        msg.extend_from_slice(killer.name.bytes());
                    }
                    msg.extend_from_slice(b" killed ");
                    msg.extend_from_slice(worm.name.bytes());
                }
                self.banner(scr, &msg, r.x1 + 2, vp.banner_y);
            }
        }

        self.draw_objects(scr, offs);
        self.draw_worms(scr, offs, &mut vp.rand);
        for worm in self.worms {
            self.draw_sight(scr, worm, offs);
        }
        self.draw_bobjects(scr, offs);
    }
}

impl LieroViewport {
    /// C++'s `Viewport` constructor: a viewport at `rect` on screen,
    /// following worm `worm_idx` around a level of `level_width` x
    /// `level_height`.
    pub fn new(rect: LieroRect, worm_idx: i32, level_width: i32, level_height: i32) -> Self {
        let (w, h) = (rect.x2 - rect.x1, rect.y2 - rect.y1);
        LieroViewport {
            x: 0,
            y: 0,
            shake: 0,
            max_x: level_width - w,
            max_y: level_height - h,
            center_x: w >> 1,
            center_y: h >> 1,
            worm_idx,
            banner_y: -8,
            rect,
            rand: LieroRand::default(),
        }
    }

    fn set_center(&mut self, x: i32, y: i32) {
        self.x = x - self.center_x;
        self.y = y - self.center_y;
    }

    /// Moves at most one pixel each way per step: `scrollTo`.
    fn scroll_to(&mut self, dest_x: i32, dest_y: i32, steps: i32) {
        for _ in 0..steps {
            let (tx, ty) = (dest_x - self.center_x, dest_y - self.center_y);
            self.x += (self.x < tx) as i32 - (self.x > tx) as i32;
            self.y += (self.y < ty) as i32 - (self.y > ty) as i32;
        }
    }

    /// Shakes the camera and keeps it inside the level.
    unsafe fn shake_and_clamp(&mut self) {
        let real_shake = ftoi(self.shake);
        if real_shake > 0 {
            let rand = &mut self.rand;
            self.x += rand.below((real_shake * 2) as u32) as i32 - real_shake;
            self.y += rand.below((real_shake * 2) as u32) as i32 - real_shake;
        }
        self.x = self.x.max(0).min(self.max_x);
        self.y = self.y.max(0).min(self.max_y);
    }
}

/// See `LieroViewport::new`.
#[no_mangle]
pub extern "C" fn liero_viewport_new(rect: LieroRect, worm_idx: i32, level_width: i32, level_height: i32) -> *mut LieroViewport {
    Box::into_raw(Box::new(LieroViewport::new(rect, worm_idx, level_width, level_height)))
}

/// # Safety
/// `vp` is valid.
#[no_mangle]
pub unsafe extern "C" fn liero_viewport_clone(vp: &LieroViewport) -> *mut LieroViewport {
    Box::into_raw(Box::new(*vp))
}

/// # Safety
/// `vp` comes from `liero_viewport_new` or `_clone`, and isn't used after.
#[no_mangle]
pub unsafe extern "C" fn liero_viewport_free(vp: *mut LieroViewport) {
    if !vp.is_null() {
        drop(Box::from_raw(vp));
    }
}

/// Moves the camera one tick: `Viewport::process` for `LIERO_VIEWPORT_WORM`,
/// `SpectatorViewport::process` for `LIERO_VIEWPORT_SPECTATOR`. `worms` has
/// `worm_count` entries in the order of `Game::worms`.
///
/// # Safety
/// `worms` must hold `worm_count` worms.
#[no_mangle]
pub unsafe extern "C" fn liero_viewport_process(
    vp: &mut LieroViewport,
    kind: i32,
    worms: *const LieroViewWorm,
    worm_count: i32,
) {
    let worms = slice(worms, worm_count);
    let worm = |i: i32| usize::try_from(i).ok().and_then(|i| worms.get(i));

    if kind == LIERO_VIEWPORT_SPECTATOR {
        // C++: "FIXME this is a bit broken". 150 is where killedTimer starts.
        if [0, 1].iter().any(|&i| worm(i).is_some_and(|w| w.killed_timer == 150)) {
            vp.banner_y = -8;
        }
    } else if let Some(w) = worm(vp.worm_idx) {
        let (x, y) = (ftoi(w.pos.x), ftoi(w.pos.y));
        if w.killed_timer <= 0 {
            if !w.visible {
                vp.scroll_to(x, y, 4);
            } else if w.steerable_count > 0 {
                vp.set_center(w.steerable_sum_x / w.steerable_count, w.steerable_sum_y / w.steerable_count);
            } else {
                vp.set_center(x, y);
            }
        } else if w.health <= 0 {
            vp.set_center(x, y);
            if w.killed_timer == 150 {
                vp.banner_y = -8;
            }
        }
    }

    vp.shake_and_clamp();
}

/// The positions a predictive bot's plans went through
/// (`FollowAI::drawDebug`), with `Bitmap::setPixel`'s clipping.
unsafe fn draw_traces(scr: &LieroBitmap, traces: &[LieroAiTrace], offs: LieroIVec2) {
    let clip = scr.clip;
    for t in traces {
        let (x, y) = (ftoi(t.x).wrapping_add(offs.x), ftoi(t.y).wrapping_add(offs.y));
        let (dx, dy) = (x.wrapping_sub(clip.x1), y.wrapping_sub(clip.y1));
        if dx >= 0 && dx < clip.x2.wrapping_sub(clip.x1) && dy >= 0 && dy < clip.y2.wrapping_sub(clip.y1) {
            *scr.pixels.offset(y as isize * scr.pitch as isize + x as isize) = t.color;
        }
    }
}

/// Draws a viewport on `scr`: `Viewport::draw` for `LIERO_VIEWPORT_WORM`,
/// `SpectatorViewport::draw` for `LIERO_VIEWPORT_SPECTATOR`. `self_index` is
/// a split-screen viewport's position in `scene.viewports`.
///
/// # Safety
/// See `LieroBitmap` and `LieroScene`: every pointer must be valid for its
/// count.
#[no_mangle]
pub unsafe extern "C" fn liero_viewport_draw(
    vp: &mut LieroViewport,
    kind: i32,
    self_index: i32,
    scene: &LieroScene,
    scr: &LieroBitmap,
) {
    let scene = Scene::new(scene);
    if kind == LIERO_VIEWPORT_SPECTATOR {
        scene.draw_spectator_view(vp, scr);
    } else {
        scene.draw_worm_view(vp, scr, self_index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rand::LIERO_RAND_DEFAULT_SEED;

    fn s(text: &'static [u8]) -> LieroStr {
        LieroStr { ptr: text.as_ptr(), len: text.len() }
    }

    fn worm(index: i32) -> LieroViewWorm {
        LieroViewWorm {
            pos: LieroIVec2 { x: 100 << 16, y: 80 << 16 },
            visible: true,
            ready: true,
            health: 100,
            max_health: 100,
            killed_timer: 0,
            stats_x: 0,
            kills: 0,
            lives: 3,
            timer: 0,
            index,
            name: s(b"W"),
            color: 9,
            pressed_change: false,
            pressed_fire: false,
            current_frame: 0,
            direction: 0,
            hotspot_x: 0,
            hotspot_y: 0,
            aiming_angle: 32 << 16,
            make_sight_green: false,
            rope_out: false,
            rope_pos: LieroIVec2::default(),
            fire_cone: 0,
            last_killed_by_idx: -1,
            current_weapon: 0,
            weapons: [LieroViewWormWeapon { weapon: 0, ammo: 0, loading_left: 0 }; 5],
            steerable_count: 0,
            steerable_sum_x: 0,
            steerable_sum_y: 0,
            traces: std::ptr::null(),
            trace_count: 0,
        }
    }

    fn viewport() -> LieroViewport {
        LieroViewport::new(LieroRect { x1: 0, y1: 0, x2: 158, y2: 158 }, 0, 504, 350)
    }

    #[test]
    fn new_viewport_matches_cpp_and_copies_are_separate() {
        let vp = viewport();
        // What C++'s Viewport constructor set.
        assert_eq!((vp.max_x, vp.max_y, vp.center_x, vp.center_y), (504 - 158, 350 - 158, 79, 79));
        assert_eq!((vp.x, vp.y, vp.shake, vp.banner_y), (0, 0, 0, -8));
        assert_eq!(vp.rand, LieroRand::new(LIERO_RAND_DEFAULT_SEED));
        unsafe {
            let a = liero_viewport_new(LieroRect { x1: 160, y1: 0, x2: 318, y2: 158 }, 1, 504, 350);
            (*a).x = 12;
            (*a).shake = 3;
            (*a).rand.next_u32();
            let c = liero_viewport_clone(&*a);
            // Every field is copied.
            assert_eq!(format!("{:?}", *c), format!("{:?}", *a));
            (*c).x = 1;
            assert_eq!((*a).x, 12);
            liero_viewport_free(c);
            liero_viewport_free(a);
        }
    }

    fn process(vp: &mut LieroViewport, kind: i32, worms: &[LieroViewWorm]) {
        unsafe { liero_viewport_process(vp, kind, worms.as_ptr(), worms.len() as i32) }
    }

    #[test]
    fn worm_viewport_follows_its_worm() {
        let mut vp = viewport();
        let mut w = worm(0);

        process(&mut vp, LIERO_VIEWPORT_WORM, &[w]);
        assert_eq!((vp.x, vp.y), (100 - 79, 80 - 79));

        // Steerables are followed instead of the worm.
        w.steerable_count = 2;
        w.steerable_sum_x = 2 * 300;
        w.steerable_sum_y = 2 * 200;
        process(&mut vp, LIERO_VIEWPORT_WORM, &[w]);
        assert_eq!((vp.x, vp.y), (300 - 79, 200 - 79));

        // Before spawning, the camera scrolls 4 pixels a tick.
        w.visible = false;
        process(&mut vp, LIERO_VIEWPORT_WORM, &[w]);
        assert_eq!((vp.x, vp.y), (300 - 79 - 4, 200 - 79 - 4));

        // Kept inside the level.
        w.visible = true;
        w.steerable_count = 0;
        w.pos = LieroIVec2 { x: -5 << 16, y: 1000 << 16 };
        process(&mut vp, LIERO_VIEWPORT_WORM, &[w]);
        assert_eq!((vp.x, vp.y), (0, vp.max_y));

        // Dying shows the kill banner.
        w.health = 0;
        w.killed_timer = 150;
        vp.banner_y = 2;
        process(&mut vp, LIERO_VIEWPORT_WORM, &[w]);
        assert_eq!(vp.banner_y, -8);
    }

    #[test]
    fn traces_are_clipped() {
        let mut screen = vec![0u8; 10 * 10];
        let scr = LieroBitmap {
            pixels: screen.as_mut_ptr(),
            w: 10,
            h: 10,
            pitch: 10,
            clip: LieroRect { x1: 2, y1: 3, x2: 6, y2: 7 },
        };
        let t = |x: i32, y: i32| LieroAiTrace { x: x << 16, y: y << 16, color: 9 };
        let traces = [t(0, 4), t(1, 3), t(2, 3), t(5, 6), t(6, 6), t(5, 7), t(4, 2)];
        unsafe { draw_traces(&scr, &traces, LieroIVec2 { x: 1, y: 0 }) };
        let set: Vec<(usize, usize)> = (0..100).filter(|&i| screen[i] == 9).map(|i| (i % 10, i / 10)).collect();
        assert_eq!(set, vec![(2, 3), (3, 3)]);
    }

    #[test]
    fn spectator_viewport_only_shakes() {
        let mut vp = viewport();
        vp.x = 50;
        vp.y = 60;
        let worms = [worm(0), worm(1)];
        process(&mut vp, LIERO_VIEWPORT_SPECTATOR, &worms);
        assert_eq!((vp.x, vp.y), (50, 60));

        vp.shake = 3 << 16;
        let mut expect = LieroRand::new(LIERO_RAND_DEFAULT_SEED);
        let dx = expect.below(6) as i32 - 3;
        let dy = expect.below(6) as i32 - 3;
        process(&mut vp, LIERO_VIEWPORT_SPECTATOR, &worms);
        assert_eq!((vp.x, vp.y), (50 + dx, 60 + dy));

        let mut worms = worms;
        worms[1].killed_timer = 150;
        vp.banner_y = 0;
        process(&mut vp, LIERO_VIEWPORT_SPECTATOR, &worms);
        assert_eq!(vp.banner_y, -8);
    }

    /// A scene to draw on a 320 x 200 screen: a level of colour 1, 7 x 7
    /// small sprites of colour 100 + frame, 4 x 4 text sprites of colour
    /// 200 + letter, worm sprites of colour 50 and an empty font.
    struct TestScene {
        level: Vec<u8>,
        materials: Vec<LieroMaterial>,
        small: Vec<u8>,
        text: Vec<u8>,
        worm_sprites: Vec<u8>,
        font: Vec<LieroFontChar>,
        weapons: Vec<LieroViewWeapon>,
        worms: Vec<LieroViewWorm>,
        bonuses: Vec<LieroViewBonus>,
        wobjects: Vec<LieroViewWObject>,
        screen: Vec<u8>,
    }

    impl TestScene {
        fn new() -> Self {
            let sprites = |w: usize, n: usize, base: u8| (0..n).flat_map(|i| vec![base + i as u8; w * w]).collect();
            let weapon = |name: &'static [u8]| LieroViewWeapon {
                name: s(name),
                ammo: 10,
                loading_time: 0,
                computed_loading_time: 1,
                laser_sight: false,
                color_bullets: 0,
                fire_cone: 0,
                start_frame: -1,
                shot_type: 0,
                shadow: false,
            };
            TestScene {
                level: vec![1; 504 * 350],
                materials: vec![LieroMaterial::default(); 256],
                small: sprites(7, 50, 100),
                text: sprites(4, 26, 200),
                worm_sprites: vec![50; 16 * 16 * 84],
                font: vec![LieroFontChar { data: [0; 56], width: 8 }; 250],
                weapons: (0..40).map(|i| weapon(if i % 2 == 0 { b"AB" } else { b"C" })).collect(),
                worms: vec![worm(0), worm(1)],
                bonuses: Vec::new(),
                wobjects: Vec::new(),
                screen: vec![0; 320 * 200],
            }
        }

        fn draw(&mut self, kind: i32, f: impl FnOnce(&mut LieroScene)) {
            let set = |data: &Vec<u8>, w: i32| LieroSpriteSet {
                data: data.as_ptr(),
                width: w,
                height: w,
                count: (data.len() / (w * w) as usize) as i32,
            };
            use std::ptr::null;
            let mut scene = LieroScene {
                worms: self.worms.as_ptr(),
                worm_count: self.worms.len() as i32,
                weapons: self.weapons.as_ptr(),
                weapon_count: self.weapons.len() as i32,
                bonuses: self.bonuses.as_ptr(),
                bonus_count: self.bonuses.len() as i32,
                sobjects: null(),
                sobject_count: 0,
                wobjects: self.wobjects.as_ptr(),
                wobject_count: self.wobjects.len() as i32,
                nobjects: null(),
                nobject_count: 0,
                bobjects: null(),
                bobject_count: 0,
                viewports: null(),
                viewport_count: 0,
                small_sprites: set(&self.small, 7),
                large_sprites: set(&self.worm_sprites, 16),
                worm_sprites: set(&self.worm_sprites, 16),
                fire_cone_sprites: set(&self.worm_sprites, 16),
                text_sprites: set(&self.text, 4),
                font: self.font.as_ptr(),
                font_count: self.font.len(),
                materials: self.materials.as_ptr(),
                level: LieroLevel {
                    data: self.level.as_mut_ptr(),
                    materials: std::ptr::null_mut(),
                    width: 504,
                    height: 350,
                },
                bonus_frames: [0, 1],
                reloading: s(b""),
                kills: s(b""),
                lives: s(b""),
                press_fire: s(b""),
                youre_it: s(b""),
                killed_msg: s(b""),
                committed_suicide_msg: s(b""),
                cycles: 1,
                game_mode: LIERO_GAME_KILL_EM_ALL,
                holdazone: LieroHoldazone::default(),
                zone_capture_time: 1,
                last_killed_idx: -1,
                got_changed: false,
                shadow: false,
                names_on_bonuses: false,
                allow_viewing_spawn_point: false,
                map: false,
                rem_exp: false,
                laser_weapon: 0,
                bonus_flicker_time: 0,
                nr_colour_begin: 0,
                nr_colour_end: 1,
                render_res_x: 320,
                render_res_y: 200,
                state_game: true,
                is_replay: false,
            };
            f(&mut scene);

            let mut vp = viewport();
            let scr = LieroBitmap {
                pixels: self.screen.as_mut_ptr(),
                w: 320,
                h: 200,
                pitch: 320,
                clip: LieroRect { x1: 0, y1: 0, x2: 320, y2: 200 },
            };
            unsafe { liero_viewport_draw(&mut vp, kind, 0, &scene, &scr) };
        }

        fn px(&self, x: i32, y: i32) -> u8 {
            self.screen[(y * 320 + x) as usize]
        }
    }

    #[test]
    fn bonus_names_are_drawn_in_small_letters() {
        let mut t = TestScene::new();
        t.bonuses.push(LieroViewBonus { x: 60 << 16, y: 60 << 16, timer: 100, frame: 0, weapon: 0 });
        t.draw(LIERO_VIEWPORT_WORM, |_| {});
        assert_eq!(t.px(57, 57), 100);
        assert_eq!(t.px(56, 50), 1, "no name unless names on bonuses");

        t.draw(LIERO_VIEWPORT_WORM, |s| s.names_on_bonuses = true);
        // "AB" is 8 pixels wide, centred on x = 60, 10 pixels above.
        assert_eq!(t.px(56, 50), 200);
        assert_eq!(t.px(60, 50), 201);
    }

    #[test]
    fn weapon_34_shows_a_name_picked_by_its_slot() {
        let mut t = TestScene::new();
        t.wobjects.push(LieroViewWObject { pos: LieroIVec2 { x: 60 << 16, y: 60 << 16 }, weapon: 34, cur_frame: 0, slot: 41 });
        t.draw(LIERO_VIEWPORT_WORM, |s| s.names_on_bonuses = true);
        // Slot 41 of 40 weapons names weapon 1, "C", drawn from x = 60 - 2.
        assert_eq!(t.px(58, 50), 202);

        t.draw(LIERO_VIEWPORT_SPECTATOR, |s| {
            s.names_on_bonuses = true;
            s.rem_exp = true;
        });
        assert_eq!(t.px(58, 50), 1, "the hack is off");
    }

    #[test]
    fn spawn_point_shows_while_change_is_held() {
        let mut t = TestScene::new();
        t.worms[0].visible = false;
        t.worms[0].ready = false;
        t.worms[0].pressed_change = true;
        t.worms[1].visible = false;
        t.draw(LIERO_VIEWPORT_WORM, |_| {});
        assert_eq!((t.px(93, 75), t.px(94, 75)), (1, 1), "not allowed");

        t.draw(LIERO_VIEWPORT_WORM, |s| s.allow_viewing_spawn_point = true);
        // Every other pixel of the sprite at (100 - 7, 80 - 5), by cycles 1.
        assert_eq!((t.px(93, 75), t.px(94, 75)), (50, 1));
        assert_eq!((t.px(93, 76), t.px(94, 76)), (1, 50));
    }

    #[test]
    fn replays_show_name_colour_and_time() {
        let mut t = TestScene::new();
        t.worms[0].stats_x = 10;
        t.draw(LIERO_VIEWPORT_WORM, |_| {});
        assert_eq!(t.px(10, 192), 0);

        t.draw(LIERO_VIEWPORT_WORM, |s| s.is_replay = true);
        assert_eq!(t.px(10, 192), 7);
        assert_eq!(t.px(11, 193), 9);
        assert_eq!(t.px(17, 199), 7);
    }

    #[test]
    fn timer_length_follows_the_cpp_call() {
        // drawText got 5 * multiplier as the length; past the five characters
        // only line breaks are drawn.
        let mut t = TestScene::new();
        t.font = vec![LieroFontChar { data: [1; 56], width: 8 }; 250];
        t.worms[0].index = 0;
        t.worms[0].timer = 61;
        t.draw(LIERO_VIEWPORT_WORM, |s| s.game_mode = LIERO_GAME_GAME_OF_TAG);
        // Five 8-pixel characters from x = 106 at y = 200 - 39.
        assert_ne!(t.px(106 + 4 * 8 + 3, 161 + 3), 0);
        assert_eq!(t.px(106 + 5 * 8 + 3, 161 + 3), 0);
    }
}
