//! The game's object lists: bonuses, weapon objects (`WObject`), special
//! objects (`SObject`: explosions and smoke), non-weapon objects (`NObject`:
//! splinters and the like) and blood. Rust owns them, in `LieroGameData`.
//!
//! The four fixed lists are laid out like C++'s `ExactObjectList<T, N>`
//! (`src/game/exactObjectList.hpp`): `N` slots and a sentinel slot that is
//! always used, a bitmask of free slots, and a count. C++'s list code works
//! on them in place. The blood list, C++'s `BObjectList`, is a pointer to
//! `limit` particles of which the first `count` are in use.
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
#[repr(C)]
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
#[repr(C)]
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
#[repr(C)]
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
#[repr(C)]
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
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroBonusList {
    pub arr: [LieroBonus; LIERO_MAX_BONUSES],
    pub sentinel: LieroBonus,
    /// A set bit is a free slot.
    pub free_list: [u32; 4],
    pub count: usize,
}

/// `ExactObjectList<WObject, 600>`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroWObjectList {
    pub arr: [LieroWObject; LIERO_MAX_WOBJECTS],
    pub sentinel: LieroWObject,
    pub free_list: [u32; 19],
    pub count: usize,
}

/// `ExactObjectList<SObject, 700>`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LieroSObjectList {
    pub arr: [LieroSObject; LIERO_MAX_SOBJECTS],
    pub sentinel: LieroSObject,
    pub free_list: [u32; 22],
    pub count: usize,
}

/// `ExactObjectList<NObject, 600>`.
#[repr(C)]
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

/// The blood particles, C++'s `BObjectList`: `arr` holds `limit`
/// particles, and the first `count` are in use.
#[repr(C)]
pub struct LieroBObjectList {
    pub arr: *mut LieroBObject,
    pub limit: usize,
    pub count: usize,
}

impl LieroBObjectList {
    /// A list of `limit` particles, none in use.
    pub(crate) fn new(limit: usize) -> Self {
        Self::from_vec(vec![LieroBObject::default(); limit], 0)
    }

    fn from_vec(v: Vec<LieroBObject>, count: usize) -> Self {
        let limit = v.len();
        LieroBObjectList { arr: Box::into_raw(v.into_boxed_slice()) as *mut LieroBObject, limit, count }
    }

    fn slice(&self) -> &[LieroBObject] {
        // SAFETY: `arr` holds `limit` particles.
        unsafe { std::slice::from_raw_parts(self.arr, self.limit) }
    }

    /// `FastObjectList::resize`: keeps the first particles, and new ones
    /// are zero.
    pub(crate) fn resize(&mut self, limit: usize) {
        let mut v = self.slice().to_vec();
        v.resize(limit, LieroBObject::default());
        // Dropping the old list frees its particles.
        *self = Self::from_vec(v, self.count.min(limit));
    }
}

impl Clone for LieroBObjectList {
    fn clone(&self) -> Self {
        Self::from_vec(self.slice().to_vec(), self.count)
    }
}

impl Drop for LieroBObjectList {
    fn drop(&mut self) {
        // SAFETY: `arr` came from a boxed slice of `limit` particles.
        unsafe { drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(self.arr, self.limit))) }
    }
}

impl std::fmt::Debug for LieroBObjectList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LieroBObjectList")
            .field("arr", &self.slice())
            .field("limit", &self.limit)
            .field("count", &self.count)
            .finish()
    }
}

/// `BObjectList::resize`.
///
/// # Safety
/// `list` is valid, and no pointer into it is used after.
#[no_mangle]
pub unsafe extern "C" fn liero_bobject_list_resize(list: &mut LieroBObjectList, limit: usize) {
    list.resize(limit);
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
    fn blood_list_resizes_and_copies() {
        let mut l = LieroBObjectList::new(1);
        assert_eq!((l.limit, l.count), (1, 0));
        l.resize(4);
        unsafe { (*l.arr.add(2)).color = 7 };
        l.count = 3;
        let c = l.clone();
        assert_eq!(format!("{c:?}"), format!("{l:?}"));
        l.resize(2);
        assert_eq!((l.limit, l.count), (2, 2));
        assert_eq!(unsafe { (*c.arr.add(2)).color }, 7);
        l.resize(5);
        assert_eq!(unsafe { (*l.arr.add(4)).color }, 0);
    }
}
