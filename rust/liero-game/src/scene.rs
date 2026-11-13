//! The game as its viewports see it. Rust builds each viewport's
//! `LieroScene` from the game's fields, as C++'s `viewport.cpp` copied it
//! from its `Game`, and processes the viewports at the end of a tick.

use crate::ai::LieroWormAi;
use crate::game::{LieroGameFields, LieroGameSettings};
use crate::gfx::LieroBitmap;
use crate::tc::{c, h, s, LieroTcData, LieroTcSprites};
use crate::viewport::{
    liero_viewport_draw, liero_viewport_process, LieroScene, LieroSpriteSet, LieroViewBObject, LieroViewBonus,
    LieroViewNObject, LieroViewSObject, LieroViewWObject, LieroViewWeapon, LieroViewWorm, LieroViewWormWeapon,
    LieroViewport, LieroViewportBanner,
};
use crate::worm::{LIERO_CONTROL_CHANGE, LIERO_CONTROL_FIRE};

/// What a draw needs besides the game: the settings the tick doesn't use,
/// and the screen.
#[derive(Clone, Copy, Debug)]
pub struct LieroDrawSettings {
    pub names_on_bonuses: bool,
    pub allow_viewing_spawn_point: bool,
    pub map: bool,
    pub render_res_x: i32,
    pub render_res_y: i32,
    /// The game is running (`StateGame`), not paused or over.
    pub state_game: bool,
    pub is_replay: bool,
}

unsafe fn slice<'a, T>(p: *const T, n: i32) -> &'a [T] {
    unsafe {
        if n <= 0 || p.is_null() {
            &[]
        } else {
            std::slice::from_raw_parts(p, n as usize)
        }
    }
}

fn sprites(s: &LieroTcSprites) -> LieroSpriteSet<'_> {
    LieroSpriteSet {
        data: s.pixels(),
        width: s.width,
        height: s.height,
        count: s.count,
    }
}

/// The worms, in the game's order (`viewWorms`).
///
/// # Safety
/// The pointers in `f` are valid, and the worms' names stay so while the
/// views are used.
pub(crate) unsafe fn view_worms<'a>(f: &LieroGameFields) -> Vec<LieroViewWorm<'a>> {
    unsafe {
        slice(f.worms, f.worm_count)
            .iter()
            .map(|gw| {
                let w = &*gw.data;
                let traces = (gw.ai as *const LieroWormAi).as_ref().map_or(&[][..], |ai| ai.traces());
                LieroViewWorm {
                    pos: w.pos,
                    visible: w.visible,
                    ready: w.ready,
                    health: w.health,
                    max_health: gw.max_health,
                    killed_timer: w.killed_timer,
                    stats_x: w.stats_x,
                    kills: w.kills,
                    lives: w.lives,
                    timer: w.timer,
                    index: w.index,
                    name: gw.name.bytes(),
                    color: gw.color,
                    pressed_change: w.control_states >> LIERO_CONTROL_CHANGE & 1 != 0,
                    pressed_fire: w.control_states >> LIERO_CONTROL_FIRE & 1 != 0,
                    current_frame: w.current_frame,
                    direction: w.direction,
                    hotspot_x: w.hotspot_x,
                    hotspot_y: w.hotspot_y,
                    aiming_angle: w.aiming_angle,
                    make_sight_green: w.make_sight_green,
                    rope_out: w.ninjarope.out,
                    rope_pos: w.ninjarope.pos,
                    fire_cone: w.fire_cone,
                    last_killed_by_idx: w.last_killed_by_idx,
                    current_weapon: w.current_weapon,
                    weapons: w.weapons.map(|ww| LieroViewWormWeapon {
                        weapon: ww.kind,
                        ammo: ww.ammo,
                        loading_left: ww.loading_left,
                    }),
                    steerable_count: w.steerable_count,
                    steerable_sum_x: w.steerable_sum_x,
                    steerable_sum_y: w.steerable_sum_y,
                    traces,
                }
            })
            .collect()
    }
}

/// Each viewport's worm, shake and banner after a tick
/// (`Game::processViewports`).
///
/// # Safety
/// The pointers in `f` are valid.
pub(crate) unsafe fn process_viewports(f: &LieroGameFields) {
    unsafe {
        let worms = view_worms(f);
        for (list, n) in [
            (f.viewports, f.viewport_count),
            (f.spectator_viewports, f.spectator_viewport_count),
        ] {
            for &vp in slice(list, n) {
                liero_viewport_process(&mut *vp, (*vp).kind, &worms);
            }
        }
    }
}

/// Draws viewport `vp` on `scr` (`Viewport::draw` and
/// `SpectatorViewport::draw`, by its kind).
///
/// # Safety
/// The pointers in `game` and `settings` are valid, and `vp` is one of the
/// game's viewports.
pub unsafe fn liero_game_draw_viewport(
    game: &LieroGameFields,
    settings: &LieroGameSettings,
    draw: &LieroDrawSettings,
    vp: *mut LieroViewport,
    scr: &LieroBitmap,
) {
    unsafe {
        let tc: &LieroTcData = &*game.tc;
        let d = &*game.game;
        let l = lists(game, settings, vp);

        let scene = LieroScene {
            worms: &l.worms,
            weapons: &l.weapons,
            bonuses: &l.bonuses,
            sobjects: &l.sobjects,
            wobjects: &l.wobjects,
            nobjects: &l.nobjects,
            bobjects: &l.bobjects,
            viewports: &l.banners,

            small_sprites: sprites(&tc.small_sprites),
            large_sprites: sprites(&tc.large_sprites),
            worm_sprites: sprites(&tc.worm_sprites),
            fire_cone_sprites: sprites(&tc.fire_cone_sprites),
            text_sprites: sprites(&tc.text_sprites),
            font: &tc.font,
            materials: &tc.materials,
            level: game.level.view(),
            bonus_frames: tc.bonus_frames,

            reloading: tc.text(s("Reloading")),
            kills: tc.text(s("Kills")),
            lives: tc.text(s("Lives")),
            press_fire: tc.text(s("PressFire")),
            youre_it: tc.text(s("YoureIt")),
            killed_msg: tc.text(s("KilledMsg")),
            committed_suicide_msg: tc.text(s("CommittedSuicideMsg")),

            cycles: d.cycles,
            game_mode: settings.game_mode,
            holdazone: d.holdazone,
            zone_capture_time: settings.zone_capture_time,
            last_killed_idx: d.last_killed_idx,
            got_changed: d.got_changed,
            shadow: settings.shadow,
            names_on_bonuses: draw.names_on_bonuses,
            allow_viewing_spawn_point: draw.allow_viewing_spawn_point,
            map: draw.map,
            rem_exp: tc.hack(h("RemExp")),
            laser_weapon: tc.constant(c("LaserWeapon")),
            bonus_flicker_time: tc.constant(c("BonusFlickerTime")),
            nr_colour_begin: tc.constant(c("NRColourBegin")),
            nr_colour_end: tc.constant(c("NRColourEnd")),

            render_res_x: draw.render_res_x,
            render_res_y: draw.render_res_y,
            state_game: draw.state_game,
            is_replay: draw.is_replay,
        };
        liero_viewport_draw(&mut *vp, (*vp).kind, l.self_index, &scene, scr);
    }
}

/// The lists a viewport draws, in C++'s order, which decides what is drawn
/// on top, and `vp`'s place among the split-screen viewports.
struct Lists<'a> {
    worms: Vec<LieroViewWorm<'a>>,
    weapons: Vec<LieroViewWeapon<'a>>,
    bonuses: Vec<LieroViewBonus>,
    sobjects: Vec<LieroViewSObject>,
    wobjects: Vec<LieroViewWObject>,
    nobjects: Vec<LieroViewNObject>,
    bobjects: Vec<LieroViewBObject>,
    banners: Vec<LieroViewportBanner>,
    self_index: i32,
}

/// # Safety
/// As for `liero_game_draw_viewport`.
unsafe fn lists<'a>(game: &LieroGameFields, settings: &LieroGameSettings, vp: *mut LieroViewport) -> Lists<'a> {
    unsafe {
        let tc: &LieroTcData = &*game.tc;
        let d = &*game.game;

        let worms = view_worms(game);
        let weapons: Vec<LieroViewWeapon> = (tc.weapons.iter())
            .map(|w| {
                let t = settings.loading_time.wrapping_mul(w.loading_time) / 100;
                LieroViewWeapon {
                    name: &w.name,
                    ammo: w.ammo,
                    loading_time: w.loading_time,
                    computed_loading_time: if t == 0 { 1 } else { t },
                    laser_sight: w.laser_sight,
                    color_bullets: w.color_bullets,
                    fire_cone: w.fire_cone,
                    start_frame: w.start_frame,
                    shot_type: w.shot_type,
                    shadow: w.shadow,
                }
            })
            .collect();
        let bonuses: Vec<LieroViewBonus> = d
            .bonuses
            .arr
            .iter()
            .filter(|b| b.used)
            .map(|b| LieroViewBonus {
                x: b.x,
                y: b.y,
                timer: b.timer,
                frame: b.frame,
                weapon: b.weapon,
            })
            .collect();
        let sobjects: Vec<LieroViewSObject> = d
            .sobjects
            .arr
            .iter()
            .filter(|o| o.used)
            .map(|o| {
                let start_frame = tc.sobjects[o.id as usize].start_frame;
                LieroViewSObject {
                    x: o.x,
                    y: o.y,
                    cur_frame: o.cur_frame,
                    start_frame,
                }
            })
            .collect();
        let wobjects: Vec<LieroViewWObject> = d
            .wobjects
            .arr
            .iter()
            .enumerate()
            .filter(|(_, o)| o.used)
            .map(|(i, o)| LieroViewWObject {
                pos: o.pos,
                weapon: o.kind,
                cur_frame: o.cur_frame,
                slot: i as i32,
            })
            .collect();
        let nobjects: Vec<LieroViewNObject> = d
            .nobjects
            .arr
            .iter()
            .filter(|o| o.used)
            .map(|o| LieroViewNObject {
                pos: o.pos,
                cur_frame: o.cur_frame,
                start_frame: tc.nobjects[o.kind as usize].start_frame,
            })
            .collect();
        let bobjects: Vec<LieroViewBObject> = d
            .bobjects
            .used()
            .iter()
            .map(|b| LieroViewBObject {
                pos: b.pos,
                color: b.color,
            })
            .collect();

        let vps = slice(game.viewports, game.viewport_count);
        let self_index = vps.iter().position(|&v| v == vp).map_or(-1, |i| i as i32);
        let banners: Vec<LieroViewportBanner> = vps
            .iter()
            .map(|&v| LieroViewportBanner {
                worm_idx: (*v).worm_idx,
                banner_y: (*v).banner_y,
            })
            .collect();

        Lists {
            worms,
            weapons,
            bonuses,
            sobjects,
            wobjects,
            nobjects,
            bobjects,
            banners,
            self_index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{LieroGameData, LieroGameWorm, LieroRect};
    use crate::level::LieroLevel;
    use crate::sfx::LieroSoundOut;
    use crate::tc::LieroTcSObject;
    use crate::tc::{LieroTcNObject, LieroTcWeapon};
    use crate::text::LieroStr;
    use crate::viewport::LIERO_VIEWPORT_SPECTATOR;
    use crate::worm::LieroWormData;

    fn defaults<T: Default>(n: usize) -> Vec<T> {
        (0..n).map(|_| T::default()).collect()
    }

    /// Runs `f` on a game of two worms named A and B in a TC of three
    /// weapons, with a split-screen viewport per worm and a spectator one.
    fn with_game(f: impl FnOnce(&mut LieroGameFields, &mut [LieroWormData; 2])) {
        let mut weapons: Vec<LieroTcWeapon> = defaults(3);
        for (i, (w, t)) in weapons.iter_mut().zip([0, 1, 250]).enumerate() {
            w.id = i as i32;
            w.loading_time = t;
        }
        let mut sobjects: Vec<LieroTcSObject> = defaults(2);
        sobjects[1].start_frame = 40;
        let mut nobjects: Vec<LieroTcNObject> = defaults(1);
        nobjects[0].start_frame = 30;
        let mut tc = LieroTcData::empty();
        tc.weapons = weapons;
        tc.sobjects = sobjects;
        tc.nobjects = nobjects;

        let mut data = [LieroWormData::default(), LieroWormData::default()];
        data[1].index = 1;
        let names = [b"A", b"B"];
        let worms: Vec<LieroGameWorm> = data
            .iter_mut()
            .zip(names)
            .enumerate()
            .map(|(i, (d, n))| LieroGameWorm {
                id: crate::worm::WormId(i as u32),
                data: d,
                max_health: 100,
                ai: std::ptr::null_mut(),
                name: LieroStr {
                    ptr: n.as_ptr(),
                    len: 1,
                },
                color: 10 + i as i32,
            })
            .collect();
        let rect = LieroRect {
            x1: 0,
            y1: 0,
            x2: 158,
            y2: 158,
        };
        let mut owned = [
            LieroViewport::new(rect, 0, 504, 350),
            LieroViewport::new(rect, 1, 504, 350),
            LieroViewport::new(rect, 0, 504, 350),
        ];
        owned[2].kind = LIERO_VIEWPORT_SPECTATOR;
        let [a, b, c] = owned.each_mut().map(|v| v as *mut LieroViewport);
        let (vps, spectators) = ([a, b], [c]);
        let pool: Vec<*mut LieroWormData> = worms.iter().map(|w| w.data).collect();
        let mut game = LieroGameData::new_boxed();
        let mut fields = LieroGameFields {
            worms: worms.as_ptr(),
            worm_count: 2,
            pool: pool.as_ptr(),
            pool_count: pool.len(),
            game: &mut *game,
            level: LieroLevel {
                data: std::ptr::null_mut(),
                materials: std::ptr::null_mut(),
                width: 0,
                height: 0,
            },
            tc: &*tc,
            viewports: vps.as_ptr(),
            viewport_count: 2,
            spectator_viewports: spectators.as_ptr(),
            spectator_viewport_count: 1,
            stats: std::ptr::null_mut(),
            sound: LieroSoundOut::NONE,
        };
        // The worms are reached through `fields` while `f` changes them, as
        // the tick does.
        let data_ptr: *mut [LieroWormData; 2] = &mut data;
        f(&mut fields, unsafe { &mut *data_ptr });
    }

    #[test]
    fn lists_follow_the_game() {
        with_game(|f, worms| unsafe {
            let d = &mut *f.game;
            worms[1].control_states = 1 << LIERO_CONTROL_FIRE;
            worms[0].weapons[2].kind = 2;
            for slot in [5, 2] {
                d.wobjects.arr[slot].used = true;
                d.wobjects.arr[slot].kind = 1;
            }
            d.bonuses.arr[3].used = true;
            d.bonuses.arr[3].weapon = 2;
            d.sobjects.arr[7].used = true;
            d.sobjects.arr[7].id = 1;
            d.nobjects.arr[0].used = true;
            d.nobjects.arr[0].kind = 0;
            d.bobjects.resize(4);
            d.bobjects.count = 3;
            (**f.viewports.add(1)).banner_y = 5;

            let mut settings: LieroGameSettings = std::mem::zeroed();
            settings.loading_time = 50;
            let l = lists(f, &settings, *f.viewports.add(1));

            let w = &l.worms;
            assert_eq!((w[0].name, w[1].name, w[1].color), (&b"A"[..], &b"B"[..], 11));
            assert!(w[1].pressed_fire && !w[1].pressed_change && !w[0].pressed_fire);
            assert_eq!((w[0].weapons[2].weapon, w[0].weapons[0].weapon), (2, -1));
            assert_eq!(
                l.weapons.iter().map(|w| w.computed_loading_time).collect::<Vec<_>>(),
                vec![1, 1, 125]
            );
            assert_eq!(
                l.wobjects.iter().map(|o| (o.slot, o.weapon)).collect::<Vec<_>>(),
                vec![(2, 1), (5, 1)]
            );
            assert_eq!((l.bonuses.len(), l.bonuses[0].weapon), (1, 2));
            assert_eq!((l.sobjects.len(), l.sobjects[0].start_frame), (1, 40));
            assert_eq!(l.nobjects[0].start_frame, 30);
            assert_eq!(l.bobjects.len(), 3);
            assert_eq!(l.self_index, 1);
            assert_eq!(
                l.banners.iter().map(|b| (b.worm_idx, b.banner_y)).collect::<Vec<_>>(),
                vec![(0, -8), (1, 5)]
            );
            assert_eq!(lists(f, &settings, *f.spectator_viewports).self_index, -1);
        });
    }

    #[test]
    fn viewports_are_processed_by_kind() {
        with_game(|f, worms| unsafe {
            worms[0].visible = true;
            worms[0].pos = crate::math::LieroIVec2 {
                x: 300 << 16,
                y: 200 << 16,
            };
            let spectator = &mut **f.spectator_viewports;
            let before = (spectator.x, spectator.y);
            process_viewports(f);
            let vp = &**f.viewports;
            assert_eq!(
                (vp.x + vp.center_x, vp.y + vp.center_y),
                (300, 200),
                "the worm's viewport follows it"
            );
            assert_eq!((spectator.x, spectator.y), before, "the spectator one doesn't");

            // A single-screen replay's spectator viewport in the split-screen
            // list is still a spectator one.
            let vp = &mut **f.viewports.add(1);
            vp.kind = LIERO_VIEWPORT_SPECTATOR;
            vp.worm_idx = 0;
            let before = (vp.x, vp.y);
            process_viewports(f);
            assert_eq!((vp.x, vp.y), before);
        });
    }
}
