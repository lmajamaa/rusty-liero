//! Boxes that Rust and C++ share by counting references: C++ holds a raw
//! pointer and gives up its reference with the box's `free` export, and Rust
//! holds a `Shared`. The box is freed with the last reference.

use std::cell::UnsafeCell;
use std::rc::Rc;

/// A new box with one reference, for C++ to hold.
pub fn new_shared<T>(v: T) -> *mut T {
    // UnsafeCell<T> is laid out as T, and lets references to the box change
    // it.
    Rc::into_raw(Rc::new(UnsafeCell::new(v))) as *mut T
}

/// Another reference to `p`.
///
/// # Safety
/// `p` comes from `new_shared` and still has a reference.
pub unsafe fn retain_shared<T>(p: *mut T) -> *mut T {
    Rc::increment_strong_count(p as *const UnsafeCell<T>);
    p
}

/// Gives up one reference to `p`, freeing it with the last.
///
/// # Safety
/// `p` comes from `new_shared`, and this reference isn't used again.
pub unsafe fn release_shared<T>(p: *mut T) {
    Rc::decrement_strong_count(p as *const UnsafeCell<T>);
}

/// One reference to a shared box.
pub struct Shared<T>(*mut T);

impl<T> Shared<T> {
    /// A new reference to `p`.
    ///
    /// # Safety
    /// See `retain_shared`.
    pub unsafe fn retain(p: *mut T) -> Self {
        Shared(retain_shared(p))
    }

    /// Takes over the reference `p` has, as `new_shared` returns it.
    ///
    /// # Safety
    /// `p` comes from `new_shared`, and that reference isn't used again.
    pub unsafe fn adopt(p: *mut T) -> Self {
        Shared(p)
    }

    /// A new box with one reference, held by this.
    pub fn new(v: T) -> Self {
        Shared(new_shared(v))
    }

    pub fn as_ptr(&self) -> *mut T {
        self.0
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        // SAFETY: this reference keeps the box alive.
        unsafe { Shared::retain(self.0) }
    }
}

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        // SAFETY: this reference isn't used again.
        unsafe { release_shared(self.0) }
    }
}

impl<T> std::ops::Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: this reference keeps the box alive.
        unsafe { &*self.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Counted<'a>(&'a Cell<i32>);

    impl Drop for Counted<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn the_last_reference_frees_the_box() {
        let dropped = Cell::new(0);
        let p = new_shared(Counted(&dropped));
        let a = unsafe { Shared::retain(p) };
        let b = a.clone();
        assert_eq!(b.as_ptr(), p);
        unsafe { release_shared(p) };
        drop(a);
        assert_eq!(dropped.get(), 0);
        assert!(std::ptr::eq(b.0, &*b));
        drop(b);
        assert_eq!(dropped.get(), 1);
    }
}
