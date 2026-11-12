//! The game's object lists: bonuses, weapon objects (`WObject`), special
//! objects (`SObject`: explosions and smoke), non-weapon objects (`NObject`:
//! splinters and the like) and blood. Rust owns them, in `LieroGameData`.
//!
//! The four fixed lists work like C++'s `ExactObjectList<T, N>`
//! (`src/game/exactObjectList.hpp`): `N` slots and a sentinel slot that is
//! always used, a bitmask of free slots, and a count, with slots handed out
//! and walked by index. The blood list, C++'s `BObjectList`, is a `Vec` of
//! particles of which the first `count` are in use.
//!
//! The lists are 74 KB in all, more than the web build's 64 KB stack, so
//! they are only ever made and copied on the heap.

use std::ffi::c_void;

use crate::math::LieroIVec2;
use crate::object::LieroBObject;
use crate::weapon::LieroWormWeapon;

/// List sizes, as in `Game`'s typedefs.
pub const LIERO_MAX_BONUSES: usize = 99;
pub const LIERO_MAX_WOBJECTS: usize = 600;
pub const LIERO_MAX_SOBJECTS: usize = 700;
pub const LIERO_MAX_NOBJECTS: usize = 600;

/// A bonus, laid out like C++'s `Bonus`.
#[derive(Clone, Copy, Debug)]
pub struct LieroBonus {
    pub used: bool,
    pub x: i32,
    pub y: i32,
    pub vel_y: i32,
    /// 0 is a weapon, 1 health; -1 before the slot's first bonus.
    pub frame: i32,
    pub timer: i32,
    pub weapon: i32,
}

/// A special object, laid out like C++'s `SObject`.
#[derive(Clone, Copy, Debug)]
pub struct LieroSObject {
    pub used: bool,
    pub x: i32,
    pub y: i32,
    /// The type.
    pub id: i32,
    pub cur_frame: i32,
    pub anim_delay: i32,
}

/// A weapon object, laid out like C++'s `WObject`.
#[derive(Clone, Copy, Debug)]
pub struct LieroWObject {
    pub used: bool,
    pub pos: LieroIVec2,
    pub vel: LieroIVec2,
    /// The C++ `Weapon`.
    pub kind: *const c_void,
    pub owner_idx: i32,
    pub cur_frame: i32,
    pub time_left: i32,
    /// The weapon slot that fired it, for stats.
    pub fired_by: *mut LieroWormWeapon,
    pub has_hit: bool,
}

/// A non-weapon object, laid out like C++'s `NObject`.
#[derive(Clone, Copy, Debug)]
pub struct LieroNObject {
    pub used: bool,
    pub pos: LieroIVec2,
    pub vel: LieroIVec2,
    pub time_left: i32,
    /// The C++ `NObjectType`.
    pub kind: *const c_void,
    pub owner_idx: i32,
    pub cur_frame: i32,
    /// The weapon slot that fired it, for stats.
    pub fired_by: *mut LieroWormWeapon,
    pub has_hit: bool,
}

/// `ExactObjectList<Bonus, 99>`.
#[derive(Clone, Copy, Debug)]
pub struct LieroBonusList {
    pub arr: [LieroBonus; LIERO_MAX_BONUSES],
    pub sentinel: LieroBonus,
    /// A set bit is a free slot.
    pub free_list: [u32; 4],
    pub count: usize,
}

/// `ExactObjectList<WObject, 600>`.
#[derive(Clone, Copy, Debug)]
pub struct LieroWObjectList {
    pub arr: [LieroWObject; LIERO_MAX_WOBJECTS],
    pub sentinel: LieroWObject,
    pub free_list: [u32; 19],
    pub count: usize,
}

/// `ExactObjectList<SObject, 700>`.
#[derive(Clone, Copy, Debug)]
pub struct LieroSObjectList {
    pub arr: [LieroSObject; LIERO_MAX_SOBJECTS],
    pub sentinel: LieroSObject,
    pub free_list: [u32; 22],
    pub count: usize,
}

/// `ExactObjectList<NObject, 600>`.
#[derive(Clone, Copy, Debug)]
pub struct LieroNObjectList {
    pub arr: [LieroNObject; LIERO_MAX_NOBJECTS],
    pub sentinel: LieroNObject,
    pub free_list: [u32; 19],
    pub count: usize,
}

/// A zeroed `T`, made on the heap and never on the stack.
pub(crate) fn zeroed_box<T>() -> Box<std::mem::MaybeUninit<T>> {
    let layout = std::alloc::Layout::new::<T>();
    // SAFETY: the layout isn't zero-sized, as the lists aren't, and the
    // memory is from the global allocator with T's layout.
    unsafe {
        let p = std::alloc::alloc_zeroed(layout);
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Box::from_raw(p.cast())
    }
}

/// What C++'s `ExactObjectList::clear` does, on a list that is all zeros.
macro_rules! clear_list {
    ($($list:ty),*) => {
        $(
            impl $list {
                pub(crate) fn clear(&mut self) {
                    self.free_list.fill(!0);
                    self.count = 0;
                    for o in self.arr.iter_mut() {
                        o.used = false;
                    }
                    self.sentinel.used = true;
                    // The bits past the last slot are never free.
                    for i in self.arr.len()..self.free_list.len() * 32 {
                        self.free_list[i >> 5] &= !(1u32 << (i & 31));
                    }
                }

                /// The slots in use.
                #[allow(dead_code)]
                pub(crate) fn used(&self) -> usize {
                    self.arr.iter().filter(|o| o.used).count()
                }
            }
        )*
    };
}

clear_list!(LieroBonusList, LieroWObjectList, LieroSObjectList, LieroNObjectList);

/// A list `Cursor` can walk: slots by index, with the sentinel, which is
/// always used, at `LIMIT`.
pub(crate) trait ExactList {
    const LIMIT: usize;
    fn is_used(&self, i: usize) -> bool;
}

/// What C++'s `ExactObjectList` does to its slots, by index.
macro_rules! exact_list {
    ($list:ty, $limit:expr) => {
        impl ExactList for $list {
            const LIMIT: usize = $limit;

            fn is_used(&self, i: usize) -> bool {
                if i == $limit {
                    self.sentinel.used
                } else {
                    self.arr[i].used
                }
            }
        }

        // Not every list uses every function.
        #[allow(dead_code)]
        impl $list {
            /// The lowest free slot, now used.
            fn get_free_object(&mut self) -> usize {
                assert!(self.count < $limit);
                self.count += 1;
                let (word, bits) = self
                    .free_list
                    .iter()
                    .enumerate()
                    .find(|(_, &f)| f != 0)
                    .expect("a free slot");
                let bit = bits.trailing_zeros() as usize;
                self.free_list[word] &= !(1u32 << bit);
                let i = (word << 5) + bit;
                assert!(!self.arr[i].used);
                self.arr[i].used = true;
                i
            }

            /// A free slot, or the last slot when there is none.
            pub(crate) fn new_object_reuse(&mut self) -> usize {
                if self.count >= $limit {
                    $limit - 1
                } else {
                    self.get_free_object()
                }
            }

            /// A free slot, if there is one.
            pub(crate) fn new_object(&mut self) -> Option<usize> {
                (self.count < $limit).then(|| self.get_free_object())
            }

            pub(crate) fn free(&mut self, i: usize) {
                assert!(self.arr[i].used);
                self.free_list[i >> 5] |= 1u32 << (i & 31);
                self.arr[i].used = false;
                assert!(self.count > 0);
                self.count -= 1;
            }
        }
    };
}

exact_list!(LieroBonusList, LIERO_MAX_BONUSES);
exact_list!(LieroWObjectList, LIERO_MAX_WOBJECTS);
exact_list!(LieroSObjectList, LIERO_MAX_SOBJECTS);
exact_list!(LieroNObjectList, LIERO_MAX_NOBJECTS);

/// `ExactObjectList::range` as an index: the slots in use, first to last,
/// stopping at the sentinel. Each step reads the list afresh, so objects
/// freed or made while walking it are seen as C++ saw them.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Cursor {
    cur: usize,
}

impl Cursor {
    pub(crate) fn next<L: ExactList>(&mut self, l: &L) -> Option<usize> {
        while !l.is_used(self.cur) {
            self.cur += 1;
        }
        let i = self.cur;
        self.cur += 1;
        (i != L::LIMIT).then_some(i)
    }

    /// The slot `next` returned last.
    pub(crate) fn last(&self) -> usize {
        self.cur - 1
    }
}

impl LieroBonusList {
    /// Clears the list, with every bonus as C++'s `Bonus()` made it.
    pub(crate) fn init(&mut self) {
        for b in self.arr.iter_mut() {
            b.frame = -1;
            b.weapon = 0;
        }
        self.sentinel.frame = -1;
        self.clear();
    }
}

/// The blood particles, C++'s `BObjectList`: `arr` holds the list's
/// limit of particles, and the first `count` are in use.
#[derive(Clone, Debug)]
pub struct LieroBObjectList {
    pub arr: Vec<LieroBObject>,
    pub count: usize,
}

impl LieroBObjectList {
    /// A list of `limit` particles, none in use.
    pub(crate) fn new(limit: usize) -> Self {
        LieroBObjectList {
            arr: vec![LieroBObject::default(); limit],
            count: 0,
        }
    }

    /// How many particles the list holds.
    pub(crate) fn limit(&self) -> usize {
        self.arr.len()
    }

    /// The particles in use.
    pub(crate) fn used(&self) -> &[LieroBObject] {
        &self.arr[..self.count]
    }

    /// `FastObjectList::resize`: keeps the first particles, and new ones
    /// are zero.
    pub(crate) fn resize(&mut self, limit: usize) {
        self.arr.resize(limit, LieroBObject::default());
        self.count = self.count.min(limit);
    }

    /// `BloodList::newObjectReuse`: a new particle, or the last one when
    /// the list is full.
    pub(crate) fn new_object_reuse(&mut self) -> &mut LieroBObject {
        if self.count == self.limit() {
            return &mut self.arr[self.count - 1];
        }
        self.count += 1;
        &mut self.arr[self.count - 1]
    }

    /// `BloodList::free`: the last particle takes particle `i`'s place.
    pub(crate) fn free(&mut self, i: usize) {
        self.count -= 1;
        self.arr[i] = self.arr[self.count];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleared_lists_match_cpp() {
        let mut l: Box<LieroWObjectList> = unsafe { zeroed_box().assume_init() };
        l.clear();
        assert_eq!(l.count, 0);
        assert!(l.sentinel.used && l.used() == 0);
        // 600 free slots, in 19 words; the last 8 bits are never free.
        assert!(l.free_list[..18].iter().all(|&w| w == !0));
        assert_eq!(l.free_list[18], (1 << (600 - 18 * 32)) - 1);

        let mut b: Box<LieroBonusList> = unsafe { zeroed_box().assume_init() };
        b.init();
        assert!(b.arr.iter().all(|b| b.frame == -1 && b.weapon == 0 && !b.used));
        assert_eq!(b.free_list[3], (1 << (99 - 3 * 32)) - 1);
    }

    #[test]
    fn lists_hand_out_the_lowest_free_slot_and_reuse_the_last_when_full() {
        let mut l: Box<LieroSObjectList> = unsafe { zeroed_box().assume_init() };
        l.clear();
        let (a, b, c) = (l.new_object(), l.new_object(), l.new_object());
        assert_eq!((a, b, c), (Some(0), Some(1), Some(2)));

        l.free(1);
        assert_eq!(l.count, 2);
        assert_eq!(l.new_object(), Some(1));

        for _ in 3..LIERO_MAX_SOBJECTS {
            assert!(l.new_object().is_some());
        }
        assert_eq!(l.new_object(), None);
        assert_eq!(l.new_object_reuse(), LIERO_MAX_SOBJECTS - 1);
        assert_eq!(l.count, LIERO_MAX_SOBJECTS);
    }

    #[test]
    fn cursors_skip_free_slots_and_survive_freeing_the_current_one() {
        let mut l: Box<LieroBonusList> = unsafe { zeroed_box().assume_init() };
        l.init();
        for _ in 0..5 {
            l.new_object();
        }
        l.free(1);

        let mut seen = Vec::new();
        let mut r = Cursor::default();
        while let Some(i) = r.next(&*l) {
            seen.push(i);
            if i == 2 {
                l.free(r.last());
            }
        }
        assert_eq!(seen, vec![0, 2, 3, 4]);
        assert_eq!(l.count, 3);
    }

    #[test]
    fn blood_reuses_the_last_particle_when_full() {
        let mut l = LieroBObjectList::new(2);
        l.new_object_reuse().color = 1;
        l.new_object_reuse().color = 2;
        l.new_object_reuse().color = 3;
        assert_eq!(l.count, 2);
        assert_eq!((l.arr[0].color, l.arr[1].color), (1, 3));
        l.free(0);
        assert_eq!((l.count, l.arr[0].color), (1, 3));
    }

    #[test]
    fn blood_list_resizes_and_copies() {
        let mut l = LieroBObjectList::new(1);
        assert_eq!((l.limit(), l.count), (1, 0));
        l.resize(4);
        l.arr[2].color = 7;
        l.count = 3;
        let c = l.clone();
        assert_eq!(format!("{c:?}"), format!("{l:?}"));
        l.resize(2);
        assert_eq!((l.limit(), l.count), (2, 2));
        assert_eq!(c.arr[2].color, 7);
        l.resize(5);
        assert_eq!(l.arr[4].color, 0);
    }
}
