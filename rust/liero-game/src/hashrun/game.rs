//! The hash of a game's whole state (`hashGame` in `src/game/hashrun.cpp`),
//! which the bot games, replays, AI games and sound games print.

use super::Fnv;
use crate::controller::State;
use crate::game_box::{Game, Worm};
use crate::math::FVec2;
use crate::renderer::Renderer;
use crate::stats::{Heatmap, Stats};

impl Fnv {
    fn vec(&mut self, v: FVec2) {
        let [x, y] = v.raw();
        self.i32(x);
        self.i32(y);
    }
}

/// The used slots of an `ExactObjectList`, in the order they're processed.
fn used<T>(arr: &[T], is_used: impl Fn(&T) -> bool) -> impl Iterator<Item = &T> {
    arr.iter().filter(move |o| is_used(o))
}

/// `anchor_index` is the `index` of the worm its rope is anchored to, or -1.
fn hash_worm(f: &mut Fnv, w: &Worm, anchor_index: i32) {
    let w = &w.data;
    f.vec(w.pos);
    f.vec(w.vel);
    f.i32(w.logic_respawn.x);
    f.i32(w.logic_respawn.y);
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
    f.i32(anchor_index);
    f.vec(r.pos);
    f.vec(r.vel);
    f.i32(r.length);
    f.i32(r.cur_len);

    f.i32(w.current_weapon);
    f.i32(w.last_killed_by_idx.unwrap_or(-1));
    f.i32(w.fire_cone);
    f.i32(w.leave_shell_timer);
    w.reacts.iter().for_each(|&r| f.i32(r));

    for ww in &w.weapons {
        f.i32(ww.ammo);
        f.i32(ww.delay_left);
        f.i32(ww.loading_left);
        f.i32(ww.kind);
    }

    f.i32(w.direction);
    f.u32(w.control_states);
    f.u32(w.prev_control_states);
}

pub fn hash_game(game: &mut Game) -> u64 {
    let mut f = Fnv::default();
    let g = game.data_mut();
    f.i32(g.cycles);
    f.u32(g.rand.x);
    f.u32(g.rand.c);
    f.i32(g.screen_flash);
    f.i32(g.last_killed_idx.unwrap_or(-1));
    f.bool(g.got_changed);

    let z = &g.holdazone;
    for v in [
        z.rect.x1,
        z.rect.y1,
        z.rect.x2,
        z.rect.y2,
        z.holder_idx.unwrap_or(-1),
        z.contender_idx.unwrap_or(-1),
        z.contender_frames,
        z.timeout_left,
        z.zone_width,
        z.zone_height,
    ] {
        f.i32(v);
    }

    for i in 0..game.worm_count() {
        let w = game.worm_at(i).expect("an empty place among the game's worms");
        let anchor = w.data.ninjarope.anchor;
        let anchor_index = anchor.map_or(-1, |a| game.worm_mut(a).data.index);
        hash_worm(&mut f, game.worm_at(i).unwrap(), anchor_index);
    }

    // Each list in processing order, since the order changes the outcome.
    let g = game.data_mut();
    f.u32(g.bonuses.count as u32);
    for b in used(&g.bonuses.arr[..], |b| b.used) {
        for v in [b.x, b.y, b.vel_y, b.frame, b.timer, b.weapon] {
            f.i32(v);
        }
    }

    f.u32(g.sobjects.count as u32);
    for s in used(&g.sobjects.arr[..], |s| s.used) {
        for v in [s.x, s.y, s.id, s.cur_frame, s.anim_delay] {
            f.i32(v);
        }
    }

    f.u32(g.wobjects.count as u32);
    for o in used(&g.wobjects.arr[..], |o| o.used) {
        f.vec(o.pos);
        f.vec(o.vel);
        f.i32(o.kind);
        for v in [o.owner_idx, o.cur_frame, o.time_left] {
            f.i32(v);
        }
    }

    f.u32(g.nobjects.count as u32);
    for o in used(&g.nobjects.arr[..], |o| o.used) {
        f.vec(o.pos);
        f.vec(o.vel);
        f.i32(o.kind);
        for v in [o.owner_idx, o.cur_frame, o.time_left] {
            f.i32(v);
        }
    }

    let b = &g.bobjects;
    f.u32(b.count as u32);
    for p in b.used() {
        f.vec(p.pos);
        f.vec(p.vel);
        f.i32(p.color);
    }

    f.bytes(game.level_box().pixels());
    f.0
}

/// A screen's pixels, row by row, and its palette (`hashFrame`).
pub(super) fn add_screen(f: &mut Fnv, r: &Renderer) {
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
pub fn hash_frame(game: &mut Game, split: &mut Renderer, spectator: &mut Renderer) -> u64 {
    let mut f = Fnv::default();
    for (r, spectators) in [(split, false), (spectator, true)] {
        r.clear();
        game.draw(r, State::Game, spectators, false);
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
pub fn hash_stats(s: &Stats) -> u64 {
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
