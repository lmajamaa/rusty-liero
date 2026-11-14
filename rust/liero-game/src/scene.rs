//! The game as its viewports see it: each draw's `LieroScene`, made from
//! the game's state as C++'s `viewport.cpp` read it from its `Game`, and
//! the viewports' processing at the end of a tick.

use crate::ai::LieroWormAi;
use crate::game::{LieroGameData, LieroGameSettings};
use crate::gfx::LieroBitmap;
use crate::level::LevelView;
use crate::tc::{c, h, s, LieroTcData, LieroTcSprites};
use crate::viewport::{
    liero_viewport_draw, liero_viewport_process, LieroScene, LieroSpriteSet, LieroViewBObject, LieroViewBonus,
    LieroViewNObject, LieroViewSObject, LieroViewWObject, LieroViewWeapon, LieroViewWorm, LieroViewWormWeapon,
    LieroViewport, LieroViewportBanner,
};
use crate::worm::{LieroWormData, LIERO_CONTROL_CHANGE, LIERO_CONTROL_FIRE};

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

/// What a worm's view takes of its settings: its full health
/// (`settings->health`), its name and its colour.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WormLooks<'a> {
    pub max_health: i32,
    pub name: &'a [u8],
    pub color: i32,
}

fn sprites(s: &LieroTcSprites) -> LieroSpriteSet<'_> {
    LieroSpriteSet {
        data: s.pixels(),
        width: s.width,
        height: s.height,
        count: s.count,
    }
}

/// A worm as the viewports see it (`viewWorms`).
pub(crate) fn view_worm<'a>(
    w: &'a LieroWormData,
    looks: WormLooks<'a>,
    ai: Option<&'a LieroWormAi>,
) -> LieroViewWorm<'a> {
    LieroViewWorm {
        pos: w.pos,
        visible: w.visible,
        ready: w.ready,
        health: w.health,
        max_health: looks.max_health,
        killed_timer: w.killed_timer,
        stats_x: w.stats_x,
        kills: w.kills,
        lives: w.lives,
        timer: w.timer,
        index: w.index,
        name: looks.name,
        color: looks.color,
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
        traces: ai.map_or(&[], |ai| ai.traces()),
    }
}

/// Each viewport's worm, shake and banner after a tick
/// (`Game::processViewports`). `worms` are in the game's order.
pub(crate) fn process_viewports<'v>(worms: &[LieroViewWorm], viewports: impl Iterator<Item = &'v mut LieroViewport>) {
    for vp in viewports {
        liero_viewport_process(vp, vp.kind, worms);
    }
}

/// The game as a draw reads it.
pub(crate) struct SceneSource<'a> {
    pub data: &'a LieroGameData,
    /// In the game's order.
    pub worms: Vec<LieroViewWorm<'a>>,
    pub tc: &'a LieroTcData,
    pub level: LevelView<'a>,
    pub settings: &'a LieroGameSettings,
}

/// Draws `viewports` on `scr` (`Viewport::draw` and `SpectatorViewport::draw`,
/// by each one's kind). `split` are the split-screen viewports, whose kill
/// messages every viewport shows; `spectators` draws the spectator ones
/// instead.
pub(crate) fn draw_viewports(
    src: &SceneSource,
    draw: &LieroDrawSettings,
    split: &mut [Box<LieroViewport>],
    spectators: Option<&mut [Box<LieroViewport>]>,
    scr: &LieroBitmap,
) {
    let l = lists(src);
    let banners = banners(split);
    let scene = scene(src, &l, &banners, draw);
    match spectators {
        Some(list) => {
            for vp in list {
                liero_viewport_draw(vp, vp.kind, -1, &scene, scr);
            }
        }
        None => {
            for (i, vp) in split.iter_mut().enumerate() {
                liero_viewport_draw(vp, vp.kind, i as i32, &scene, scr);
            }
        }
    }
}

/// The split-screen viewports' kill messages.
fn banners(split: &[Box<LieroViewport>]) -> Vec<LieroViewportBanner> {
    split
        .iter()
        .map(|v| LieroViewportBanner {
            worm_idx: v.worm_idx,
            banner_y: v.banner_y,
        })
        .collect()
}

fn scene<'a>(
    src: &'a SceneSource<'a>,
    l: &'a Lists<'a>,
    banners: &'a [LieroViewportBanner],
    draw: &LieroDrawSettings,
) -> LieroScene<'a> {
    let (tc, d, settings) = (src.tc, src.data, src.settings);
    LieroScene {
        worms: &src.worms,
        weapons: &l.weapons,
        bonuses: &l.bonuses,
        sobjects: &l.sobjects,
        wobjects: &l.wobjects,
        nobjects: &l.nobjects,
        bobjects: &l.bobjects,
        viewports: banners,

        small_sprites: sprites(&tc.small_sprites),
        large_sprites: sprites(&tc.large_sprites),
        worm_sprites: sprites(&tc.worm_sprites),
        fire_cone_sprites: sprites(&tc.fire_cone_sprites),
        text_sprites: sprites(&tc.text_sprites),
        font: &tc.font,
        materials: &tc.materials,
        level: src.level,
        bonus_frames: tc.model.bonus_frames,

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
    }
}

/// The object lists a viewport draws, in C++'s order, which decides what
/// is drawn on top.
struct Lists<'a> {
    weapons: Vec<LieroViewWeapon<'a>>,
    bonuses: Vec<LieroViewBonus>,
    sobjects: Vec<LieroViewSObject>,
    wobjects: Vec<LieroViewWObject>,
    nobjects: Vec<LieroViewNObject>,
    bobjects: Vec<LieroViewBObject>,
}

fn lists<'a>(src: &SceneSource<'a>) -> Lists<'a> {
    let (tc, d, settings) = (src.tc, src.data, src.settings);
    let weapons: Vec<LieroViewWeapon> = (tc.model.weapons.iter())
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
        .map(|o| LieroViewSObject {
            x: o.x,
            y: o.y,
            cur_frame: o.cur_frame,
            start_frame: tc.model.sobjects[o.id as usize].start_frame,
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
            start_frame: tc.model.nobjects[o.kind as usize].start_frame,
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
    Lists {
        weapons,
        bonuses,
        sobjects,
        wobjects,
        nobjects,
        bobjects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::LieroRect;
    use crate::viewport::LIERO_VIEWPORT_SPECTATOR;
    use liero_data::model::{NObject, SObject, Weapon};

    fn defaults<T: Default>(n: usize) -> Vec<T> {
        (0..n).map(|_| T::default()).collect()
    }

    /// A TC of three weapons, two sobject types and one nobject type.
    fn tc() -> Box<LieroTcData> {
        let mut weapons: Vec<Weapon> = defaults(3);
        for (w, t) in weapons.iter_mut().zip([0, 1, 250]) {
            w.loading_time = t;
        }
        let mut sobjects: Vec<SObject> = defaults(2);
        sobjects[1].start_frame = 40;
        let mut nobjects: Vec<NObject> = defaults(1);
        nobjects[0].start_frame = 30;
        let mut tc = LieroTcData::empty();
        tc.model.weapons = weapons;
        tc.model.sobjects = sobjects;
        tc.model.nobjects = nobjects;
        tc
    }

    /// Worms named A and B, of colours 10 and 11.
    fn views(worms: &[LieroWormData]) -> Vec<LieroViewWorm<'_>> {
        worms
            .iter()
            .zip([&b"A"[..], b"B"])
            .enumerate()
            .map(|(i, (w, name))| {
                let looks = WormLooks {
                    max_health: 100,
                    name,
                    color: 10 + i as i32,
                };
                view_worm(w, looks, None)
            })
            .collect()
    }

    fn two_worms() -> [LieroWormData; 2] {
        let mut worms = [LieroWormData::default(), LieroWormData::default()];
        worms[1].index = 1;
        worms
    }

    fn viewport(worm_idx: i32) -> Box<LieroViewport> {
        let rect = LieroRect {
            x1: 0,
            y1: 0,
            x2: 158,
            y2: 158,
        };
        Box::new(LieroViewport::new(rect, worm_idx, 504, 350))
    }

    #[test]
    fn lists_follow_the_game() {
        let tc = tc();
        let mut d = LieroGameData::new_boxed();
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
        let mut worms = two_worms();
        worms[1].control_states = 1 << LIERO_CONTROL_FIRE;
        worms[0].weapons[2].kind = 2;
        let data = crate::settings::LieroSettingsBox::defaults().borrow().data;
        let mut settings = crate::game_box::game_settings(&data, None);
        settings.loading_time = 50;
        let src = SceneSource {
            data: &d,
            worms: views(&worms),
            tc: &tc,
            level: LevelView {
                data: &[],
                width: 0,
                height: 0,
            },
            settings: &settings,
        };
        let l = lists(&src);

        let w = &src.worms;
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

        let mut split = [viewport(0), viewport(1)];
        split[1].banner_y = 5;
        assert_eq!(
            banners(&split)
                .iter()
                .map(|b| (b.worm_idx, b.banner_y))
                .collect::<Vec<_>>(),
            vec![(0, -8), (1, 5)]
        );
    }

    #[test]
    fn viewports_are_processed_by_kind() {
        let mut worms = two_worms();
        worms[0].visible = true;
        worms[0].pos = crate::math::LieroIVec2 {
            x: 300 << 16,
            y: 200 << 16,
        };
        let mut vps = [viewport(0), viewport(1), viewport(0)];
        vps[2].kind = LIERO_VIEWPORT_SPECTATOR;
        let before = (vps[2].x, vps[2].y);
        process_viewports(&views(&worms), vps.iter_mut().map(|v| &mut **v));
        assert_eq!(
            (vps[0].x + vps[0].center_x, vps[0].y + vps[0].center_y),
            (300, 200),
            "the worm's viewport follows it"
        );
        assert_eq!((vps[2].x, vps[2].y), before, "the spectator one doesn't");

        // A single-screen replay's spectator viewport in the split-screen
        // list is still a spectator one.
        vps[1].kind = LIERO_VIEWPORT_SPECTATOR;
        vps[1].worm_idx = 0;
        let before = (vps[1].x, vps[1].y);
        process_viewports(&views(&worms), vps.iter_mut().map(|v| &mut **v));
        assert_eq!((vps[1].x, vps[1].y), before);
    }
}
