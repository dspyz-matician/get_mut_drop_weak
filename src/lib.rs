use std::{
    mem::ManuallyDrop,
    sync::{Arc, Weak},
};

/// Attempts to get a mutable reference to the inner data of an Arc.
///
/// If the Arc has a strong count of 1 and a weak count of 0, it returns
/// the mutable reference directly.
///
/// If the Arc has a strong count greater than 1, it returns None.
///
/// If the Arc has a strong count of 1 and a weak count greater than 0,
/// it attempts to replace the Arc instance with a new one containing the
/// same data, effectively invalidating all existing weak pointers. This
/// involves an internal allocation for the new Arc instance. If this
/// allocation fails, the function will panic (before modifying the input Arc).
///
/// Returns Ok(&mut T) on success, or Err(&mut Arc<T>) if the strong count was
/// greater than 1.
///
/// The Err variant is useful for the caller to avoid borrow-checker issues
/// due to rust's lack of non-lexical lifetimes. That is, if the caller
/// only has a mutable reference to the Arc, they may not be able to reborrow
/// it when calling this function if they want to return a mutable reference
/// to the inner data. Thus, if the function fails, they may have "lost" the
/// only reference they had. The Err variant gives it back so they can try
/// something else.
///
/// (See https://rust-lang.github.io/rfcs/2094-nll.html#problem-case-2-conditional-control-flow)
pub fn get_mut_drop_weak<T>(this: &mut Arc<T>) -> Result<&mut T, &mut Arc<T>> {
    /// Use [`Arc::get_mut_unchecked`] when stable.
    ///
    /// ```compile_fail
    /// use std::sync::Arc;
    /// let mut a = Arc::new(0usize);
    /// let b = unsafe { Arc::get_mut_unchecked(&mut a) };
    /// *b += 1;
    /// ```
    unsafe fn get_mut_unchecked<T>(this: &mut Arc<T>) -> &mut T {
        let ptr = Arc::as_ptr(this);
        unsafe { &mut *ptr.cast_mut() }
    }

    if Arc::get_mut(this).is_some() {
        return Ok(unsafe { get_mut_unchecked(this) });
    }

    let weak = Arc::downgrade(this);

    unsafe {
        let bitcopied = std::ptr::read(this);

        match Arc::try_unwrap(bitcopied) {
            Ok(inner) => {
                struct ReInitThroughWeak<T> {
                    weak: ManuallyDrop<Weak<T>>,
                    inner: ManuallyDrop<T>,
                }
                impl<T> ReInitThroughWeak<T> {
                    unsafe fn new(weak: Weak<T>, inner: T) -> Self {
                        Self {
                            weak: ManuallyDrop::new(weak),
                            inner: ManuallyDrop::new(inner),
                        }
                    }

                    fn defuse(self) -> T {
                        let mut this = ManuallyDrop::new(self);
                        unsafe { ManuallyDrop::drop(&mut this.weak) };
                        unsafe { ManuallyDrop::take(&mut this.inner) }
                    }
                }
                impl<T> Drop for ReInitThroughWeak<T> {
                    fn drop(&mut self) {
                        let ptr = self.weak.as_ptr();
                        let inner = unsafe { ManuallyDrop::take(&mut self.inner) };
                        unsafe { std::ptr::write(ptr.cast_mut(), inner) };
                        unsafe { Arc::increment_strong_count(ptr) };
                    }
                }

                let reinit_guard = ReInitThroughWeak::new(weak, inner);
                let mut alloc = Arc::new_uninit();
                let inner = reinit_guard.defuse();

                get_mut_unchecked(&mut alloc).write(inner);
                let initialized = alloc.assume_init();

                std::ptr::write(this, initialized);
                Ok(get_mut_unchecked(this))
            }
            Err(bitcopied) => {
                std::ptr::write(this, bitcopied);
                Err(this)
            }
        }
    }
}
