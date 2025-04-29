use std::{mem::MaybeUninit, ptr, sync::Arc};

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
//
// # Safety Notes
// This function uses unsafe code internally to handle the Arc replacement
// while aiming to be panic-safe *after* the initial allocation check.
// It relies on ptr::read/write and careful state management.
pub fn get_mut_drop_weak<T>(arc: &mut Arc<T>) -> Result<&mut T, &mut Arc<T>> {
    // Handle easy cases first without allocation or unsafe code
    if Arc::get_mut(arc).is_some() {
        // Strong=1, Weak=0. Already exclusive.
        // Need to call it again to get the reference with the right lifetime.
        return Ok(Arc::get_mut(arc).unwrap());
    }
    if Arc::strong_count(arc) > 1 {
        // Strong > 1. Cannot get exclusive access.
        return Err(arc);
    }

    // State: Strong = 1, Weak > 0. Need to replace the Arc instance.

    // --- Potentially panicking allocation happens here ---
    // Pre-allocate storage for the new Arc. If this fails, we panic *before*
    // entering the unsafe block or modifying `arc`, which is safe for the caller.
    let mut preallocated_arc: Arc<MaybeUninit<T>> = Arc::new(MaybeUninit::uninit());
    // --- Allocation succeeded ---

    // Unsafe block to perform the swap without panicking mid-state-change.
    unsafe {
        // Read the original Arc out, leaving `arc` pointing to invalid memory temporarily.
        let original_arc = ptr::read(arc as *mut Arc<T>);

        // Consume the original Arc to get the value. Should succeed unless another thread
        // upgraded a weak reference to a strong one in parallel.
        match Arc::try_unwrap(original_arc) {
            Ok(value) => {
                // Got the value, old weak pointers are now orphaned.

                // Initialize the pre-allocated memory.
                // get_mut is guaranteed safe because preallocated_arc count is 1.
                let slot = Arc::get_mut(&mut preallocated_arc).unwrap();
                slot.write(value); // Moves value, initializes memory.

                // Convert Arc<MaybeUninit<T>> -> Arc<T>
                let final_arc = {
                    let raw_ptr = Arc::into_raw(preallocated_arc).cast::<T>();
                    // SAFETY: We just initialized the value at raw_ptr via slot.write.
                    Arc::from_raw(raw_ptr)
                };
                // `preallocated_arc` is now consumed.

                // Write the new Arc<T> back into the user's reference location.
                ptr::write(arc, final_arc); // Consumes final_arc.

                // Return mutable reference from the new Arc. Guaranteed safe.
                // SAFETY: We just wrote a valid Arc<T> to `arc`.
                Ok(Arc::get_mut(arc).unwrap())
            }
            Err(restored_arc) => {
                // Failed to unwrap, meaning another thread upgraded a weak reference.
                ptr::write(arc, restored_arc); // Consumes restored_arc.
                Err(arc) // Indicate failure.
            }
        }
    }
}
