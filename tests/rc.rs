use std::cell::Cell;
use std::ptr;
use std::rc::Rc;

use get_mut_drop_weak::rc::get_mut_drop_weak;

#[test]
fn test_exclusive_access_no_weak() {
    // Scenario: Strong count = 1, Weak count = 0
    let mut rc = Rc::new(10);
    let original_ptr = Rc::as_ptr(&rc);

    // Action
    let result = get_mut_drop_weak(&mut rc);

    // Verification
    let val_mut = result.unwrap(); // Expect Ok, panic if Err
    assert_eq!(*val_mut, 10);

    // Modify
    *val_mut = 20;

    // Check modification and state
    assert_eq!(*rc, 20);
    assert_eq!(Rc::strong_count(&rc), 1);
    assert_eq!(Rc::weak_count(&rc), 0);
    // Ensure the Rc instance itself wasn't replaced
    assert_eq!(Rc::as_ptr(&rc), original_ptr);
}

#[test]
fn test_strong_shared_no_mut() {
    // Scenario: Strong count > 1
    let mut rc1 = Rc::new(String::from("hello"));
    let rc2 = Rc::clone(&rc1);
    let original_ptr = Rc::as_ptr(&rc1);

    assert_eq!(Rc::strong_count(&rc1), 2);

    // Action
    let result = get_mut_drop_weak(&mut rc1);

    // Verification
    let err_ref = result.unwrap_err(); // Expect Err, panic if Ok
    assert!(ptr::eq(err_ref, &rc1)); // Check the returned ref is the input ref

    // Check state hasn't changed
    assert_eq!(*rc1, "hello");
    assert_eq!(*rc2, "hello");
    assert_eq!(Rc::strong_count(&rc1), 2);
    assert_eq!(Rc::weak_count(&rc1), 0);
    // Ensure the Rc instance itself wasn't replaced
    assert_eq!(Rc::as_ptr(&rc1), original_ptr);

    // Drop the second reference to allow cleanup
    drop(rc2);
    assert_eq!(Rc::strong_count(&rc1), 1);
}

#[test]
fn test_strong_shared_with_weak_no_mut() {
    // Scenario: Strong count > 1, Weak count > 0
    let mut rc1 = Rc::new(vec![1, 2, 3]);
    let rc2 = Rc::clone(&rc1);
    let weak1 = Rc::downgrade(&rc1);
    let original_ptr = Rc::as_ptr(&rc1);

    assert_eq!(Rc::strong_count(&rc1), 2);
    assert_eq!(Rc::weak_count(&rc1), 1);
    assert!(weak1.upgrade().is_some());

    // Action
    let result = get_mut_drop_weak(&mut rc1);

    // Verification
    let err_ref = result.unwrap_err(); // Expect Err, panic if Ok
    assert!(ptr::eq(err_ref, &rc1));

    // Check state hasn't changed
    assert_eq!(*rc1, vec![1, 2, 3]);
    assert_eq!(*rc2, vec![1, 2, 3]);
    assert_eq!(Rc::strong_count(&rc1), 2);
    assert_eq!(Rc::weak_count(&rc1), 1);
    assert!(weak1.upgrade().is_some()); // Weak pointer still valid
    // Ensure the Rc instance itself wasn't replaced
    assert_eq!(Rc::as_ptr(&rc1), original_ptr);

    // Drop the second reference
    drop(rc2);
    assert_eq!(Rc::strong_count(&rc1), 1);
    assert_eq!(Rc::weak_count(&rc1), 1); // Weak count remains
}

#[test]
fn test_weak_shared_drops_weak_success() {
    // Scenario: Strong count = 1, Weak count > 0
    #[derive(Debug, PartialEq)]
    struct TestData {
        value: i32,
    }
    let mut rc = Rc::new(TestData { value: 50 });
    let weak1 = Rc::downgrade(&rc);
    let weak2 = Rc::downgrade(&rc);
    let original_ptr = Rc::as_ptr(&rc);

    assert_eq!(Rc::strong_count(&rc), 1);
    assert_eq!(Rc::weak_count(&rc), 2);
    assert!(weak1.upgrade().is_some());
    assert!(weak2.upgrade().is_some());

    // Action
    let result = get_mut_drop_weak(&mut rc);

    // Verification
    let val_mut = result.unwrap(); // Expect Ok, panic if Err
    assert_eq!(val_mut.value, 50);

    // Modify
    val_mut.value = 60;

    // Check modification and state
    assert_eq!(rc.value, 60);
    assert_eq!(Rc::strong_count(&rc), 1);
    // The weak count refers *to the new Rc instance*, which has no weak refs yet.
    assert_eq!(Rc::weak_count(&rc), 0);

    // Crucially, verify the Rc instance was replaced
    let new_ptr = Rc::as_ptr(&rc);
    assert_ne!(new_ptr, original_ptr);

    // Verify the old weak pointers are now dangling
    assert!(weak1.upgrade().is_none());
    assert!(weak2.upgrade().is_none());
}

// Helper struct for drop testing
#[derive(Debug)]
struct DropTracker<'a> {
    id: i32,
    dropped: &'a Cell<bool>,
}

impl<'a> Drop for DropTracker<'a> {
    fn drop(&mut self) {
        println!("Dropping DropTracker id: {}", self.id);
        self.dropped.set(true);
    }
}

#[test]
fn test_weak_shared_drops_weak_with_drop_impl() {
    // Scenario: Strong=1, Weak > 0, with type implementing Drop
    let dropped_flag = Cell::new(false);
    let data = DropTracker {
        id: 1,
        dropped: &dropped_flag,
    };

    let mut rc = Rc::new(data);
    let weak = Rc::downgrade(&rc);
    let original_ptr = Rc::as_ptr(&rc);

    assert_eq!(Rc::strong_count(&rc), 1);
    assert_eq!(Rc::weak_count(&rc), 1);
    assert!(!dropped_flag.get()); // Not dropped yet

    // Action
    let result = get_mut_drop_weak(&mut rc);

    // Verification
    let val_mut = result.unwrap();
    val_mut.id = 2; // Modify data

    assert_eq!(val_mut.id, 2);
    assert_eq!(Rc::strong_count(&rc), 1);
    assert_eq!(Rc::weak_count(&rc), 0); // New Rc has no weak ptrs
    assert_ne!(Rc::as_ptr(&rc), original_ptr); // Instance replaced
    assert!(weak.upgrade().is_none()); // Old weak pointer is dangling
    assert!(!dropped_flag.get()); // Data should not have been dropped

    // Drop the final Rc, triggering the Drop impl
    drop(rc);
    assert!(dropped_flag.get()); // Now it should be dropped
}
