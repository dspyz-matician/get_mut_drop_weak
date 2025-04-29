use std::ptr;
use std::sync::Arc;

use get_mut_drop_weak::get_mut_drop_weak;

#[test]
fn test_exclusive_access_no_weak() {
    // Scenario: Strong count = 1, Weak count = 0
    let mut arc = Arc::new(10);
    let original_ptr = Arc::as_ptr(&arc);

    // Action
    let result = get_mut_drop_weak(&mut arc);

    // Verification
    let val_mut = result.unwrap(); // Expect Ok, panic if Err
    assert_eq!(*val_mut, 10);

    // Modify
    *val_mut = 20;

    // Check modification and state
    assert_eq!(*arc, 20);
    assert_eq!(Arc::strong_count(&arc), 1);
    assert_eq!(Arc::weak_count(&arc), 0);
    // Ensure the Arc instance itself wasn't replaced
    assert_eq!(Arc::as_ptr(&arc), original_ptr);
}

#[test]
fn test_strong_shared_no_mut() {
    // Scenario: Strong count > 1
    let mut arc1 = Arc::new(String::from("hello"));
    let arc2 = Arc::clone(&arc1);
    let original_ptr = Arc::as_ptr(&arc1);

    assert_eq!(Arc::strong_count(&arc1), 2);

    // Action
    let result = get_mut_drop_weak(&mut arc1);

    // Verification
    let err_ref = result.unwrap_err(); // Expect Err, panic if Ok
    assert!(ptr::eq(err_ref, &arc1)); // Check the returned ref is the input ref

    // Check state hasn't changed
    assert_eq!(*arc1, "hello");
    assert_eq!(*arc2, "hello");
    assert_eq!(Arc::strong_count(&arc1), 2);
    assert_eq!(Arc::weak_count(&arc1), 0);
    // Ensure the Arc instance itself wasn't replaced
    assert_eq!(Arc::as_ptr(&arc1), original_ptr);

    // Drop the second reference to allow cleanup
    drop(arc2);
    assert_eq!(Arc::strong_count(&arc1), 1);
}

#[test]
fn test_strong_shared_with_weak_no_mut() {
    // Scenario: Strong count > 1, Weak count > 0
    let mut arc1 = Arc::new(vec![1, 2, 3]);
    let arc2 = Arc::clone(&arc1);
    let weak1 = Arc::downgrade(&arc1);
    let original_ptr = Arc::as_ptr(&arc1);

    assert_eq!(Arc::strong_count(&arc1), 2);
    assert_eq!(Arc::weak_count(&arc1), 1);
    assert!(weak1.upgrade().is_some());

    // Action
    let result = get_mut_drop_weak(&mut arc1);

    // Verification
    let err_ref = result.unwrap_err(); // Expect Err, panic if Ok
    assert!(ptr::eq(err_ref, &arc1));

    // Check state hasn't changed
    assert_eq!(*arc1, vec![1, 2, 3]);
    assert_eq!(*arc2, vec![1, 2, 3]);
    assert_eq!(Arc::strong_count(&arc1), 2);
    assert_eq!(Arc::weak_count(&arc1), 1);
    assert!(weak1.upgrade().is_some()); // Weak pointer still valid
    // Ensure the Arc instance itself wasn't replaced
    assert_eq!(Arc::as_ptr(&arc1), original_ptr);

    // Drop the second reference
    drop(arc2);
    assert_eq!(Arc::strong_count(&arc1), 1);
    assert_eq!(Arc::weak_count(&arc1), 1); // Weak count remains
}

#[test]
fn test_weak_shared_drops_weak_success() {
    // Scenario: Strong count = 1, Weak count > 0
    #[derive(Debug, PartialEq)]
    struct TestData {
        value: i32,
    }
    let mut arc = Arc::new(TestData { value: 50 });
    let weak1 = Arc::downgrade(&arc);
    let weak2 = Arc::downgrade(&arc);
    let original_ptr = Arc::as_ptr(&arc);

    assert_eq!(Arc::strong_count(&arc), 1);
    assert_eq!(Arc::weak_count(&arc), 2);
    assert!(weak1.upgrade().is_some());
    assert!(weak2.upgrade().is_some());

    // Action
    let result = get_mut_drop_weak(&mut arc);

    // Verification
    let val_mut = result.unwrap(); // Expect Ok, panic if Err
    assert_eq!(val_mut.value, 50);

    // Modify
    val_mut.value = 60;

    // Check modification and state
    assert_eq!(arc.value, 60);
    assert_eq!(Arc::strong_count(&arc), 1);
    // The weak count refers *to the new Arc instance*, which has no weak refs yet.
    assert_eq!(Arc::weak_count(&arc), 0);

    // Crucially, verify the Arc instance was replaced
    let new_ptr = Arc::as_ptr(&arc);
    assert_ne!(new_ptr, original_ptr);

    // Verify the old weak pointers are now dangling
    assert!(weak1.upgrade().is_none());
    assert!(weak2.upgrade().is_none());
}

// Helper struct for drop testing
#[derive(Debug)]
struct DropTracker<'a> {
    id: i32,
    dropped: &'a std::sync::atomic::AtomicBool,
}

impl<'a> Drop for DropTracker<'a> {
    fn drop(&mut self) {
        println!("Dropping DropTracker id: {}", self.id);
        self.dropped
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[test]
fn test_weak_shared_drops_weak_with_drop_impl() {
    // Scenario: Strong=1, Weak > 0, with type implementing Drop
    let dropped_flag = std::sync::atomic::AtomicBool::new(false);
    let data = DropTracker {
        id: 1,
        dropped: &dropped_flag,
    };

    let mut arc = Arc::new(data);
    let weak = Arc::downgrade(&arc);
    let original_ptr = Arc::as_ptr(&arc);

    assert_eq!(Arc::strong_count(&arc), 1);
    assert_eq!(Arc::weak_count(&arc), 1);
    assert!(!dropped_flag.load(std::sync::atomic::Ordering::SeqCst)); // Not dropped yet

    // Action
    let result = get_mut_drop_weak(&mut arc);

    // Verification
    let val_mut = result.unwrap();
    val_mut.id = 2; // Modify data

    assert_eq!(val_mut.id, 2);
    assert_eq!(Arc::strong_count(&arc), 1);
    assert_eq!(Arc::weak_count(&arc), 0); // New Arc has no weak ptrs
    assert_ne!(Arc::as_ptr(&arc), original_ptr); // Instance replaced
    assert!(weak.upgrade().is_none()); // Old weak pointer is dangling
    assert!(!dropped_flag.load(std::sync::atomic::Ordering::SeqCst)); // Data should not have been dropped

    // Drop the final Arc, triggering the Drop impl
    drop(arc);
    assert!(dropped_flag.load(std::sync::atomic::Ordering::SeqCst)); // Now it should be dropped
}

#[test]
fn simple_multithreaded() {
    use std::{
        sync::{Arc, Barrier},
        thread,
    };

    const NUM_WEAK_JOBS: usize = 2;

    let arc = Arc::new(Box::new(42usize));
    let barrier = Barrier::new(NUM_WEAK_JOBS + 1);

    thread::scope(|s| {
        for _ in 0..NUM_WEAK_JOBS {
            let arc = arc.clone();
            s.spawn(|| {
                let weak = Arc::downgrade(&arc);
                assert!(weak.upgrade().is_some());
                barrier.wait(); // a
                barrier.wait(); // b
                drop(arc);
                barrier.wait(); // c
                barrier.wait(); // d
                assert!(weak.upgrade().is_none());
            });
        }

        let mut arc = arc;
        let barrier = &barrier;
        s.spawn(move || {
            barrier.wait(); // a
            get_mut_drop_weak(&mut arc).unwrap_err();
            barrier.wait(); // b
            barrier.wait(); // c
            let b = get_mut_drop_weak(&mut arc).unwrap();
            **b += 1;
            assert_eq!(**arc, 43);
            barrier.wait(); // d
        })
        .join()
        .unwrap();
    });
}
