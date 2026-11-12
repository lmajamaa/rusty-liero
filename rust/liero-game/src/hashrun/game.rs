//! The hash of a game's whole state (`hashGame` in `src/game/hashrun.cpp`),
//! which the bot games, replays, AI games and sound games print.

use super::Fnv;
use crate::controller::LIERO_STATE_GAME;
use crate::game_box::{LieroGame, LieroWorm};
use crate::math::LieroIVec2;
use crate::renderer::LieroRenderer;
use crate::stats::{Heatmap, LieroStats};
use crate::tc::{LieroTcNObject, LieroTcWeapon};
use crate::worm::LieroWormData;

impl Fnv {
    fn vec(&mut self, v: LieroIVec2) {
        self.i32(v.x);
        self.i32(v.y);
    }
}

/// The used slots of an `ExactObjectList`, in the order they're processed.
fn used<T>(arr: &[T], is_used: impl Fn(&T) -> bool) -> impl Iterator<Item = &T> {
    arr.iter().filter(move |o| is_used(o))
}

fn hash_worm(f: &mut Fnv, w: &LieroWorm) {
    let w = &w.data;
    f.vec(w.pos);
    f.vec(w.vel);
    f.vec(w.logic_respawn);
    f.i32(w.hotspot_x);
    f.i32(w.hotspot_y);
    f.i32(w.aiming_angle);
    f.i32(w.aiming_speed);
    for b in [
        w.able_to_jump,
        w.able_to_dig,
        w.key_change_pressed,
        w.movable,
        w.animate,
        w.visible,
        w.ready,
        w.flag,
        w.make_sight_green,
    ] {
        f.bool(b);
    }
    for v in [
        w.health,
        w.lives,
        w.kills,
        w.timer,
        w.killed_timer,
        w.current_frame,
        w.flags,
    ] {
        f.i32(v);
    }

    let r = &w.ninjarope;
    f.bool(r.out);
    f.bool(r.attached);
    // SAFETY: an anchor is one of the game's worms.
    f.i32(unsafe { r.anchor.cast::<LieroWormData>().as_ref() }.map_or(-1, |a| a.index));
    f.vec(r.pos);
    f.vec(r.vel);
    f.i32(r.length);
    f.i32(r.cur_len);

    f.i32(w.current_weapon);
    f.i32(w.last_killed_by_idx);
    f.i32(w.fire_cone);
    f.i32(w.leave_shell_timer);
    w.reacts.iter().for_each(|&r| f.i32(r));

    for ww in &w.weapons {
        f.i32(ww.ammo);
        f.i32(ww.delay_left);
        f.i32(ww.loading_left);
        // SAFETY: a slot's weapon is the TC's.
        f.i32(unsafe { ww.kind.cast::<LieroTcWeapon>().as_ref() }.map_or(-1, |k| k.id));
    }

    f.i32(w.direction);
    f.u32(w.control_states);
    f.u32(w.prev_control_states);
}

pub fn hash_game(game: &mut LieroGame) -> u64 {
    let mut f = Fnv::default();
    let g = game.data_mut();
    f.i32(g.cycles);
    f.u32(g.rand.x);
    f.u32(g.rand.c);
    f.i32(g.screen_flash);
    f.i32(g.last_killed_idx);
    f.bool(g.got_changed);

    let z = &g.holdazone;
    for v in [
        z.rect.x1,
        z.rect.y1,
        z.rect.x2,
        z.rect.y2,
        z.holder_idx,
        z.contender_idx,
        z.contender_frames,
        z.timeout_left,
        z.zone_width,
        z.zone_height,
    ] {
        f.i32(v);
    }

    for w in game.listed_worms() {
        hash_worm(&mut f, w.expect("an empty place among the game's worms"));
    }

    // Each list in processing order, since the order changes the outcome.
    let g = game.data_mut();
    f.u32(g.bonuses.count as u32);
    for b in used(&g.bonuses.arr, |b| b.used) {
        for v in [b.x, b.y, b.vel_y, b.frame, b.timer, b.weapon] {
            f.i32(v);
        }
    }

    f.u32(g.sobjects.count as u32);
    for s in used(&g.sobjects.arr, |s| s.used) {
        for v in [s.x, s.y, s.id, s.cur_frame, s.anim_delay] {
            f.i32(v);
        }
    }

    f.u32(g.wobjects.count as u32);
    for o in used(&g.wobjects.arr, |o| o.used) {
        f.vec(o.pos);
        f.vec(o.vel);
        // SAFETY: a weapon object's weapon is the TC's.
        f.i32(unsafe { &*o.kind.cast::<LieroTcWeapon>() }.id);
        for v in [o.owner_idx, o.cur_frame, o.time_left] {
            f.i32(v);
        }
    }

    f.u32(g.nobjects.count as u32);
    for o in used(&g.nobjects.arr, |o| o.used) {
        f.vec(o.pos);
        f.vec(o.vel);
        // SAFETY: an object's type is the TC's.
        f.i32(unsafe { &*o.kind.cast::<LieroTcNObject>() }.id);
        for v in [o.owner_idx, o.cur_frame, o.time_left] {
            f.i32(v);
        }
    }

    let b = &g.bobjects;
    f.u32(b.count as u32);
    // SAFETY: the list holds `limit` particles, and the first `count` are
    // in use.
    for p in unsafe { std::slice::from_raw_parts(b.arr, b.count) } {
        f.vec(p.pos);
        f.vec(p.vel);
        f.i32(p.color);
    }

    f.bytes(game.level_box().pixels());
    f.0
}

/// A screen's pixels, row by row, and its palette (`hashFrame`).
pub(super) fn add_screen(f: &mut Fnv, r: &LieroRenderer) {
    let v = r.bmp.view();
    let (w, pitch) = (v.w as usize, v.pitch as usize);
    let pixels = r.bmp.pixels();
    for y in 0..v.h as usize {
        f.bytes(&pixels[y * pitch..y * pitch + w]);
    }
    for c in &r.data.pal {
        for x in [c.r, c.g, c.b] {
            f.u32(u32::from(x));
        }
    }
}

/// The game as the split-screen and spectator windows would show it.
pub fn hash_frame(game: &mut LieroGame, split: &mut LieroRenderer, spectator: &mut LieroRenderer) -> u64 {
    let mut f = Fnv::default();
    for (r, spectators) in [(split, false), (spectator, true)] {
        r.clear();
        // SAFETY: the renderer is a live one.
        unsafe { game.draw(r, LIERO_STATE_GAME, spectators, false) };
        add_screen(&mut f, r);
    }
    f.0
}

fn add_heatmap(f: &mut Fnv, hm: &Heatmap) {
    for v in [hm.width, hm.height, hm.org_width, hm.org_height] {
        f.i32(v);
    }
    hm.map.iter().for_each(|&v| f.i32(v));
}

/// What the stats recorder has, but the process times, which are measured
/// with the clock (`hashStats`).
pub fn hash_stats(s: &LieroStats) -> u64 {
    let mut f = Fnv::default();
    f.i32(s.frame);
    f.i32(s.game_time);
    add_heatmap(&mut f, &s.presence);
    for w in &s.worms {
        f.u32(w.life_spans.len() as u32);
        for l in &w.life_spans {
            f.i32(l[0]);
            f.i32(l[1]);
        }
        for ws in &w.weapons {
            for v in [
                ws.potential_hp,
                ws.actual_hp,
                ws.potential_hits,
                ws.actual_hits,
                ws.total_hp,
                ws.index,
            ] {
                f.i32(v);
            }
        }
        for v in [w.damage, w.damage_dealt, w.self_damage] {
            f.i32(v);
        }
        add_heatmap(&mut f, &w.damage_hm);
        add_heatmap(&mut f, &w.presence);
        f.i32(w.weapon_change_good);
        f.i32(w.weapon_change_bad);
        f.u32(w.frame_stats.len() as u32);
        for fs in &w.frame_stats {
            f.i32(fs.damage);
            f.i32(fs.total_hp);
        }
        for v in [w.spawn_time, w.index, w.lives, w.timer, w.kills] {
            f.i32(v);
        }
    }
    f.0
}
