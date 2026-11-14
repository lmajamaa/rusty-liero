//! Game data, mirroring the fields of `Common` in `src/game/common.hpp` that
//! a TC folder holds.

#[derive(Default, Clone, PartialEq)]
pub struct Weapon {
    pub name: Vec<u8>,
    pub id_str: String,
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

#[derive(Default, Clone, PartialEq)]
pub struct NObject {
    pub id_str: String,
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

#[derive(Default, Clone, PartialEq)]
pub struct SObject {
    pub id_str: String,
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

#[derive(Default, Clone, Copy, PartialEq)]
pub struct Texture {
    pub n_draw_back: bool,
    pub m_frame: i32,
    pub s_frame: i32,
    pub r_frame: i32,
}

/// Square sprites stored one after another, each row-major.
#[derive(Default, PartialEq)]
pub struct SpriteSet {
    pub width: usize,
    pub height: usize,
    pub count: usize,
    pub data: Vec<u8>,
}

impl SpriteSet {
    /// Sprite `frame`'s pixels. Panics on a frame there isn't.
    pub fn sprite(&self, frame: usize) -> &[u8] {
        assert!(frame < self.count, "no sprite {frame}");
        let size = self.width * self.height;
        &self.data[frame * size..(frame + 1) * size]
    }

    pub fn sprite_mut(&mut self, frame: usize) -> &mut [u8] {
        let size = self.width * self.height;
        &mut self.data[frame * size..(frame + 1) * size]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontChar {
    /// 7x8 pixels, row-major. Read from a TC folder, set pixels are 8.
    pub data: [u8; 56],
    pub width: usize,
}

#[derive(PartialEq)]
pub struct Sound {
    pub name: String,
    /// Signed 8-bit samples, as stored in LIERO.SND. `None` when a TC's WAV
    /// file isn't 8-bit mono 22050 Hz: the game then plays nothing for it.
    pub data: Option<Vec<u8>>,
}

#[derive(PartialEq)]
pub struct Common {
    pub constants: Vec<(&'static str, i32)>,
    pub texts: Vec<(&'static str, Vec<u8>)>,
    pub hacks: Vec<(&'static str, bool)>,
    /// 6-bit VGA palette entries.
    pub palette: [[u8; 3]; 256],
    pub color_anim: [(i32, i32); 4],
    pub materials: [u8; 256],
    pub weapons: Vec<Weapon>,
    pub nobjects: Vec<NObject>,
    pub sobjects: Vec<SObject>,
    pub textures: [Texture; 9],
    /// Indexed [bonus][0 = timer, 1 = timerV].
    pub bonus_rand_timer: [[i32; 2]; 2],
    pub bonus_sobjects: [i32; 2],
    pub bonus_frames: [i32; 2],
    /// Indexed [0 = off, 1 = on][key].
    pub ai_params: [[i32; 7]; 2],
    pub large_sprites: SpriteSet,
    pub small_sprites: SpriteSet,
    pub text_sprites: SpriteSet,
    pub font: Vec<FontChar>,
    pub sounds: Vec<Sound>,
}

/// Lowercases ASCII letters and digits and turns everything else into `_`.
pub fn to_id(name: &[u8]) -> String {
    name.iter()
        .map(|&c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase() as char
            } else {
                '_'
            }
        })
        .collect()
}
