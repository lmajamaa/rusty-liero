//! The ninja rope (`ninjarope.cpp`).

use super::*;

impl World<'_> {
    /// `Ninjarope::process` for worm `i`'s rope: the position in the game's
    /// worms.
    pub(super) fn process_rope(&self, i: i32) {
        let k = |name| self.constant(c(name));
        let (force_shl_x, force_div_x) = (k("NRForceShlX"), k("NRForceDivX"));
        let (force_shl_y, force_div_y) = (k("NRForceShlY"), k("NRForceDivY"));
        let (force_len_shl, attach_length) = (k("NRForceLenShl"), k("NRAttachLength"));
        let gravity = k("NinjaropeGravity");
        let level = self.g.f.level;

        // SAFETY: the worms are valid for the tick. Making splinters
        // touches only the object lists and the random numbers.
        unsafe {
            let owner = self.worm(i);
            let r = addr_of_mut!((*owner).ninjarope);

            if !(*r).out {
                return;
            }

            (*r).pos = add((*r).pos, (*r).vel);
            let ipos = ftoi_vec((*r).pos);

            // The first worm other than the owner that the rope hits.
            (*r).anchor = (0..self.worm_count())
                .find(|&w| w != i && self.spec_worm_hit(ipos.x, ipos.y, 1, w))
                .map(|w| self.g.worm(w).id);

            let diff = sub((*r).pos, (*owner).pos);
            let force = LieroIVec2 {
                x: diff.x.wrapping_shl(force_shl_x as u32) / force_div_x,
                y: diff.y.wrapping_shl(force_shl_y as u32) / force_div_y,
            };

            (*r).cur_len = (vector_length(ftoi(diff.x), ftoi(diff.y)) + 1).wrapping_shl(force_len_shl as u32);

            let hits_terrain = ipos.x <= 0
                || ipos.x >= level.width - 1
                || ipos.y <= 0
                || ipos.y >= level.height - 1
                || level.get().mat(ipos.x, ipos.y).dirt_rock();

            if hits_terrain {
                if !(*r).attached {
                    (*r).length = attach_length;
                    (*r).attached = true;

                    if inside(&level, ipos.x, ipos.y) {
                        let (any_dirt, pix) = {
                            let lv = level.get();
                            (lv.mat(ipos.x, ipos.y).any_dirt(), lv.pixel(ipos.x, ipos.y))
                        };
                        if any_dirt {
                            // TODO in C++: check 11 and read it from the EXE.
                            for _ in 0..11 {
                                let angle = self.rand(128) as i32;
                                let index = (*owner).index;
                                self.create_nobject2(
                                    2,
                                    angle,
                                    LieroIVec2::default(),
                                    (*r).pos,
                                    pix as i32,
                                    index,
                                    null_mut(),
                                );
                            }
                        }
                    }
                }

                (*r).vel = LieroIVec2::default();
            } else if let Some(anchor) = (*r).anchor {
                if !(*r).attached {
                    // TODO in C++: should this be separate from attaching to terrain?
                    (*r).length = attach_length;
                    (*r).attached = true;
                }

                let anchor = self.pool_worm(anchor);
                if (*r).cur_len > (*r).length {
                    (*anchor).vel = sub((*anchor).vel, div(force, (*r).cur_len));
                }

                (*r).vel = (*anchor).vel;
                (*r).pos = (*anchor).pos;
            } else {
                (*r).attached = false;
            }

            if (*r).attached {
                // cur_len can't be 0.
                if (*r).cur_len > (*r).length {
                    (*owner).vel = add((*owner).vel, div(force, (*r).cur_len));
                }
            } else {
                (*r).vel.y = (*r).vel.y.wrapping_add(gravity);

                if (*r).cur_len > (*r).length {
                    (*r).vel = sub((*r).vel, div(force, (*r).cur_len));
                }
            }
        }
    }
}
