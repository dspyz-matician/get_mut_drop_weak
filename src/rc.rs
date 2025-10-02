use std::{mem::MaybeUninit, ptr, rc::Rc};

/// Attempts to get a mutable reference to the inner data of an Rc.
///
/// If the Rc has a strong count of 1 and a weak count of 0, it returns
/// the mutable reference directly.
///
/// If the Rc has a strong count greater than 1, it returns None.
///
/// If the Rc has a strong count of 1 and a weak count greater than 0,
/// it attempts to replace the Rc instance with a new one containing the
/// same data, effectively invalidating all existing weak pointers. This
/// involves an internal allocation for the new Rc instance. If this
/// allocation fails, the function will panic (before modifying the input Rc).
///
/// Returns Ok(&mut T) on success, or Err(&mut Rc<T>) if the strong count was
/// greater than 1.
///
/// The Err variant is useful for the caller to avoid borrow-checker issues
/// due to rust's lack of non-lexical lifetimes. That is, if the caller
/// only has a mutable reference to the Rc, they may not be able to reborrow
/// it when calling this function if they want to return a mutable reference
/// to the inner data. Thus, if the function fails, they may have "lost" the
/// only reference they had. The Err variant gives it back so they can try
/// something else.
///
/// (See https://rust-lang.github.io/rfcs/2094-nll.html#problem-case-2-conditional-control-flow)
//
// # Safety Notes
// This function uses unsafe code internally to handle the Rc replacement
// while aiming to be panic-safe *after* the initial allocation check.
// It relies on ptr::read/write and careful state management.
pub fn get_mut_drop_weak<T>(rc: &mut Rc<T>) -> Result<&mut T, &mut Rc<T>> {
    // Handle easy cases first without allocation
    if Rc::get_mut(rc).is_some() {
        // Strong=1, Weak=0. Already exclusive.
        // Need to call it again to get the reference with the right lifetime.
        return Ok(unsafe { get_mut_unchecked(rc) });
    }
    if Rc::strong_count(rc) > 1 {
        // Strong > 1. Cannot get exclusive access.
        return Err(rc);
    }

    // State: Strong = 1, Weak > 0. Need to replace the Rc instance.

    // --- Potentially panicking allocation happens here ---
    // Pre-allocate storage for the new Rc. If this fails, we panic *before*
    // entering the unsafe block or modifying `rc`, which is safe for the caller.
    let mut preallocated_rc: Rc<MaybeUninit<T>> = Rc::new_uninit();
    // --- Allocation succeeded ---

    let rc_ptr = ptr::from_mut(rc);

    // Unsafe block to perform the swap without panicking mid-state-change.
    unsafe {
        // Read the original Rc out, leaving `rc` pointing to invalid memory temporarily.
        let original_rc = ptr::read(rc_ptr);

        // Consume the original Rc to get the value. Since Rcs can't be shared between threads and
        // we checked the strong count is 1, this will always succeed.
        let value = Rc::try_unwrap(original_rc).unwrap_or_else(|_| {
            unreachable!("Rc::try_unwrap failed: strong count was 1 and now isn't")
        });

        // Got the value, old weak pointers are now orphaned.

        // Initialize the pre-allocated memory.
        // get_mut is guaranteed safe because preallocated_rc count is 1.
        let slot = get_mut_unchecked(&mut preallocated_rc);
        slot.write(value); // Moves value, initializes memory.

        // Convert Rc<MaybeUninit<T>> -> Rc<T>
        let final_rc = preallocated_rc.assume_init();
        // `preallocated_rc` is now consumed.

        // Write the new Rc<T> back into the user's reference location.
        ptr::write(rc_ptr, final_rc); // Consumes final_rc.

        // Return mutable reference from the new Rc. Guaranteed safe.
        // SAFETY: We just wrote a valid Rc<T> to `rc`.
        Ok(get_mut_unchecked(rc))
    }
}

/// Use [`Rc::get_mut_unchecked`] when stable.
///
/// ```compile_fail
/// use std::sync::Rc;
/// let mut a = Rc::new(0usize);
/// let b = unsafe { Rc::get_mut_unchecked(&mut a) };
/// *b += 1;
/// ```
unsafe fn get_mut_unchecked<T>(this: &mut Rc<T>) -> &mut T {
    let ptr = Rc::as_ptr(this);
    unsafe { &mut *ptr.cast_mut() }
}
