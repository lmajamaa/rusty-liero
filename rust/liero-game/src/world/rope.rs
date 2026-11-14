//! The ninja rope (`ninjarope.cpp`).

use super::*;

impl World<'_> {
    /// `Ninjarope::process` for worm `i`'s rope: the position in the game's
    /// worms.
    ///
    /// The rope is changed through a copy, written back at the end: what
    /// this calls (finding the worm it hits, and making splinters) doesn't
    /// read or change ropes.
    pub(super) fn process_rope(&mut self, i: i32) {
        let k = |name| self.constant(c(name));
        let (force_shl_x, force_div_x) = (k("NRForceShlX"), k("NRForceDivX"));
        let (force_shl_y, force_div_y) = (k("NRForceShlY"), k("NRForceDivY"));
        let (force_len_shl, attach_length) = (k("NRForceLenShl"), k("NRAttachLength"));
        let gravity = k("NinjaropeGravity");
        let (width, height) = (self.g.level.width, self.g.level.height);

        let mut r = self.worm(i).ninjarope;

        if !r.out {
            return;
        }

        r.pos += r.vel;
        let ipos = r.pos.to_int();

        // The first worm other than the owner that the rope hits.
        r.anchor = (0..self.worm_count())
            .find(|&w| w != i && self.spec_worm_hit(ipos.x, ipos.y, 1, w))
            .map(|w| self.g.worm_id(w));

        let diff = r.pos - self.worm(i).pos;
        let force = FVec2 {
            x: Fixed(diff.x.0.wrapping_shl(force_shl_x as u32)) / force_div_x,
            y: Fixed(diff.y.0.wrapping_shl(force_shl_y as u32)) / force_div_y,
        };

        r.cur_len = (vector_length(diff.x.to_int(), diff.y.to_int()) + 1).wrapping_shl(force_len_shl as u32);

        let hits_terrain = ipos.x <= 0
            || ipos.x >= width - 1
            || ipos.y <= 0
            || ipos.y >= height - 1
            || self.g.level.mat(ipos.x, ipos.y).dirt_rock();

        if hits_terrain {
            if !r.attached {
                r.length = attach_length;
                r.attached = true;

                if inside(&self.g.level, ipos.x, ipos.y) {
                    let any_dirt = self.g.level.mat(ipos.x, ipos.y).any_dirt();
                    let pix = self.g.level.pixel(ipos.x, ipos.y);
                    if any_dirt {
                        // TODO in C++: check 11 and read it from the EXE.
                        for _ in 0..11 {
                            let angle = self.rand(128) as i32;
                            let index = self.worm(i).index;
                            self.create_nobject2(
                                2,
                                angle,
                                FVec2::default(),
                                r.pos,
                                i32::from(pix),
                                index,
                                FiredBy::NONE,
                            );
                        }
                    }
                }
            }

            r.vel = FVec2::default();
        } else if let Some(anchor) = r.anchor {
            if !r.attached {
                // TODO in C++: should this be separate from attaching to terrain?
                r.length = attach_length;
                r.attached = true;
            }

            // Another worm than the owner: `find` above skips it.
            let anchor = self.g.pool_worm_mut(anchor);
            if r.cur_len > r.length {
                anchor.vel -= force / r.cur_len;
            }

            r.vel = anchor.vel;
            r.pos = anchor.pos;
        } else {
            r.attached = false;
        }

        let owner = self.g.worm_mut(i);
        if r.attached {
            // cur_len can't be 0.
            if r.cur_len > r.length {
                owner.vel += force / r.cur_len;
            }
        } else {
            r.vel.y += Fixed(gravity);

            if r.cur_len > r.length {
                r.vel -= force / r.cur_len;
            }
        }
        owner.ninjarope = r;
    }
}
