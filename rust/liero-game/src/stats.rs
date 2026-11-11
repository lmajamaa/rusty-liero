//! Game stats: what `NormalStatsRecorder` (`src/game/stats_recorder.cpp`)
//! collects during a game, and the stats screen drawn from it after the game
//! (`src/game/controller/stats_presenter.cpp`).
//!
//! C++'s `NormalStatsRecorder` owns a `LieroStats` and forwards each event,
//! with worms and weapons as indexes and the worms' state copied into
//! `LieroStatsWorm`s. The base `StatsRecorder`, which AI clones of a game use,
//! stays a C++ no-op. The SDL loop that scrolls the stats screen stays in C++
//! and calls `liero_stats_screen_draw` every frame.
//!
//! The counters are C++ `int`s, which wrap around here rather than being
//! undefined.

use crate::cpp_sort;
use crate::gfx::{liero_gfx_blit_image, liero_gfx_draw_graph, liero_gfx_draw_heatmap, liero_gfx_draw_rounded_box, LieroBitmap};
use crate::screen::{into_ffi, Host, LieroScreen, Screen, Tick};
use crate::text::{time_to_string, time_to_string_ex, LieroFont, LieroStr};

const WEAPON_COUNT: usize = 40;
const WORM_COLOUR_INDEXES: [u8; 2] = [0x58, 0x78];
/// `Settings::GMGameOfTag` and `Settings::GMHoldazone`.
const GAME_OF_TAG: i32 = 1;
const HOLDAZONE: i32 = 2;

/// Counts over the level, at half its resolution.
#[derive(Clone, Debug, PartialEq)]
pub struct Heatmap {
    pub width: i32,
    pub height: i32,
    pub org_width: i32,
    pub org_height: i32,
    pub map: Vec<i32>,
}

impl Heatmap {
    fn new() -> Self {
        let (width, height) = (504 / 2, 350 / 2);
        Heatmap { width, height, org_width: 504, org_height: 350, map: vec![0; (width * height) as usize] }
    }

    /// The cell for level position (x, y), clamped to the map.
    fn cell(&self, x: i32, y: i32) -> (i32, i32) {
        let x = (x.wrapping_mul(self.width) / self.org_width).max(0).min(self.width - 1);
        let y = (y.wrapping_mul(self.height) / self.org_height).max(0).min(self.height - 1);
        (x, y)
    }

    fn add(&mut self, x: i32, y: i32, v: i32) {
        let i = (y * self.width + x) as usize;
        self.map[i] = self.map[i].wrapping_add(v);
    }

    pub fn inc(&mut self, x: i32, y: i32) {
        let (x, y) = self.cell(x, y);
        self.add(x, y, 1);
    }

    /// Adds `v` around (x, y), weighted down towards the corners of a 5 x 5
    /// square.
    pub fn inc_area(&mut self, x: i32, y: i32, v: i32) {
        let (x, y) = self.cell(x, y);
        for y1 in -2..=2 {
            for x1 in -2..=2 {
                let (cx, cy) = (x + x1, y + y1);
                if cx >= 0 && cy >= 0 && cx < self.width && cy < self.height {
                    let weight = 4 * 4 - (x1 * y1) * (x1 * y1);
                    self.add(cx, cy, v.wrapping_mul(weight));
                }
            }
        }
    }

    fn view(&self) -> LieroHeatmapView {
        LieroHeatmapView {
            width: self.width,
            height: self.height,
            org_width: self.org_width,
            org_height: self.org_height,
            map: self.map.as_ptr(),
            len: self.map.len(),
        }
    }
}

/// One worm's results with one weapon.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LieroWeaponStats {
    /// Damage the weapon's shots could have done.
    pub potential_hp: i32,
    /// Damage done by each shot's first hit on the other worm.
    pub actual_hp: i32,
    pub potential_hits: i32,
    pub actual_hits: i32,
    /// All damage done to the other worm.
    pub total_hp: i32,
    /// The weapon's index in the TC.
    pub index: i32,
}

impl LieroWeaponStats {
    fn combine(&mut self, o: &LieroWeaponStats) {
        self.potential_hits = self.potential_hits.wrapping_add(o.potential_hits);
        self.potential_hp = self.potential_hp.wrapping_add(o.potential_hp);
        self.actual_hits = self.actual_hits.wrapping_add(o.actual_hits);
        self.actual_hp = self.actual_hp.wrapping_add(o.actual_hp);
        self.total_hp = self.total_hp.wrapping_add(o.total_hp);
    }
}

/// A worm's damage taken during a frame, and its health then, counting the
/// lives left.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LieroWormFrameStats {
    pub damage: i32,
    pub total_hp: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WormStats {
    /// The frames of each spawn and death.
    pub life_spans: Vec<[i32; 2]>,
    pub weapons: [LieroWeaponStats; WEAPON_COUNT],
    pub damage: i32,
    pub damage_dealt: i32,
    pub self_damage: i32,
    pub damage_hm: Heatmap,
    pub presence: Heatmap,
    pub weapon_change_good: i32,
    pub weapon_change_bad: i32,
    pub frame_stats: Vec<LieroWormFrameStats>,
    /// The frame of the last spawn, or -1 while dead.
    pub spawn_time: i32,
    pub index: i32,
    pub lives: i32,
    pub timer: i32,
    pub kills: i32,
    pub ai_process_time: u64,
}

impl WormStats {
    fn new(index: i32) -> Self {
        let mut weapons = [LieroWeaponStats::default(); WEAPON_COUNT];
        for (i, w) in weapons.iter_mut().enumerate() {
            w.index = i as i32;
        }
        WormStats {
            life_spans: Vec::new(),
            weapons,
            damage: 0,
            damage_dealt: 0,
            self_damage: 0,
            damage_hm: Heatmap::new(),
            presence: Heatmap::new(),
            weapon_change_good: 0,
            weapon_change_bad: 0,
            frame_stats: Vec::new(),
            spawn_time: -1,
            index,
            lives: 0,
            timer: 0,
            kills: 0,
            ai_process_time: 0,
        }
    }

    /// The shortest and longest life, or 0 and 0 with none.
    pub fn life_stats(&self) -> (i32, i32) {
        let mut lens = self.life_spans.iter().map(|l| l[1].wrapping_sub(l[0]));
        let Some(first) = lens.next() else { return (0, 0) };
        lens.fold((first, first), |(min, max), len| (min.min(len), max.max(len)))
    }
}

/// `NormalStatsRecorder`'s data.
#[derive(Clone, Debug, PartialEq)]
pub struct LieroStats {
    pub frame: i32,
    pub worms: [WormStats; 2],
    /// Clock ticks at the start of the frame, and summed over frames. Only the
    /// commented-out text report in the C++ used them.
    pub frame_start: u64,
    pub process_time_total: u64,
    pub game_time: i32,
    pub presence: Heatmap,
}

impl Default for LieroStats {
    fn default() -> Self {
        LieroStats {
            frame: 0,
            worms: [WormStats::new(0), WormStats::new(1)],
            frame_start: 0,
            process_time_total: 0,
            game_time: 0,
            presence: Heatmap::new(),
        }
    }
}

/// What the recorder reads from a worm, copied from C++'s `Worm`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LieroStatsWorm {
    pub index: i32,
    /// `ftoi(pos)`.
    pub x: i32,
    pub y: i32,
    pub visible: bool,
    pub health: i32,
    pub lives: i32,
    pub timer: i32,
    pub kills: i32,
    /// `settings->health`.
    pub max_health: i32,
    /// The fire, change, left and right controls.
    pub fire: bool,
    pub change: bool,
    pub left: bool,
    pub right: bool,
    pub current_weapon: i32,
    pub loading_left: [i32; 5],
}

impl LieroStats {
    fn weapon(&mut self, by: i32, weapon: i32) -> Option<&mut LieroWeaponStats> {
        if by < 0 || weapon < 0 {
            return None;
        }
        Some(&mut self.worms[by as usize].weapons[weapon as usize])
    }

    /// A shot or object that could do `hp` damage. `by` is the worm's index
    /// or -1, `weapon` the weapon's index or -1, as with the other events.
    pub fn damage_potential(&mut self, by: i32, weapon: i32, hp: i32) {
        if let Some(w) = self.weapon(by, weapon) {
            w.potential_hp = w.potential_hp.wrapping_add(hp);
        }
    }

    /// `hp` damage to worm `to` at (x, y). `has_hit` is whether the object
    /// had already hit a worm.
    #[allow(clippy::too_many_arguments)]
    pub fn damage_dealt(&mut self, by: i32, weapon: i32, to: i32, x: i32, y: i32, hp: i32, has_hit: bool) {
        let w = &mut self.worms[to as usize];
        w.damage = w.damage.wrapping_add(hp);
        // C++ read the last frame's stats unchecked; there always is one, as
        // every frame starts with `pre_tick`.
        if let Some(f) = w.frame_stats.last_mut() {
            f.damage = f.damage.wrapping_add(hp);
        }
        w.damage_hm.inc_area(x, y, hp);

        if by >= 0 {
            let b = &mut self.worms[by as usize];
            if by != to {
                b.damage_dealt = b.damage_dealt.wrapping_add(hp);
            } else {
                b.self_damage = b.self_damage.wrapping_add(hp);
            }
        }

        if by != to {
            if let Some(w) = self.weapon(by, weapon) {
                if !has_hit {
                    w.actual_hp = w.actual_hp.wrapping_add(hp);
                }
                w.total_hp = w.total_hp.wrapping_add(hp);
            }
        }
    }

    pub fn shot(&mut self, by: i32, weapon: i32) {
        if let Some(w) = self.weapon(by, weapon) {
            w.potential_hits = w.potential_hits.wrapping_add(1);
        }
    }

    pub fn hit(&mut self, by: i32, weapon: i32, to: i32) {
        if by != to {
            if let Some(w) = self.weapon(by, weapon) {
                w.actual_hits = w.actual_hits.wrapping_add(1);
            }
        }
    }

    pub fn after_spawn(&mut self, worm: i32) {
        self.worms[worm as usize].spawn_time = self.frame;
    }

    pub fn after_death(&mut self, worm: i32) {
        let frame = self.frame;
        let w = &mut self.worms[worm as usize];
        w.life_spans.push([w.spawn_time, frame]);
        w.spawn_time = -1;
    }

    /// Starts a frame. `worms` are the game's worms, in order.
    pub fn pre_tick(&mut self, ticks: u64, worms: &[LieroStatsWorm]) {
        self.frame_start = ticks;
        for w in &mut self.worms {
            let worm = &worms[w.index as usize];
            let h = if worm.visible { worm.health.max(0) } else { worm.max_health };
            w.frame_stats.push(LieroWormFrameStats {
                damage: 0,
                total_hp: worm.lives.wrapping_mul(worm.max_health).wrapping_add(h),
            });
        }
    }

    /// Ends a frame: where the worms are, and whether each let a weapon
    /// reload while holding a loaded one it could have changed from.
    pub fn tick(&mut self, ticks: u64, worms: &[LieroStatsWorm]) {
        self.process_time_total = self.process_time_total.wrapping_add(ticks.wrapping_sub(self.frame_start));
        for w in worms {
            if !w.visible {
                continue;
            }
            let ws = &mut self.worms[w.index as usize];
            self.presence.inc(w.x, w.y);
            ws.presence.inc(w.x, w.y);

            let bad = !w.fire
                && (!w.change || (!w.left && !w.right))
                && w.loading_left[w.current_weapon as usize] == 0
                && w.loading_left.iter().any(|&l| l > 0);
            if bad {
                ws.weapon_change_bad = ws.weapon_change_bad.wrapping_add(1);
            } else {
                ws.weapon_change_good = ws.weapon_change_good.wrapping_add(1);
            }
        }
        self.frame = self.frame.wrapping_add(1);
    }

    /// Ends the game: closes the open lives and keeps each worm's result.
    pub fn finish(&mut self, worms: &[LieroStatsWorm]) {
        let frame = self.frame;
        for (w, gw) in self.worms.iter_mut().zip(worms) {
            if w.spawn_time >= 0 {
                w.life_spans.push([w.spawn_time, frame]);
                w.spawn_time = -1;
            }
            w.lives = gw.lives;
            w.timer = gw.timer;
            w.kills = gw.kills;
        }
        self.game_time = frame;
    }

    pub fn ai_process_time(&mut self, worm: i32, time: u64) {
        let w = &mut self.worms[worm as usize];
        w.ai_process_time = w.ai_process_time.wrapping_add(time);
    }
}

// ---- The stats screen ----

/// `stretch` from `stats.hpp`: `src` resampled to `len` values, each the mean
/// of the values that fall in it.
fn stretch(src: &[f64], len: usize) -> Vec<f64> {
    let n = src.len();
    let mut v = vec![0.0; len];
    let (mut i, mut cum) = (0, 0);
    for out in v.iter_mut() {
        let mut c = 0;
        let mut sum = 0.0;
        cum += n;
        while i < n {
            sum += src[i];
            c += 1;
            if cum < len {
                break;
            }
            i += 1;
            cum -= len;
        }
        *out = if c != 0 { sum / c as f64 } else { 0.0 };
    }
    v
}

fn cumulative(v: &mut [f64]) {
    let mut prev = 0.0;
    for e in v {
        *e += prev;
        prev = *e;
    }
}

/// `std::max` and `std::min`, which keep the first of equal values.
fn cpp_max(a: f64, b: f64) -> f64 {
    if a < b {
        b
    } else {
        a
    }
}

fn cpp_min(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

/// `normalize` with `balance`: scales `v` so its largest magnitude
/// becomes `limit / 2`.
fn normalize(v: &mut [f64], limit: f64) {
    let Some(&first) = v.first() else { return };
    let max = v.iter().fold(first, |m, &e| cpp_max(m, e));
    let min = v.iter().fold(first, |m, &e| cpp_min(m, e));
    let max = cpp_max(max, 0.0);
    let min = cpp_min(min, 0.0);
    let max = cpp_max(-min, max);
    let min = cpp_min(-max, min);
    let range = max - min;
    if range > 0.0 {
        for e in v {
            *e = (*e * limit) / range;
        }
    }
}

fn sort_weapon_stats(v: &mut [LieroWeaponStats]) {
    cpp_sort::sort(v, &|a: &LieroWeaponStats, b: &LieroWeaponStats| a.actual_hp > b.actual_hp);
}

/// `percent` from the presenter: "12.34%", or nothing when `den` is 0.
fn percent(nom: i32, den: i32) -> String {
    if den == 0 {
        String::new()
    } else {
        format!("{:.2}%", nom as f64 * 100.0 / den as f64)
    }
}

/// `timeToStringFrames`.
fn time_frames(frames: i32) -> Vec<u8> {
    time_to_string_ex(frames.wrapping_mul(14), false, false)
}

/// What the stats screen shows, worked out once: `StatsScreen`'s
/// constructor.
pub struct LieroStatsScreen {
    combined: Vec<LieroWeaponStats>,
    weapons: [Vec<LieroWeaponStats>; 2],
    worm_damages: [Vec<f64>; 2],
    total_hp_diff: Vec<f64>,
}

impl LieroStatsScreen {
    pub fn new(stats: &LieroStats, render_res_x: i32) -> Self {
        let pane_width = render_res_x - 20;
        let graph_width = (pane_width - 20).max(0) as usize;

        let worm_damages = [0, 1].map(|i| {
            let damage: Vec<f64> = stats.worms[i].frame_stats.iter().map(|f| f.damage as f64).collect();
            let mut d = stretch(&damage, graph_width);
            cumulative(&mut d);
            normalize(&mut d, 50.0);
            d
        });

        let (a, b) = (&stats.worms[0].frame_stats, &stats.worms[1].frame_stats);
        let diff: Vec<f64> = a.iter().zip(b).map(|(a, b)| a.total_hp as f64 - b.total_hp as f64).collect();
        let mut total_hp_diff = stretch(&diff, graph_width);
        normalize(&mut total_hp_diff, 100.0);

        let mut combined: Vec<LieroWeaponStats> = (0..WEAPON_COUNT)
            .map(|i| {
                let mut ws = stats.worms[0].weapons[i];
                ws.combine(&stats.worms[1].weapons[i]);
                ws
            })
            .collect();
        let mut weapons = [0, 1].map(|i| stats.worms[i].weapons.to_vec());
        sort_weapon_stats(&mut combined);
        for w in &mut weapons {
            sort_weapon_stats(w);
        }

        LieroStatsScreen { combined, weapons, worm_damages, total_hp_diff }
    }
}

/// A sprite in the TC, `Sprite` in C++.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroSprite {
    pub mem: *const u8,
    pub width: i32,
    pub height: i32,
    pub pitch: i32,
}

/// What the stats screen draws with, besides the stats.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroStatsScene {
    pub font: LieroFont,
    /// Each worm facing the middle: `wormSpriteObj(2, i == 0 ? 1 : 0, i)`.
    pub worm_sprites: [LieroSprite; 2],
    pub names: [LieroStr; 2],
    pub game_mode: i32,
    pub game_mode_name: LieroStr,
    /// The TC's weapon names, by index.
    pub weapon_names: *const LieroStr,
    pub weapon_count: usize,
    pub render_res_x: i32,
    pub render_res_y: i32,
    /// `gvl::hires_ticks_per_sec()`, for the AI's processing time.
    pub ticks_per_sec: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum Place {
    Left,
    Center,
    Right,
}

const TEXT_COLOR: i32 = 7;
const PANE_X: i32 = 10;

/// `StatsRenderer`: lays the screen out from the top down, skipping what is
/// outside the screen.
struct Layout<'a> {
    scr: &'a LieroBitmap,
    scene: &'a LieroStatsScene,
    stats: &'a LieroStats,
    pane_width: i32,
    offs_x: i32,
    y: i32,
}

impl Layout<'_> {
    /// `Font::drawText` for a `gvl::cell`: nothing for an empty text, and
    /// centred or right-aligned on x.
    unsafe fn text(&self, s: &[u8], place: Place, x: i32, y: i32, color: i32) {
        if s.is_empty() {
            return;
        }
        let x = match place {
            Place::Left => x,
            Place::Center => x - self.scene.font.dims(s).0 / 2,
            Place::Right => x - self.scene.font.dims(s).0,
        };
        self.scene.font.text(self.scr, s, x, y, color, 1);
    }

    unsafe fn worm_sprite(&self, i: usize, x: i32, y: i32) {
        let s = self.scene.worm_sprites[i];
        liero_gfx_blit_image(self.scr, s.mem, x, y, s.width, s.height, s.pitch);
    }

    unsafe fn name(&self, i: usize) -> &[u8] {
        self.scene.names[i].bytes()
    }

    unsafe fn pane(&mut self, n: i32, left_x: i32, top_y: i32, p: impl FnOnce(&mut Self)) {
        let res_x = self.scene.render_res_x;
        self.offs_x = n.wrapping_mul(res_x).wrapping_add(left_x);
        if self.offs_x >= -res_x && self.offs_x < res_x {
            self.y = top_y.wrapping_add(10);
            liero_gfx_draw_rounded_box(self.scr, self.offs_x + PANE_X, self.y, 0, 2000, self.pane_width);
            self.y += 10;
            p(self);
        }
    }

    /// Runs `b` if a block `height` high at the current y would show, and
    /// moves down past it. Returns whether it ran.
    fn hblock(&mut self, height: i32, b: impl FnOnce(&mut Self)) -> bool {
        let ran = self.y < self.scene.render_res_y && self.y + height > 0;
        if ran {
            b(self);
        }
        self.y += height;
        ran
    }

    unsafe fn draw_worms(&mut self) {
        self.hblock(20, |r| {
            let res_x = r.scene.render_res_x;
            for i in 0..2 {
                let x = res_x / 2 + if i == 0 { -1 } else { 1 } * (res_x / 4) + r.offs_x;
                r.worm_sprite(i, x - 8, r.y);
                let (place, dx) = if i == 0 { (Place::Right, -16) } else { (Place::Left, 16) };
                r.text(r.name(i), place, x + dx, r.y + 2, TEXT_COLOR);
            }
        });
    }

    unsafe fn draw_worm(&mut self, i: usize) {
        let visible = self.hblock(20, |r| {
            let x = r.scene.render_res_x / 2 + r.offs_x;
            r.worm_sprite(i, x - 8, r.y);
            let (place, dx) = if i == 0 { (Place::Right, -16) } else { (Place::Left, 16) };
            r.text(r.name(i), place, x + dx, r.y + 2, TEXT_COLOR);
        });
        if !visible {
            self.worm_sprite(i, 18 + self.offs_x - 8, 10);
        }
    }

    /// A row with `name` in the middle and each worm's value beside it.
    unsafe fn draw_worm_stat(&mut self, name: &[u8], stat: impl Fn(&WormStats) -> Vec<u8>) {
        self.hblock(11, |r| {
            let mid = r.scene.render_res_x / 2 + r.offs_x;
            r.text(name, Place::Center, mid, r.y, TEXT_COLOR);
            for i in 0..2 {
                let (place, dx) = if i == 0 { (Place::Right, -40) } else { (Place::Left, 40) };
                r.text(&stat(&r.stats.worms[i]), place, mid + dx, r.y, TEXT_COLOR);
            }
        });
    }

    unsafe fn draw_worm_int(&mut self, name: &[u8], field: impl Fn(&WormStats) -> i32) {
        self.draw_worm_stat(name, |w| field(w).to_string().into_bytes());
    }

    /// A row with `name` left of the middle and `value` right of it.
    unsafe fn draw_stat(&mut self, name: &[u8], value: &[u8]) {
        self.hblock(11, |r| {
            let mid = r.scene.render_res_x / 2 + r.offs_x;
            r.text(name, Place::Right, mid, r.y, TEXT_COLOR);
            r.text(value, Place::Left, mid + 10, r.y, TEXT_COLOR);
        });
    }

    /// A heading: level 0 is a section of its own, level 1 a weapon's name.
    unsafe fn section(&mut self, s: &[u8], level: i32) {
        let (x, color) = if level > 0 { (self.offs_x + 40, 3) } else { (self.offs_x + 20, TEXT_COLOR) };
        self.text(s, Place::Left, x, self.y, color);
        if level == 0 {
            self.y += 11;
        }
    }

    fn gap(&mut self) {
        self.y += 5;
    }

    unsafe fn weapon_stats(&mut self, list: &[LieroWeaponStats]) {
        for ws in list.iter().filter(|ws| ws.total_hp > 0) {
            let name = match usize::try_from(ws.index) {
                Ok(i) if i < self.scene.weapon_count => (*self.scene.weapon_names.add(i)).bytes(),
                _ => &[],
            };
            self.section(name, 1);
            let hits = format!("{}/{} ({})", ws.actual_hits, ws.potential_hits, percent(ws.actual_hits, ws.potential_hits));
            self.draw_stat(b"hits", hits.as_bytes());
            if ws.potential_hp > 0 {
                let damage = format!("{}/{} ({})", ws.actual_hp, ws.potential_hp, percent(ws.actual_hp, ws.potential_hp));
                self.draw_stat(b"damage", damage.as_bytes());
            }
            if ws.total_hp != ws.actual_hp {
                self.draw_stat(b"total damage", ws.total_hp.to_string().as_bytes());
            }
            self.gap();
        }
    }

    unsafe fn graph(&mut self, data: &[f64], height: i32, color: u8, neg_color: u8, balanced: bool) {
        self.y += 2;
        self.hblock(height, |r| {
            liero_gfx_draw_graph(r.scr, data.as_ptr(), data.len(), height, 20 + r.offs_x, r.y, color, neg_color, balanced);
        });
        self.y += 7;
    }

    unsafe fn heatmap(&mut self, hm: &Heatmap) {
        self.y += 2;
        self.hblock(hm.height, |r| {
            let x = PANE_X + r.pane_width / 2 - hm.width / 2 + r.offs_x;
            liero_gfx_draw_heatmap(r.scr, x, r.y, hm.map.as_ptr(), hm.width, hm.height);
        });
        self.y += 7;
    }
}

impl LieroStatsScreen {
    /// Draws the overall pane in the middle and each worm's pane beside it,
    /// `pane` screens to the right (-1 to 1, or between while moving) and
    /// scrolled `offset` pixels: `StatsScreen::draw`.
    ///
    /// # Safety
    /// See `LieroBitmap` and the scene's views.
    pub unsafe fn draw(&self, stats: &LieroStats, scene: &LieroStatsScene, scr: &LieroBitmap, pane: f64, offset: f64) {
        let mut r = Layout { scr, scene, stats, pane_width: scene.render_res_x - 20, offs_x: 0, y: 0 };
        let offs_x = (pane * -(scene.render_res_x as f64)).floor() as i32;
        let offs_y = offset as i32;

        r.pane(0, offs_x, offs_y, |r| {
            r.draw_worms();

            let old_y = r.y;
            r.y -= 20;
            r.draw_stat(scene.game_mode_name.bytes(), &time_frames(stats.game_time));
            r.y = old_y;

            let ticks_per_sec = scene.ticks_per_sec.max(1);
            r.draw_worm_stat(b"ai processing", |w| {
                format!("{}ms", (w.ai_process_time.wrapping_mul(1000) / ticks_per_sec) as i32).into_bytes()
            });

            if scene.game_mode == HOLDAZONE || scene.game_mode == GAME_OF_TAG {
                r.draw_worm_stat(b"timer", |w| time_to_string(w.timer).to_vec());
            } else {
                r.draw_worm_int(b"lives left", |w| w.lives);
            }
            r.draw_worm_int(b"kills", |w| w.kills);
            r.draw_worm_int(b"damage dealt", |w| w.damage_dealt);
            r.draw_worm_int(b"damage received", |w| w.damage);
            r.draw_worm_int(b"damage to self", |w| w.self_damage);
            r.draw_worm_stat(b"shortest life", |w| time_frames(w.life_stats().0));
            r.draw_worm_stat(b"longest life", |w| time_frames(w.life_stats().1));
            r.draw_worm_stat(b"loading efficiency", |w| {
                percent(w.weapon_change_good, w.weapon_change_good.wrapping_add(w.weapon_change_bad)).into_bytes()
            });

            r.gap();
            r.weapon_stats(&self.combined);

            r.section(b"Total health difference", 0);
            r.graph(&self.total_hp_diff, 100, WORM_COLOUR_INDEXES[0], WORM_COLOUR_INDEXES[1], true);

            r.section(b"Presence", 0);
            r.heatmap(&stats.presence);
        });

        for i in 0..2 {
            let w = &stats.worms[i];
            r.pane(if i == 0 { -1 } else { 1 }, offs_x, offs_y, |r| {
                r.draw_worm(i);
                r.weapon_stats(&self.weapons[i]);

                r.section(b"Damage over time", 0);
                r.graph(&self.worm_damages[i], 50, WORM_COLOUR_INDEXES[i], 0, false);

                r.section(b"Presence", 0);
                r.heatmap(&w.presence);
                r.section(b"Damage", 0);
                r.heatmap(&w.damage_hm);
            });
        }
    }
}

// ---- C interface ----

/// A heatmap's size and counts, for C++ to read.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroHeatmapView {
    pub width: i32,
    pub height: i32,
    pub org_width: i32,
    pub org_height: i32,
    pub map: *const i32,
    pub len: usize,
}

/// A worm's stats, for C++ to read. The pointers are valid until the stats
/// next change.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroWormStatsView {
    /// Pairs of spawn and death frames.
    pub life_spans: *const [i32; 2],
    pub life_span_count: usize,
    /// 40 weapons, by index.
    pub weapons: *const LieroWeaponStats,
    pub damage: i32,
    pub damage_dealt: i32,
    pub self_damage: i32,
    pub damage_hm: LieroHeatmapView,
    pub presence: LieroHeatmapView,
    pub weapon_change_good: i32,
    pub weapon_change_bad: i32,
    pub frame_stats: *const LieroWormFrameStats,
    pub frame_stat_count: usize,
    pub spawn_time: i32,
    pub index: i32,
    pub lives: i32,
    pub timer: i32,
    pub kills: i32,
    pub ai_process_time: u64,
}

/// All the stats, for C++ to read.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroStatsView {
    pub frame: i32,
    pub game_time: i32,
    pub presence: LieroHeatmapView,
    pub worms: [LieroWormStatsView; 2],
    pub frame_start: u64,
    pub process_time_total: u64,
}

impl LieroStats {
    pub fn view(&self) -> LieroStatsView {
        let worm = |w: &WormStats| LieroWormStatsView {
            life_spans: w.life_spans.as_ptr(),
            life_span_count: w.life_spans.len(),
            weapons: w.weapons.as_ptr(),
            damage: w.damage,
            damage_dealt: w.damage_dealt,
            self_damage: w.self_damage,
            damage_hm: w.damage_hm.view(),
            presence: w.presence.view(),
            weapon_change_good: w.weapon_change_good,
            weapon_change_bad: w.weapon_change_bad,
            frame_stats: w.frame_stats.as_ptr(),
            frame_stat_count: w.frame_stats.len(),
            spawn_time: w.spawn_time,
            index: w.index,
            lives: w.lives,
            timer: w.timer,
            kills: w.kills,
            ai_process_time: w.ai_process_time,
        };
        LieroStatsView {
            frame: self.frame,
            game_time: self.game_time,
            presence: self.presence.view(),
            worms: [worm(&self.worms[0]), worm(&self.worms[1])],
            frame_start: self.frame_start,
            process_time_total: self.process_time_total,
        }
    }
}

unsafe fn worms<'a>(worms: *const LieroStatsWorm, count: usize) -> &'a [LieroStatsWorm] {
    if count == 0 || worms.is_null() {
        &[]
    } else {
        std::slice::from_raw_parts(worms, count)
    }
}

#[no_mangle]
pub extern "C" fn liero_stats_view(s: &LieroStats) -> LieroStatsView {
    s.view()
}

#[no_mangle]
pub extern "C" fn liero_stats_damage_potential(s: &mut LieroStats, by: i32, weapon: i32, hp: i32) {
    s.damage_potential(by, weapon, hp);
}

#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn liero_stats_damage_dealt(s: &mut LieroStats, by: i32, weapon: i32, to: i32, x: i32, y: i32, hp: i32, has_hit: bool) {
    s.damage_dealt(by, weapon, to, x, y, hp, has_hit);
}

#[no_mangle]
pub extern "C" fn liero_stats_shot(s: &mut LieroStats, by: i32, weapon: i32) {
    s.shot(by, weapon);
}

#[no_mangle]
pub extern "C" fn liero_stats_hit(s: &mut LieroStats, by: i32, weapon: i32, to: i32) {
    s.hit(by, weapon, to);
}

#[no_mangle]
pub extern "C" fn liero_stats_after_spawn(s: &mut LieroStats, worm: i32) {
    s.after_spawn(worm);
}

#[no_mangle]
pub extern "C" fn liero_stats_after_death(s: &mut LieroStats, worm: i32) {
    s.after_death(worm);
}

/// # Safety
/// `worms` must point to `count` worms.
#[no_mangle]
pub unsafe extern "C" fn liero_stats_pre_tick(s: &mut LieroStats, ticks: u64, w: *const LieroStatsWorm, count: usize) {
    s.pre_tick(ticks, worms(w, count));
}

/// # Safety
/// `worms` must point to `count` worms.
#[no_mangle]
pub unsafe extern "C" fn liero_stats_tick(s: &mut LieroStats, ticks: u64, w: *const LieroStatsWorm, count: usize) {
    s.tick(ticks, worms(w, count));
}

/// # Safety
/// `worms` must point to `count` worms.
#[no_mangle]
pub unsafe extern "C" fn liero_stats_finish(s: &mut LieroStats, w: *const LieroStatsWorm, count: usize) {
    s.finish(worms(w, count));
}

#[no_mangle]
pub extern "C" fn liero_stats_ai_process_time(s: &mut LieroStats, worm: i32, time: u64) {
    s.ai_process_time(worm, time);
}

/// The stats screen for `s`, laid out for a screen `render_res_x` wide. Free
/// it with `liero_stats_screen_free`.
#[no_mangle]
pub extern "C" fn liero_stats_screen_new(s: &LieroStats, render_res_x: i32) -> *mut LieroStatsScreen {
    Box::into_raw(Box::new(LieroStatsScreen::new(s, render_res_x)))
}

/// # Safety
/// `screen` must come from `liero_stats_screen_new`, or be null.
#[no_mangle]
pub unsafe extern "C" fn liero_stats_screen_free(screen: *mut LieroStatsScreen) {
    if !screen.is_null() {
        drop(Box::from_raw(screen));
    }
}

/// # Safety
/// See `LieroBitmap` and `LieroStatsScene`.
#[no_mangle]
pub unsafe extern "C" fn liero_stats_screen_draw(
    screen: &LieroStatsScreen,
    s: &LieroStats,
    scene: &LieroStatsScene,
    scr: &LieroBitmap,
    pane: f64,
    offset: f64,
) {
    screen.draw(s, scene, scr, pane, offset);
}

const SCANCODE_RETURN: u32 = 40;
const SCANCODE_ESCAPE: u32 = 41;
const SCANCODE_RIGHT: u32 = 79;
const SCANCODE_LEFT: u32 = 80;
const SCANCODE_DOWN: u32 = 81;
const SCANCODE_UP: u32 = 82;

/// The stats screen after a game, over its last frame: Up and Down scroll,
/// Left and Right move between the panes, Enter or Escape closes it.
struct StatsPresenter {
    stats: *const LieroStats,
    screen: LieroStatsScreen,
    /// The scene's texts are copied, so the game's can change.
    names: [Vec<u8>; 2],
    game_mode_name: Vec<u8>,
    weapon_names: Vec<Vec<u8>>,
    scene: LieroStatsScene,
    /// The last frame of the game, row by row.
    bg: Vec<u8>,
    offset: f64,
    dest_offset: f64,
    pane: f64,
    dest_pane: f64,
    shown: bool,
}

fn text(s: &[u8]) -> LieroStr {
    LieroStr { ptr: s.as_ptr(), len: s.len() }
}

impl StatsPresenter {
    fn draw(&mut self, scr: &LieroBitmap) {
        let weapon_names: Vec<LieroStr> = self.weapon_names.iter().map(|n| text(n)).collect();
        let scene = LieroStatsScene {
            names: [text(&self.names[0]), text(&self.names[1])],
            game_mode_name: text(&self.game_mode_name),
            weapon_names: weapon_names.as_ptr(),
            weapon_count: weapon_names.len(),
            ..self.scene
        };
        // SAFETY: the stats and the scene's font and sprites outlive the
        // screen, and the texts are ours.
        unsafe { self.screen.draw(&*self.stats, &scene, scr, self.pane, self.offset) };
    }
}

/// Copies a bitmap's `w` x `h` pixels into or out of `buf`.
fn copy_rows(scr: &LieroBitmap, buf: &mut Vec<u8>, save: bool) {
    let (w, h) = (scr.w.max(0) as usize, scr.h.max(0) as usize);
    buf.resize(w * h, 0);
    for y in 0..h {
        // SAFETY: the bitmap holds h rows of pitch bytes, w of them pixels.
        let row = unsafe { std::slice::from_raw_parts_mut(scr.pixels.add(y * scr.pitch as usize), w) };
        if save {
            buf[y * w..(y + 1) * w].copy_from_slice(row);
        } else {
            row.copy_from_slice(&buf[y * w..(y + 1) * w]);
        }
    }
}

impl Screen for StatsPresenter {
    fn tick(&mut self, host: &Host) -> Tick {
        if self.shown {
            host.process();
            if host.test_key(SCANCODE_DOWN) {
                self.dest_offset -= 10.0;
            } else if host.test_key(SCANCODE_UP) {
                self.dest_offset = (self.dest_offset + 10.0).min(0.0);
            } else if host.test_key_once(SCANCODE_RIGHT) {
                self.dest_pane = (self.dest_pane + 1.0).min(1.0);
            } else if host.test_key_once(SCANCODE_LEFT) {
                self.dest_pane = (self.dest_pane - 1.0).max(-1.0);
            } else if host.test_key(SCANCODE_RETURN) || host.test_key(SCANCODE_ESCAPE) {
                host.fill_screen(0);
                host.clear_keys();
                return Tick::close(0);
            }

            self.pane = self.pane * 0.89 + self.dest_pane * 0.11;
            self.offset = self.offset * 0.89 + self.dest_offset * 0.11;
            if self.offset > 0.0 {
                self.offset = 0.0;
            }
        }

        if let Some(scr) = host.screen() {
            copy_rows(&scr, &mut self.bg, false);
            self.draw(&scr);
        }
        // We don't use the level's palette, as its colours are unpredictable.
        host.menu_palette();
        self.shown = true;
        Tick::stay(true)
    }
}

/// The stats screen for `stats` over what the screen shows now. `scene`'s
/// texts are copied; the rest of it, and `stats`, must outlive the screen.
///
/// # Safety
/// `stats` and `scene` are valid, as in `liero_stats_screen_draw`.
#[no_mangle]
pub unsafe extern "C" fn liero_stats_presenter_screen(stats: &LieroStats, scene: &LieroStatsScene) -> LieroScreen {
    let host = crate::screen::current_host();
    host.clear_keys();
    let mut bg = Vec::new();
    if let Some(scr) = host.screen() {
        copy_rows(&scr, &mut bg, true);
    }
    let weapon_names = (0..scene.weapon_count).map(|i| (*scene.weapon_names.add(i)).bytes().to_vec()).collect();
    into_ffi(Box::new(StatsPresenter {
        stats,
        screen: LieroStatsScreen::new(stats, scene.render_res_x),
        names: [scene.names[0].bytes().to_vec(), scene.names[1].bytes().to_vec()],
        game_mode_name: scene.game_mode_name.bytes().to_vec(),
        weapon_names,
        scene: *scene,
        bg,
        offset: 0.0,
        dest_offset: 0.0,
        pane: 0.0,
        dest_pane: 0.0,
        shown: false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::Fnv;

    // From rust/liero-game/reference/reference_values.cpp.
    const EXPECTED_PERCENT: u64 = 0x1eebfb0ae2b13539;

    #[test]
    fn percent_formats_like_sprintf() {
        let mut h = Fnv::new();
        let mut add = |s: String| {
            for b in s.bytes() {
                h.add(b as u32);
            }
            h.add(0xffff_ffff);
        };
        for den in 0..=1000 {
            for nom in 0..=den + 3 {
                add(percent(nom, den));
            }
        }
        for nom in [-5, 1_000_000, i32::MAX, i32::MIN] {
            add(percent(nom, 7));
        }
        assert_eq!(h.finish(), EXPECTED_PERCENT);
    }

    #[test]
    fn heatmap_clamps_and_weights() {
        let mut hm = Heatmap::new();
        hm.inc(-50, 10_000);
        assert_eq!(hm.map[(174 * 252) as usize], 1);
        hm.inc_area(0, 0, 2);
        assert_eq!(hm.map[0], 32);
        assert_eq!(hm.map[2 * 252 + 2], 2 * (16 - 16));
        assert_eq!(hm.map[252 + 2], 2 * (16 - 4));
        assert_eq!(hm.map.iter().filter(|&&v| v != 0).count(), 9);

        // The hash games never reach the level's right edge.
        let mut edge = Heatmap::new();
        edge.inc(503, 0);
        edge.inc(1000, 0);
        assert_eq!(edge.map[251], 2);
    }

    #[test]
    fn stretch_averages() {
        // As in the C++, the value after each boundary counts in both.
        assert_eq!(stretch(&[1.0, 3.0, 5.0, 7.0], 2), vec![3.0, 6.0]);
        assert_eq!(stretch(&[4.0], 3), vec![4.0, 4.0, 4.0]);
        assert_eq!(stretch(&[], 2), vec![0.0, 0.0]);
        assert_eq!(stretch(&[1.0, 2.0, 3.0], 2), vec![1.5, 2.5]);
    }

    #[test]
    fn normalize_balances() {
        let mut v = vec![1.0, -3.0, 2.0];
        normalize(&mut v, 100.0);
        assert_eq!(v, vec![1.0 * 100.0 / 6.0, -50.0, 2.0 * 100.0 / 6.0]);
        let mut z = vec![0.0, 0.0];
        normalize(&mut z, 50.0);
        assert_eq!(z, vec![0.0, 0.0]);
    }

    #[test]
    fn life_stats() {
        let mut w = WormStats::new(0);
        assert_eq!(w.life_stats(), (0, 0));
        w.life_spans = vec![[0, 100], [120, 130], [140, 400]];
        assert_eq!(w.life_stats(), (10, 260));
    }

    #[test]
    fn events() {
        let mut s = LieroStats::default();
        let worm = |index, visible| LieroStatsWorm { index, visible, health: 80, lives: 2, max_health: 100, ..Default::default() };
        let ws = [worm(0, true), worm(1, false)];

        // Damage before any frame has no frame stats to add to.
        s.damage_dealt(1, 3, 0, 10, 10, 5, false);
        s.pre_tick(10, &ws);
        assert_eq!(s.worms[0].frame_stats[0].total_hp, 280);
        assert_eq!(s.worms[1].frame_stats[0].total_hp, 300);
        // A worm still shown with negative health counts as having none.
        let mut dying = ws;
        dying[0].health = -10;
        s.pre_tick(10, &dying);
        assert_eq!(s.worms[0].frame_stats[1].total_hp, 200);
        s.worms[0].frame_stats.pop();
        s.worms[1].frame_stats.pop();

        s.damage_potential(0, 4, 20);
        s.damage_potential(-1, 4, 20);
        s.damage_potential(0, -1, 20);
        s.shot(0, 4);
        s.hit(0, 4, 1);
        s.hit(0, 4, 0);
        s.damage_dealt(0, 4, 1, 100, 100, 7, false);
        s.damage_dealt(0, 4, 1, 100, 100, 3, true);
        s.damage_dealt(0, 4, 0, 100, 100, 2, false);
        s.damage_dealt(-1, -1, 0, 100, 100, 1, false);
        let w = s.worms[0].weapons[4];
        assert_eq!((w.potential_hp, w.actual_hp, w.potential_hits, w.actual_hits, w.total_hp), (20, 7, 1, 1, 10));
        assert_eq!((s.worms[0].damage_dealt, s.worms[0].self_damage, s.worms[0].damage), (10, 2, 8));
        assert_eq!(s.worms[1].weapons[3].actual_hp, 5);
        assert_eq!(s.worms[1].frame_stats[0].damage, 10);
        assert_eq!(s.worms[0].frame_stats[0].damage, 3);

        let mut reloading = ws;
        reloading[0].loading_left = [0, 5, 0, 0, 0];
        s.tick(25, &reloading);
        reloading[0].fire = true;
        s.tick(30, &reloading);
        assert_eq!((s.worms[0].weapon_change_good, s.worms[0].weapon_change_bad), (1, 1));
        assert_eq!((s.worms[1].weapon_change_good, s.worms[1].weapon_change_bad), (0, 0));
        assert_eq!(s.process_time_total, 15 + 20);
        assert_eq!(s.frame, 2);

        s.after_spawn(1);
        s.frame = 50;
        s.after_death(1);
        s.after_spawn(1);
        s.frame = 70;
        let mut end = ws;
        end[1].kills = 3;
        s.finish(&end);
        assert_eq!(s.worms[1].life_spans, vec![[2, 50], [50, 70]]);
        assert_eq!((s.worms[1].spawn_time, s.worms[1].kills, s.game_time), (-1, 3, 70));
        s.ai_process_time(1, 9);
        assert_eq!(s.worms[1].ai_process_time, 9);
    }

    #[test]
    fn screen_sorts_by_first_hit_damage() {
        let mut s = LieroStats::default();
        s.worms[0].weapons[5].actual_hp = 10;
        s.worms[1].weapons[7].actual_hp = 30;
        s.worms[1].weapons[5].actual_hp = 15;
        s.worms[1].weapons[5].total_hp = 40;
        let screen = LieroStatsScreen::new(&s, 320);
        assert_eq!(screen.combined[0].index, 7);
        assert_eq!(screen.combined[1].index, 5);
        assert_eq!(screen.combined[1].actual_hp, 25);
        assert_eq!(screen.combined[1].total_hp, 40);
        assert_eq!(screen.weapons[0][0].index, 5);
        assert_eq!(screen.total_hp_diff.len(), 280);
    }
}
